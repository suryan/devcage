//! Connector isolation mode: box one MCP server in the client launch chain.
//!
//! The agent stays on the host. Operators insert `devcage mcp` only for the
//! connectors they choose, the same way `mcp-guard` and `mcp-secret-launcher`
//! are chained. The server keeps secrets injected for its profile. It does not
//! get Keychain, the SSH agent, or the rest of the host home.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use crate::bwrap_plan::{mcp_bwrap_argv, McpBindMode, McpBwrapPlan, McpUrlBridge};
use crate::config::{resolve_workspace, Config};
use crate::url_bridge::{
    connector_label, prepend_path_entry, start_sign_in_bridge, ChildBridgePaths, OpenUrls,
    UrlBridge, DESKTOP_SESSION_VARS,
};
use crate::util::which;

/// Environment variables removed so the SSH agent stays outside the cage.
pub const SCRUB_ENV: &[&str] = &["SSH_AUTH_SOCK", "SSH_AGENT_PID"];

/// How much of the project workspace the connector may see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceShare {
    /// Project is not mounted.
    None,
    /// Project is visible and not writable.
    ReadOnly,
    /// Project is visible and writable.
    ReadWrite,
}

impl WorkspaceShare {
    /// Parse `--share-workspace`.
    ///
    /// # Errors
    ///
    /// The value is not `none`, `ro`, or `rw`.
    pub fn parse(raw: &str) -> anyhow::Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "none" => Ok(Self::None),
            "ro" | "read-only" => Ok(Self::ReadOnly),
            "rw" | "read-write" => Ok(Self::ReadWrite),
            other => anyhow::bail!("--share-workspace must be none, ro, or rw (got `{other}`)"),
        }
    }

    /// Label for plans and docs.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ReadOnly => "ro",
            Self::ReadWrite => "rw",
        }
    }
}

/// Which OS sandbox is available for a light wrap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsSandbox {
    /// macOS `sandbox-exec`.
    Seatbelt,
    /// Linux `bwrap`.
    Bwrap,
    /// Neither tool is on `PATH`.
    None,
}

/// How `devcage mcp` will run the connector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    /// `sandbox-exec` profile.
    Seatbelt,
    /// `bwrap` namespaces.
    Bwrap,
    /// Env remap only. Used when the operator passes `--allow-env-only`.
    EnvOnly,
    /// Docker or Podman (`--container`).
    Container,
}

impl LaunchMode {
    /// Short label for `--print` and verbose stderr.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Seatbelt => "seatbelt",
            Self::Bwrap => "bwrap",
            Self::EnvOnly => "env-only",
            Self::Container => "container",
        }
    }
}

/// CLI inputs for [`run_mcp`].
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct McpParams {
    /// Print the plan and do not start the connector.
    pub print_only: bool,
    /// Explain the cage on stderr. Stdout stays the connector's stream.
    pub verbose: bool,
    /// Run inside Docker/Podman instead of a light wrap.
    pub container: bool,
    /// Start even when `sandbox-exec` and `bwrap` are missing.
    pub allow_env_only: bool,
    /// Forward sign-in links to the host. Off unless `--open-urls` is set.
    pub open_urls: OpenUrls,
    /// `allowed` or `none`.
    pub net: String,
    /// `none`, `ro`, or `rw`.
    pub share_workspace: String,
    /// Workspace override when sharing the project.
    pub workspace: Option<PathBuf>,
    /// Connector argv.
    pub command: Vec<String>,
}

/// Resolved policy before any process starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpSettings {
    /// Deny network when true.
    pub net_none: bool,
    /// Workspace exposure.
    pub share: WorkspaceShare,
    /// Use a container venue.
    pub container: bool,
    /// Permit env-only when no OS sandbox exists.
    pub allow_env_only: bool,
    /// Sign-in URL bridge.
    pub open_urls: OpenUrls,
    /// Resolved project directory when `share` is not [`WorkspaceShare::None`].
    pub workspace: Option<PathBuf>,
    /// Connector argv.
    pub command: Vec<String>,
}

impl McpSettings {
    /// Parse and validate CLI params.
    ///
    /// # Errors
    ///
    /// Empty command, unknown net/share, or a workspace that is not a directory.
    pub fn from_params(params: &McpParams) -> anyhow::Result<Self> {
        if params.command.is_empty() {
            anyhow::bail!(
                "usage: devcage mcp -- <command> [args…]\nexample: devcage mcp -- uvx my-server==1.2.3"
            );
        }
        if params.container && params.open_urls.is_enabled() {
            anyhow::bail!("--open-urls is not available with --container");
        }
        let net_none = parse_net(&params.net)?;
        let share = WorkspaceShare::parse(&params.share_workspace)?;
        let workspace = if share == WorkspaceShare::None {
            None
        } else {
            Some(resolve_workspace(params.workspace.clone())?)
        };
        Ok(Self {
            net_none,
            share,
            container: params.container,
            allow_env_only: params.allow_env_only,
            open_urls: params.open_urls,
            workspace,
            command: params.command.clone(),
        })
    }
}

/// Parse `--net`.
///
/// # Errors
///
/// The value is not `allowed` or `none`.
pub fn parse_net(raw: &str) -> anyhow::Result<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "allowed" => Ok(false),
        "none" => Ok(true),
        other => anyhow::bail!("--net must be allowed or none (got `{other}`)"),
    }
}

/// True when this variable is the SSH agent and must not enter the cage.
#[must_use]
pub fn should_scrub(key: &str) -> bool {
    SCRUB_ENV.contains(&key)
}

/// Inputs for the host pkg-guard cache path (no ambient env reads).
#[derive(Debug, Clone, Copy)]
pub struct HostCacheInputs<'a> {
    /// `PKG_GUARD_CACHE_DIR` when the parent set it.
    pub pkg_guard_cache_dir: Option<&'a str>,
    /// `XDG_CACHE_HOME` when the parent set it.
    pub xdg_cache_home: Option<&'a str>,
    /// Host home before it is remapped.
    pub home: &'a Path,
}

/// Host pkg-guard cache. Kept outside the ephemeral home so OSV data stays readable.
#[must_use]
pub fn host_pkg_guard_cache(inputs: HostCacheInputs<'_>) -> PathBuf {
    if let Some(dir) = nonempty(inputs.pkg_guard_cache_dir) {
        return PathBuf::from(dir);
    }
    if let Some(xdg) = nonempty(inputs.xdg_cache_home) {
        return PathBuf::from(xdg).join("pkg-guard");
    }
    inputs.home.join(".cache").join("pkg-guard")
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
}

/// Child cache and home paths for a light wrap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpChildEnv {
    /// Ephemeral `HOME`.
    pub home: PathBuf,
    /// `XDG_CACHE_HOME`.
    pub xdg_cache_home: PathBuf,
    /// `XDG_CONFIG_HOME`.
    pub xdg_config_home: PathBuf,
    /// `XDG_DATA_HOME`.
    pub xdg_data_home: PathBuf,
    /// `XDG_STATE_HOME`.
    pub xdg_state_home: PathBuf,
    /// `UV_CACHE_DIR`.
    pub uv_cache_dir: PathBuf,
    /// `UV_TOOL_DIR`.
    pub uv_tool_dir: PathBuf,
    /// `NPM_CONFIG_CACHE`.
    pub npm_cache: PathBuf,
    /// Host pkg-guard cache (read-only mount).
    pub pkg_guard_cache_dir: PathBuf,
    /// Network label exported as `DEVCAGE_MCP_NET`.
    pub net_none: bool,
}

impl McpChildEnv {
    /// Layout under an ephemeral home directory.
    #[must_use]
    pub fn from_ephemeral(ephemeral_home: &Path, pkg_guard_cache: &Path, net_none: bool) -> Self {
        let cache = ephemeral_home.join(".cache");
        Self {
            home: ephemeral_home.to_path_buf(),
            xdg_cache_home: cache.clone(),
            xdg_config_home: ephemeral_home.join(".config"),
            xdg_data_home: ephemeral_home.join(".local").join("share"),
            xdg_state_home: ephemeral_home.join(".local").join("state"),
            uv_cache_dir: cache.join("uv"),
            uv_tool_dir: ephemeral_home
                .join(".local")
                .join("share")
                .join("uv")
                .join("tools"),
            npm_cache: cache.join("npm"),
            pkg_guard_cache_dir: pkg_guard_cache.to_path_buf(),
            net_none,
        }
    }

    /// Assignments applied on top of the inherited environment.
    ///
    /// Injected API keys are not listed here and are not removed.
    #[must_use]
    pub fn assignments(&self) -> Vec<(String, String)> {
        vec![
            ("HOME".to_owned(), self.home.display().to_string()),
            (
                "XDG_CACHE_HOME".to_owned(),
                self.xdg_cache_home.display().to_string(),
            ),
            (
                "XDG_CONFIG_HOME".to_owned(),
                self.xdg_config_home.display().to_string(),
            ),
            (
                "XDG_DATA_HOME".to_owned(),
                self.xdg_data_home.display().to_string(),
            ),
            (
                "XDG_STATE_HOME".to_owned(),
                self.xdg_state_home.display().to_string(),
            ),
            (
                "UV_CACHE_DIR".to_owned(),
                self.uv_cache_dir.display().to_string(),
            ),
            (
                "UV_TOOL_DIR".to_owned(),
                self.uv_tool_dir.display().to_string(),
            ),
            (
                "NPM_CONFIG_CACHE".to_owned(),
                self.npm_cache.display().to_string(),
            ),
            (
                "PKG_GUARD_CACHE_DIR".to_owned(),
                self.pkg_guard_cache_dir.display().to_string(),
            ),
            (
                "PKG_GUARD_TRANSITIVE_CACHE_TTL_SECS".to_owned(),
                "0".to_owned(),
            ),
            ("DEVCAGE_MODE".to_owned(), "mcp".to_owned()),
            (
                "DEVCAGE_MCP_NET".to_owned(),
                if self.net_none { "none" } else { "allowed" }.to_owned(),
            ),
        ]
    }

    /// Create the ephemeral directories the connector is allowed to write.
    ///
    /// # Errors
    ///
    /// A directory cannot be created.
    pub fn create_dirs(&self) -> anyhow::Result<()> {
        for dir in [
            self.home.as_path(),
            self.xdg_cache_home.as_path(),
            self.xdg_config_home.as_path(),
            self.xdg_data_home.as_path(),
            self.xdg_state_home.as_path(),
            self.uv_cache_dir.as_path(),
            self.uv_tool_dir.as_path(),
            self.npm_cache.as_path(),
        ] {
            fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

/// Environment passed into a container guest.
///
/// `PATH` is left to the image. `DYLD_*` is host-only and is not forwarded.
/// Injected secrets other than the SSH agent are kept.
#[must_use]
pub fn container_env_pairs(
    parent: impl IntoIterator<Item = (String, String)>,
    guest: &McpChildEnv,
) -> Vec<(String, String)> {
    let mut map = BTreeMap::new();
    for (key, value) in parent {
        if key.is_empty() || should_scrub(&key) || key == "PATH" || key.starts_with("DYLD_") {
            continue;
        }
        map.insert(key, value);
    }
    for (key, value) in guest.assignments() {
        map.insert(key, value);
    }
    map.into_iter().collect()
}

/// Filesystem allow-list for the Seatbelt profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpFsPlan {
    /// Readable subpaths.
    pub read: Vec<PathBuf>,
    /// Writable subpaths.
    pub write: Vec<PathBuf>,
}

/// Build the Seatbelt filesystem allow-list.
#[must_use]
pub fn mcp_fs_plan(
    ephemeral_home: &Path,
    workspace: Option<(&Path, WorkspaceShare)>,
    read_extra: &[PathBuf],
) -> McpFsPlan {
    let mut read = toolchain_read_roots();
    read.push(ephemeral_home.to_path_buf());
    for extra in read_extra {
        push_unique(&mut read, extra);
    }
    let mut write = vec![
        ephemeral_home.to_path_buf(),
        PathBuf::from("/tmp"),
        PathBuf::from("/private/tmp"),
        PathBuf::from("/var/folders"),
        PathBuf::from("/private/var/folders"),
        PathBuf::from("/dev"),
    ];
    if let Some((path, share)) = workspace {
        match share {
            WorkspaceShare::None => {},
            WorkspaceShare::ReadOnly => push_unique(&mut read, path),
            WorkspaceShare::ReadWrite => {
                push_unique(&mut read, path);
                push_unique(&mut write, path);
            },
        }
    }
    McpFsPlan { read, write }
}

fn toolchain_read_roots() -> Vec<PathBuf> {
    [
        "/usr",
        "/System",
        "/Library",
        "/bin",
        "/sbin",
        "/opt",
        "/opt/homebrew",
        "/usr/local",
        "/dev",
        "/private/etc",
        "/private/tmp",
        "/tmp",
        "/var/folders",
        "/private/var/folders",
        "/private/var",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

fn push_unique(out: &mut Vec<PathBuf>, path: &Path) {
    if !out.iter().any(|existing| existing == path) {
        out.push(path.to_path_buf());
    }
}

/// Secret directories that stay unreadable even if a broader allow matches.
#[must_use]
pub fn secret_paths(home: &Path) -> Vec<PathBuf> {
    [".ssh", ".aws", ".gnupg", "Library/Keychains"]
        .into_iter()
        .map(|rel| home.join(rel))
        .collect()
}

/// Render a Seatbelt profile. Denies of Keychain lookup and secret paths come last.
///
/// `(literal "/")` allows reading the root directory node, which dyld needs.
/// It does not allow reading children. Child paths are the subpath allow-list.
#[must_use]
pub fn render_seatbelt(fs_plan: &McpFsPlan, secret_denies: &[PathBuf], net_none: bool) -> String {
    let read_rules = subpath_rules(&fs_plan.read);
    let write_rules = subpath_rules(&fs_plan.write);
    let secret_rules = subpath_rules(secret_denies);
    let net_line = if net_none {
        "; network denied (--net none)\n"
    } else {
        "(allow network*)\n"
    };
    format!(
        "(version 1)\n\
         ; devcage mcp connector isolation — one server, not the agent\n\
         (deny default)\n\
         (allow process*)\n\
         (allow signal)\n\
         (allow sysctl-read)\n\
         (allow file-map-executable)\n\
         (allow file-read-metadata)\n\
         (allow mach-lookup)\n\
         {net_line}\
         (allow file-read*\n\
           (literal \"/\")\n\
         {read_rules}\n\
         )\n\
         (allow file-write*\n\
         {write_rules}\n\
         )\n\
         (deny file-read*\n\
         {secret_rules}\n\
         )\n\
         (deny file-write*\n\
         {secret_rules}\n\
         )\n\
         (deny mach-lookup\n\
           (global-name \"com.apple.SecurityServer\")\n\
         )\n"
    )
}

fn subpath_rules(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|p| format!("  (subpath \"{}\")", escape_sb(&p.display().to_string())))
        .collect::<Vec<_>>()
        .join("\n")
}

fn escape_sb(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Docker/Podman `run` arguments for `--container` (no TTY).
#[derive(Debug, Clone)]
pub struct McpContainerPlan {
    /// Image name.
    pub image: String,
    /// Guest `HOME` (`/home/mcp`).
    pub guest_home: String,
    /// Host ephemeral home.
    pub host_home: PathBuf,
    /// Optional workspace `(host path, share)`.
    pub workspace: Option<(PathBuf, WorkspaceShare)>,
    /// Guest workspace path.
    pub guest_workspace: String,
    /// Host pkg-guard cache when the directory exists.
    pub host_pkg_cache: Option<PathBuf>,
    /// Guest pkg-guard cache path.
    pub guest_pkg_cache: String,
    /// Deny container network.
    pub net_none: bool,
    /// Environment entries (`KEY`, `VALUE`).
    pub env: Vec<(String, String)>,
    /// Command inside the image.
    pub command: Vec<String>,
}

/// argv after the runtime binary name (`docker` / `podman`).
#[must_use]
pub fn mcp_container_args(plan: &McpContainerPlan) -> Vec<String> {
    let mut args = vec!["run".to_owned(), "--rm".to_owned(), "-i".to_owned()];
    if plan.net_none {
        args.push("--network".to_owned());
        args.push("none".to_owned());
    }
    args.push("--security-opt".to_owned());
    args.push("no-new-privileges".to_owned());
    args.push("--cap-drop".to_owned());
    args.push("ALL".to_owned());

    args.push("-v".to_owned());
    args.push(format!(
        "{}:{}:rw",
        plan.host_home.display(),
        plan.guest_home
    ));
    if let Some((ref host, share)) = plan.workspace {
        let mode = match share {
            WorkspaceShare::None | WorkspaceShare::ReadOnly => "ro",
            WorkspaceShare::ReadWrite => "rw",
        };
        if share != WorkspaceShare::None {
            args.push("-v".to_owned());
            args.push(format!(
                "{}:{}:{mode}",
                host.display(),
                plan.guest_workspace
            ));
        }
    }
    if let Some(ref cache) = plan.host_pkg_cache {
        args.push("-v".to_owned());
        args.push(format!("{}:{}:ro", cache.display(), plan.guest_pkg_cache));
    }
    let workdir = if plan.workspace.is_some() {
        plan.guest_workspace.as_str()
    } else {
        plan.guest_home.as_str()
    };
    args.push("-w".to_owned());
    args.push(workdir.to_owned());
    for (key, value) in &plan.env {
        args.push("-e".to_owned());
        args.push(format!("{key}={value}"));
    }
    args.push(plan.image.clone());
    args.extend(plan.command.iter().cloned());
    args
}

/// Honest limit: a macOS host container is a Linux guest.
#[must_use]
#[allow(clippy::if_then_some_else_none)]
pub const fn container_darwin_limit() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some(
            "devcage mcp: --container on macOS runs a Linux guest. A Darwin uvx/npx binary cannot run there; use the default light wrap for host-native connectors.",
        )
    } else {
        None
    }
}

/// Pick Seatbelt, bwrap, or refuse.
#[must_use]
pub const fn detect_os_sandbox(seatbelt_on_path: bool, bwrap_on_path: bool) -> OsSandbox {
    if cfg!(target_os = "macos") && seatbelt_on_path {
        OsSandbox::Seatbelt
    } else if bwrap_on_path {
        OsSandbox::Bwrap
    } else if seatbelt_on_path {
        OsSandbox::Seatbelt
    } else {
        OsSandbox::None
    }
}

/// Decide the venue. Container does not require an OS sandbox on the host.
///
/// # Errors
///
/// Light wrap was requested, no OS sandbox exists, and `--allow-env-only` was not set.
pub fn choose_launch(
    container: bool,
    sandbox: OsSandbox,
    allow_env_only: bool,
) -> anyhow::Result<LaunchMode> {
    if container {
        return Ok(LaunchMode::Container);
    }
    match sandbox {
        OsSandbox::Seatbelt => Ok(LaunchMode::Seatbelt),
        OsSandbox::Bwrap => Ok(LaunchMode::Bwrap),
        OsSandbox::None if allow_env_only => Ok(LaunchMode::EnvOnly),
        OsSandbox::None => anyhow::bail!(
            "no OS sandbox (sandbox-exec or bwrap) on PATH; refusing to start because env-only would not limit this connector. Pass --allow-env-only to run without an OS sandbox."
        ),
    }
}

/// Human-readable plan. Does not include secret values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpPlanView {
    /// Venue label, or `refuse` when a real launch would fail closed.
    pub isolation: String,
    /// True when a non-print launch would refuse.
    pub would_refuse: bool,
    /// Network is denied.
    pub net_none: bool,
    /// Workspace exposure.
    pub share: WorkspaceShare,
    /// Ephemeral home, or the guest home for containers.
    pub home_display: String,
    /// pkg-guard cache path shown to the operator.
    pub pkg_guard_cache: String,
    /// Optional macOS container note.
    pub container_note: Option<&'static str>,
    /// Sign-in URL bridge mode.
    pub open_urls: OpenUrls,
    /// Connector argv.
    pub command: Vec<String>,
}

/// `--print` text. Secret values are never included.
#[must_use]
pub fn format_mcp_plan(view: &McpPlanView) -> String {
    let mut lines = vec![
        "mode: mcp".to_owned(),
        "agent_isolated: false".to_owned(),
        format!("isolation: {}", view.isolation),
        format!("would_refuse: {}", view.would_refuse),
        format!(
            "network: {}",
            if view.net_none { "none" } else { "allowed" }
        ),
        format!("share_workspace: {}", view.share.label()),
        format!("home: {}", view.home_display),
        format!("pkg_guard_cache: {}", view.pkg_guard_cache),
        format!("open_urls: {}", view.open_urls.label()),
        format!("scrub: {}", SCRUB_ENV.join(", ")),
        "keep_injected_env: true".to_owned(),
    ];
    if let Some(note) = view.container_note {
        lines.push(format!("container_note: {note}"));
    }
    lines.push(format!("command: {}", view.command.join(" ")));
    lines.join("\n")
}

/// Run connector isolation.
///
/// # Errors
///
/// Invalid flags, a missing OS sandbox without `--allow-env-only`, a missing
/// container runtime, or a failure to start the child.
pub fn run_mcp(cfg: &Config, params: &McpParams) -> anyhow::Result<ExitCode> {
    let settings = McpSettings::from_params(params)?;
    let host_home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
    let pkg_cache = host_pkg_guard_cache(HostCacheInputs {
        pkg_guard_cache_dir: std::env::var("PKG_GUARD_CACHE_DIR").ok().as_deref(),
        xdg_cache_home: std::env::var("XDG_CACHE_HOME").ok().as_deref(),
        home: &host_home,
    });
    let sandbox = detect_os_sandbox(which("sandbox-exec"), which("bwrap"));
    let launch_result = choose_launch(settings.container, sandbox, settings.allow_env_only);
    if params.print_only {
        println!(
            "{}",
            format_mcp_plan(&plan_view(&settings, &pkg_cache, launch_result.as_ref(),))
        );
        return Ok(ExitCode::SUCCESS);
    }
    let launch = launch_result?;
    if params.verbose {
        eprintln!(
            "{}",
            verbose_banner(&plan_view(&settings, &pkg_cache, Ok(&launch)))
        );
    }
    if matches!(launch, LaunchMode::Container) {
        if let Some(note) = container_darwin_limit() {
            eprintln!("{note}");
        }
    }
    if !settings.container {
        ensure_command_visible(settings.command.first().map(String::as_str))?;
    }

    let session_dir = cfg.data_dir.join("mcp").join(new_session_id());
    let home = session_dir.join("home");
    let child_env = McpChildEnv::from_ephemeral(&home, &pkg_cache, settings.net_none);
    child_env.create_dirs()?;
    let bridge = match open_url_bridge(&session_dir, &settings) {
        Ok(bridge) => bridge,
        Err(err) => {
            wipe_session(&session_dir);
            return Err(err);
        },
    };
    let outcome = launch_child(
        cfg,
        &settings,
        launch,
        &session_dir,
        &child_env,
        &host_home,
        bridge.as_ref(),
    );
    drop(bridge);
    wipe_session(&session_dir);
    outcome
}

fn open_url_bridge(
    session_dir: &Path,
    settings: &McpSettings,
) -> anyhow::Result<Option<UrlBridge>> {
    if !settings.open_urls.is_enabled() {
        return Ok(None);
    }
    let label = connector_label(&settings.command);
    start_sign_in_bridge(session_dir, &label, settings.open_urls).map(Some)
}

fn wipe_session(session_dir: &Path) {
    if let Err(err) = fs::remove_dir_all(session_dir) {
        eprintln!(
            "devcage mcp: failed to wipe {}: {err}",
            session_dir.display()
        );
    }
}

fn plan_view(
    settings: &McpSettings,
    pkg_cache: &Path,
    launch: Result<&LaunchMode, &anyhow::Error>,
) -> McpPlanView {
    let (isolation, would_refuse) = launch.map_or_else(
        |_| ("refuse".to_owned(), true),
        |mode| (mode.label().to_owned(), false),
    );
    let home_display = if settings.container {
        "/home/mcp".to_owned()
    } else {
        "(ephemeral, wiped on exit)".to_owned()
    };
    McpPlanView {
        isolation,
        would_refuse,
        net_none: settings.net_none,
        share: settings.share,
        home_display,
        pkg_guard_cache: pkg_cache.display().to_string(),
        container_note: if settings.container {
            container_darwin_limit()
        } else {
            None
        },
        open_urls: settings.open_urls,
        command: settings.command.clone(),
    }
}

fn verbose_banner(view: &McpPlanView) -> String {
    format!(
        "devcage mode: mcp (connector isolation)\n\
         \x20 command:    {}\n\
         \x20 isolation:  {}\n\
         \x20 network:    {}\n\
         \x20 workspace:  {}\n\
         \x20 open_urls:  {}\n\
         \x20 home:       {}\n\
         \x20 limits:     this connector's injected env; not Keychain, the SSH agent, or the rest of the host\n\
         \x20 agent:      not isolated (host client)\n",
        view.command.join(" "),
        view.isolation,
        if view.net_none { "none" } else { "allowed" },
        view.share.label(),
        view.open_urls.label(),
        view.home_display,
    )
}

fn ensure_command_visible(prog: Option<&str>) -> anyhow::Result<()> {
    let Some(prog) = prog else {
        anyhow::bail!("empty command");
    };
    if Path::new(prog).components().count() == 1 && !which(prog) {
        anyhow::bail!("command `{prog}` not found on PATH");
    }
    Ok(())
}

fn new_session_id() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("mcp-{}-{millis}", std::process::id())
}

fn launch_child(
    cfg: &Config,
    settings: &McpSettings,
    launch: LaunchMode,
    session_dir: &Path,
    child_env: &McpChildEnv,
    host_home: &Path,
    bridge: Option<&UrlBridge>,
) -> anyhow::Result<ExitCode> {
    match launch {
        LaunchMode::Container => launch_container(cfg, settings, child_env),
        LaunchMode::Seatbelt => {
            launch_seatbelt(settings, session_dir, child_env, host_home, bridge)
        },
        LaunchMode::Bwrap => launch_bwrap(settings, child_env, host_home, bridge),
        LaunchMode::EnvOnly => {
            let mut cmd = command_from(&settings.command)?;
            prepare_host_child(
                &mut cmd,
                settings,
                child_env,
                host_home,
                child_bridge_paths(bridge, false).as_ref(),
            );
            wait_command(&mut cmd, &settings.command)
        },
    }
}

fn child_bridge_paths(bridge: Option<&UrlBridge>, guest: bool) -> Option<ChildBridgePaths> {
    bridge.map(|bridge| bridge.child_paths(guest))
}

fn launch_container(
    cfg: &Config,
    settings: &McpSettings,
    child_env: &McpChildEnv,
) -> anyhow::Result<ExitCode> {
    let runtime = crate::backend::detect_container_runtime(cfg.container_runtime.as_deref())?;
    let guest = McpChildEnv {
        home: PathBuf::from("/home/mcp"),
        xdg_cache_home: PathBuf::from("/home/mcp/.cache"),
        xdg_config_home: PathBuf::from("/home/mcp/.config"),
        xdg_data_home: PathBuf::from("/home/mcp/.local/share"),
        xdg_state_home: PathBuf::from("/home/mcp/.local/state"),
        uv_cache_dir: PathBuf::from("/home/mcp/.cache/uv"),
        uv_tool_dir: PathBuf::from("/home/mcp/.local/share/uv/tools"),
        npm_cache: PathBuf::from("/home/mcp/.cache/npm"),
        pkg_guard_cache_dir: PathBuf::from("/pkg-guard-cache"),
        net_none: settings.net_none,
    };
    let host_pkg_cache = child_env
        .pkg_guard_cache_dir
        .is_dir()
        .then(|| child_env.pkg_guard_cache_dir.clone());
    let plan = McpContainerPlan {
        image: cfg.container_image.clone(),
        guest_home: "/home/mcp".to_owned(),
        host_home: child_env.home.clone(),
        workspace: settings
            .workspace
            .clone()
            .map(|path| (path, settings.share)),
        guest_workspace: "/workspace".to_owned(),
        host_pkg_cache,
        guest_pkg_cache: "/pkg-guard-cache".to_owned(),
        net_none: settings.net_none,
        env: container_env_pairs(std::env::vars(), &guest),
        command: settings.command.clone(),
    };
    let args = mcp_container_args(&plan);
    let mut cmd = Command::new(runtime.bin());
    cmd.args(&args);
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    wait_command(&mut cmd, &settings.command)
}

fn launch_seatbelt(
    settings: &McpSettings,
    session_dir: &Path,
    child_env: &McpChildEnv,
    host_home: &Path,
    bridge: Option<&UrlBridge>,
) -> anyhow::Result<ExitCode> {
    let profile_path = session_dir.join("profile.sb");
    let profile = seatbelt_for(settings, child_env, host_home, bridge);
    fs::write(&profile_path, profile)?;
    let (prog, rest) = split_command(&settings.command)?;
    let mut cmd = Command::new("sandbox-exec");
    cmd.arg("-f").arg(&profile_path);
    cmd.arg(prog);
    cmd.args(rest);
    prepare_host_child(
        &mut cmd,
        settings,
        child_env,
        host_home,
        child_bridge_paths(bridge, false).as_ref(),
    );
    wait_command(&mut cmd, &settings.command)
}

fn launch_bwrap(
    settings: &McpSettings,
    child_env: &McpChildEnv,
    host_home: &Path,
    bridge: Option<&UrlBridge>,
) -> anyhow::Result<ExitCode> {
    let ro_mounts = read_extra_paths(host_home, &child_env.pkg_guard_cache_dir, &settings.command)
        .into_iter()
        .filter(|path| path.is_dir())
        .collect();
    let argv = mcp_bwrap_argv(&McpBwrapPlan {
        ephemeral_home: child_env.home.clone(),
        workspace: settings.workspace.clone().and_then(|path| {
            let mode = match settings.share {
                WorkspaceShare::ReadOnly => McpBindMode::ReadOnly,
                WorkspaceShare::ReadWrite => McpBindMode::ReadWrite,
                WorkspaceShare::None => return None,
            };
            Some((path, mode))
        }),
        ro_mounts,
        url_bridge: bridge.map(|bridge| McpUrlBridge {
            shim_dir: bridge.shim_dir().to_path_buf(),
            fifo: bridge.fifo().to_path_buf(),
        }),
        net_none: settings.net_none,
        command: settings.command.clone(),
    })?;
    let Some((prog, rest)) = argv.split_first() else {
        anyhow::bail!("empty bwrap argv");
    };
    let mut cmd = Command::new(prog);
    cmd.args(rest);
    prepare_host_child(
        &mut cmd,
        settings,
        child_env,
        host_home,
        child_bridge_paths(bridge, true).as_ref(),
    );
    wait_command(&mut cmd, &settings.command)
}

fn seatbelt_for(
    settings: &McpSettings,
    child_env: &McpChildEnv,
    host_home: &Path,
    bridge: Option<&UrlBridge>,
) -> String {
    let extras = read_extra_paths(host_home, &child_env.pkg_guard_cache_dir, &settings.command);
    let workspace = settings
        .workspace
        .as_deref()
        .map(|path| (path, settings.share));
    let mut fs_plan = mcp_fs_plan(&child_env.home, workspace, &extras);
    if let Some(bridge) = bridge {
        push_unique(&mut fs_plan.read, bridge.shim_dir());
        push_unique(&mut fs_plan.write, bridge.fifo());
    }
    render_seatbelt(&fs_plan, &secret_paths(host_home), settings.net_none)
}

fn read_extra_paths(home: &Path, pkg_cache: &Path, command: &[String]) -> Vec<PathBuf> {
    let path_env = std::env::var("PATH").unwrap_or_default();
    extra_paths_from(home, pkg_cache, command, &path_env)
}

/// Directories the connector may read: toolchain bins, plus an install prefix
/// when a `PATH` entry symlinks into a sibling `lib` (nvm `npx` → `../lib/...`).
///
/// The prefix is the parent of that `PATH` directory. `$HOME` and credential
/// directories are never added.
fn extra_paths_from(
    home: &Path,
    pkg_cache: &Path,
    command: &[String],
    path_env: &str,
) -> Vec<PathBuf> {
    let mut out = vec![
        home.join(".local").join("bin"),
        home.join(".local").join("share").join("pkg-guard"),
        home.join(".cargo").join("bin"),
        home.join(".config").join("pkg-guard"),
        pkg_cache.to_path_buf(),
    ];
    for component in path_env.split(':') {
        if component.is_empty() {
            continue;
        }
        let candidate = PathBuf::from(component);
        if candidate.is_absolute() && !is_broad_or_secret(home, &candidate) {
            push_unique(&mut out, &candidate);
        }
    }
    if let Some(prog) = command.first() {
        let prog_path = PathBuf::from(prog);
        if prog_path.is_absolute() {
            if let Some(parent) = prog_path.parent() {
                if !parent.as_os_str().is_empty() && !is_broad_or_secret(home, parent) {
                    push_unique(&mut out, parent);
                }
            }
        }
    }
    push_sibling_lib_prefixes(&mut out, home);
    out
}

/// Bind `.../versions/node/v20` when `.../bin/npx` points at `../lib/...`.
///
/// A symlink that stays in `bin`, or that points at `../downloads` rather than
/// `lib`, does not pull in the parent. That keeps `~/.grok` and `~/.cargo`
/// closed even when their `bin` directories are on `PATH`.
fn push_sibling_lib_prefixes(out: &mut Vec<PathBuf>, home: &Path) {
    let dirs = out.clone();
    for dir in dirs {
        let Some(parent) = dir.parent() else {
            continue;
        };
        if !install_prefix_is_safe(home, parent) {
            continue;
        }
        if dir_links_sibling_lib(&dir, parent) {
            push_unique(out, parent);
        }
    }
}

fn install_prefix_is_safe(home: &Path, prefix: &Path) -> bool {
    !prefix.as_os_str().is_empty() && !is_broad_or_secret(home, prefix) && !is_system_prefix(prefix)
}

/// Roots already visible through the system bind, or too wide to add whole.
fn is_system_prefix(path: &Path) -> bool {
    const SYSTEM: &[&str] = &[
        "/",
        "/usr",
        "/usr/local",
        "/bin",
        "/sbin",
        "/lib",
        "/lib64",
        "/etc",
        "/opt",
        "/opt/homebrew",
        "/System",
        "/Library",
        "/private",
        "/var",
        "/run",
        "/dev",
        "/proc",
        "/tmp",
        "/sys",
        "/snap",
        "/boot",
        "/home",
        "/Users",
    ];
    SYSTEM.iter().any(|prefix| path == Path::new(prefix))
}

fn dir_links_sibling_lib(dir: &Path, parent: &Path) -> bool {
    let lib_roots = sibling_lib_roots(parent);
    if lib_roots.is_empty() {
        return false;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries
        .flatten()
        .any(|entry| symlink_lands_in(&entry.path(), &lib_roots))
}

fn sibling_lib_roots(parent: &Path) -> Vec<PathBuf> {
    ["lib", "lib64"]
        .into_iter()
        .filter_map(|name| fs::canonicalize(parent.join(name)).ok())
        .collect()
}

fn symlink_lands_in(path: &Path, lib_roots: &[PathBuf]) -> bool {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if !meta.file_type().is_symlink() {
        return false;
    }
    let Ok(target) = fs::canonicalize(path) else {
        return false;
    };
    lib_roots.iter().any(|root| target.starts_with(root))
}

fn is_broad_or_secret(home: &Path, path: &Path) -> bool {
    path == home
        || secret_paths(home)
            .iter()
            .any(|secret| path.starts_with(secret))
}

fn prepare_host_child(
    cmd: &mut Command,
    settings: &McpSettings,
    child_env: &McpChildEnv,
    host_home: &Path,
    bridge: Option<&ChildBridgePaths>,
) {
    let cwd = settings
        .workspace
        .clone()
        .unwrap_or_else(|| child_env.home.clone());
    cmd.current_dir(cwd);
    for (key, value) in child_env.assignments() {
        cmd.env(key, value);
    }
    let existing = std::env::var("PATH").unwrap_or_default();
    let mut path = prepend_shim_path(host_home, &existing);
    if let Some(bridge) = bridge {
        path = prepend_path_entry(&bridge.path_prefix, &path);
        cmd.env("BROWSER", &bridge.browser);
        cmd.env("DEVCAGE_URL_BRIDGE", &bridge.fifo);
        for key in DESKTOP_SESSION_VARS {
            cmd.env_remove(*key);
        }
    }
    cmd.env("PATH", path);
    for key in SCRUB_ENV {
        cmd.env_remove(*key);
    }
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
}

/// Put pkg-guard shims first on `PATH` when that directory exists.
#[must_use]
pub fn prepend_shim_path(home: &Path, existing: &str) -> String {
    let shim = home
        .join(".local")
        .join("share")
        .join("pkg-guard")
        .join("shims");
    if !shim.is_dir() {
        return existing.to_owned();
    }
    let shim_s = shim.display().to_string();
    if existing.split(':').next() == Some(shim_s.as_str()) {
        return existing.to_owned();
    }
    if existing.is_empty() {
        shim_s
    } else {
        format!("{shim_s}:{existing}")
    }
}

fn command_from(command: &[String]) -> anyhow::Result<Command> {
    let (prog, rest) = split_command(command)?;
    let mut cmd = Command::new(prog);
    cmd.args(rest);
    Ok(cmd)
}

fn split_command(command: &[String]) -> anyhow::Result<(&str, &[String])> {
    let Some((prog, rest)) = command.split_first() else {
        anyhow::bail!("empty command");
    };
    Ok((prog.as_str(), rest))
}

fn wait_command(cmd: &mut Command, command: &[String]) -> anyhow::Result<ExitCode> {
    let status = cmd
        .status()
        .map_err(|err| anyhow::anyhow!("failed to start `{}`: {err}", command.join(" ")))?;
    Ok(exit_code_from_i32(status.code().unwrap_or(1)))
}

fn exit_code_from_i32(code: i32) -> ExitCode {
    if code == 0 {
        ExitCode::SUCCESS
    } else if (1..=255).contains(&code) {
        ExitCode::from(u8::try_from(code).unwrap_or(1))
    } else {
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::{
        choose_launch, container_env_pairs, detect_os_sandbox, format_mcp_plan,
        host_pkg_guard_cache, mcp_container_args, mcp_fs_plan, parse_net, prepend_shim_path,
        render_seatbelt, should_scrub, HostCacheInputs, LaunchMode, McpChildEnv, McpContainerPlan,
        McpParams, McpPlanView, McpSettings, OpenUrls, OsSandbox, WorkspaceShare,
    };
    use std::fs;
    use std::path::{Path, PathBuf};

    fn workspace_share_label(share: WorkspaceShare) -> &'static str {
        share.label()
    }

    #[test]
    fn parses_share_and_net() {
        assert_eq!(
            WorkspaceShare::parse("none").expect("none"),
            WorkspaceShare::None
        );
        assert_eq!(
            WorkspaceShare::parse("RO").expect("ro"),
            WorkspaceShare::ReadOnly
        );
        assert_eq!(
            WorkspaceShare::parse("read-write").expect("rw"),
            WorkspaceShare::ReadWrite
        );
        assert!(WorkspaceShare::parse("all").is_err());
        assert!(!parse_net("allowed").expect("net"));
        assert!(parse_net("none").expect("none"));
        assert!(parse_net("full").is_err());
        assert_eq!(workspace_share_label(WorkspaceShare::None), "none");
    }

    #[test]
    fn settings_reject_empty_command_and_keep_workspace_off_by_default() {
        let err = McpSettings::from_params(&McpParams {
            print_only: true,
            verbose: false,
            container: false,
            allow_env_only: false,
            open_urls: OpenUrls::Off,
            net: "allowed".into(),
            share_workspace: "none".into(),
            workspace: None,
            command: Vec::new(),
        });
        assert!(err.is_err());

        let settings = McpSettings::from_params(&McpParams {
            print_only: false,
            verbose: false,
            container: false,
            allow_env_only: false,
            open_urls: OpenUrls::Off,
            net: "none".into(),
            share_workspace: "none".into(),
            workspace: Some(PathBuf::from("/tmp/should-not-mount")),
            command: vec!["uvx".into(), "server==1.2.3".into()],
        })
        .expect("settings");
        assert!(settings.workspace.is_none());
        assert!(settings.net_none);
        assert_eq!(settings.open_urls, OpenUrls::Off);

        let refused = McpSettings::from_params(&McpParams {
            print_only: false,
            verbose: false,
            container: true,
            allow_env_only: false,
            open_urls: OpenUrls::Open,
            net: "allowed".into(),
            share_workspace: "none".into(),
            workspace: None,
            command: vec!["uvx".into(), "server==1.2.3".into()],
        });
        assert!(refused.is_err());
    }

    #[test]
    fn env_plan_keeps_api_keys_and_drops_ssh_agent() {
        assert!(!should_scrub("ANTHROPIC_API_KEY"));
        assert!(!should_scrub("AWS_SECRET_ACCESS_KEY"));
        assert!(!should_scrub("GITHUB_TOKEN"));
        assert!(should_scrub("SSH_AUTH_SOCK"));
        assert!(should_scrub("SSH_AGENT_PID"));

        let home = Path::new("/tmp/eph-mcp");
        let cache = Path::new("/host/cache/pkg-guard");
        let env = McpChildEnv::from_ephemeral(home, cache, false);
        let pairs = env.assignments();
        assert!(pairs
            .iter()
            .any(|(k, v)| k == "HOME" && v == "/tmp/eph-mcp"));
        assert!(pairs
            .iter()
            .any(|(k, v)| k == "UV_CACHE_DIR" && v == "/tmp/eph-mcp/.cache/uv"));
        assert!(pairs
            .iter()
            .any(|(k, v)| k == "NPM_CONFIG_CACHE" && v == "/tmp/eph-mcp/.cache/npm"));
        assert!(pairs
            .iter()
            .any(|(k, v)| k == "PKG_GUARD_CACHE_DIR" && v == "/host/cache/pkg-guard"));
        assert!(pairs
            .iter()
            .any(|(k, v)| k == "PKG_GUARD_TRANSITIVE_CACHE_TTL_SECS" && v == "0"));
        assert!(pairs.iter().any(|(k, v)| k == "DEVCAGE_MODE" && v == "mcp"));
        assert!(!pairs.iter().any(|(k, _)| should_scrub(k)));
        assert!(!pairs.iter().any(|(k, _)| k == "ANTHROPIC_API_KEY"));
    }

    #[test]
    fn container_env_forwards_injected_secret_not_ssh() {
        let guest = McpChildEnv::from_ephemeral(
            Path::new("/home/mcp"),
            Path::new("/pkg-guard-cache"),
            true,
        );
        let pairs = container_env_pairs(
            [
                ("ANTHROPIC_API_KEY".into(), "test-key".into()),
                ("SSH_AUTH_SOCK".into(), "/tmp/agent.sock".into()),
                ("PATH".into(), "/usr/bin".into()),
                ("DYLD_INSERT_LIBRARIES".into(), "/evil".into()),
            ],
            &guest,
        );
        assert!(pairs
            .iter()
            .any(|(k, v)| k == "ANTHROPIC_API_KEY" && v == "test-key"));
        assert!(!pairs.iter().any(|(k, _)| k == "SSH_AUTH_SOCK"));
        assert!(!pairs.iter().any(|(k, _)| k == "PATH"));
        assert!(!pairs.iter().any(|(k, _)| k == "DYLD_INSERT_LIBRARIES"));
        assert!(pairs.iter().any(|(k, v)| k == "HOME" && v == "/home/mcp"));
        assert!(pairs
            .iter()
            .any(|(k, v)| k == "DEVCAGE_MCP_NET" && v == "none"));
    }

    #[test]
    fn host_cache_prefers_explicit_dir() {
        let home = Path::new("/Users/dev");
        let explicit = host_pkg_guard_cache(HostCacheInputs {
            pkg_guard_cache_dir: Some("/data/pkg-guard"),
            xdg_cache_home: Some("/xdg"),
            home,
        });
        assert_eq!(explicit, PathBuf::from("/data/pkg-guard"));
        let xdg = host_pkg_guard_cache(HostCacheInputs {
            pkg_guard_cache_dir: None,
            xdg_cache_home: Some("/xdg"),
            home,
        });
        assert_eq!(xdg, PathBuf::from("/xdg/pkg-guard"));
        let fallback = host_pkg_guard_cache(HostCacheInputs {
            pkg_guard_cache_dir: Some("  "),
            xdg_cache_home: None,
            home,
        });
        assert_eq!(fallback, home.join(".cache/pkg-guard"));
    }

    #[test]
    fn seatbelt_hides_workspace_agent_dirs_and_keychain() {
        let home = Path::new("/tmp/eph-mcp");
        let none = mcp_fs_plan(home, None, &[]);
        assert!(none.read.iter().any(|p| p == home));
        assert!(none.write.iter().any(|p| p == home));
        assert!(!none.read.iter().any(|p| p.ends_with("ws-mcp")));
        assert!(!none.write.iter().any(|p| {
            let s = p.display().to_string();
            s.contains(".grok")
                || s.contains(".claude")
                || s.contains(".cursor")
                || s.contains(".codex")
        }));

        let ws = Path::new("/tmp/ws-mcp");
        let ro = mcp_fs_plan(home, Some((ws, WorkspaceShare::ReadOnly)), &[]);
        assert!(ro.read.iter().any(|p| p == ws));
        assert!(!ro.write.iter().any(|p| p == ws));
        let rw = mcp_fs_plan(home, Some((ws, WorkspaceShare::ReadWrite)), &[]);
        assert!(rw.write.iter().any(|p| p == ws));

        let secrets = super::secret_paths(Path::new("/Users/dev"));
        let text = render_seatbelt(&none, &secrets, true);
        assert!(text.contains("(literal \"/\")"));
        assert!(!text.contains("(subpath \"/\")"));
        assert!(text.contains("/tmp/eph-mcp"));
        assert!(!text.contains("/tmp/ws-mcp"));
        assert!(!text.contains("(allow mach*)"));
        assert!(text.contains("com.apple.SecurityServer"));
        assert!(text.contains("network denied"));
        assert!(!text.contains("(allow network*)"));
        assert!(text.contains("/Users/dev/.ssh"));
        assert!(text.contains("/Users/dev/Library/Keychains"));
        assert!(!text.contains(".grok"));

        let open = render_seatbelt(&rw, &secrets, false);
        assert!(open.contains("(allow network*)"));
        assert!(open.contains("/tmp/ws-mcp"));
    }

    #[test]
    fn container_args_are_stdio_and_optional_workspace() {
        let env = McpChildEnv::from_ephemeral(
            Path::new("/home/mcp"),
            Path::new("/pkg-guard-cache"),
            false,
        );
        let pairs = container_env_pairs([("ANTHROPIC_API_KEY".into(), "test-key".into())], &env);
        let args = mcp_container_args(&McpContainerPlan {
            image: "alpine:3.20".into(),
            guest_home: "/home/mcp".into(),
            host_home: PathBuf::from("/tmp/eph-mcp"),
            workspace: None,
            guest_workspace: "/workspace".into(),
            host_pkg_cache: None,
            guest_pkg_cache: "/pkg-guard-cache".into(),
            net_none: false,
            env: pairs,
            command: vec!["uvx".into(), "server==1.2.3".into()],
        });
        assert!(args.contains(&"--rm".into()));
        assert!(args.contains(&"-i".into()));
        assert!(!args.contains(&"-t".into()));
        assert!(!args
            .windows(2)
            .any(|w| w[0] == "--network" && w[1] == "none"));
        assert!(!args.iter().any(|a| a.contains("/workspace")));
        assert!(args
            .iter()
            .any(|a| a.contains("ANTHROPIC_API_KEY=test-key")));
        assert_eq!(args.last().map(String::as_str), Some("server==1.2.3"));

        let offline = mcp_container_args(&McpContainerPlan {
            image: "alpine:3.20".into(),
            guest_home: "/home/mcp".into(),
            host_home: PathBuf::from("/tmp/eph-mcp"),
            workspace: Some((PathBuf::from("/tmp/ws-mcp"), WorkspaceShare::ReadOnly)),
            guest_workspace: "/workspace".into(),
            host_pkg_cache: Some(PathBuf::from("/host/pkg-guard")),
            guest_pkg_cache: "/pkg-guard-cache".into(),
            net_none: true,
            env: Vec::new(),
            command: vec!["true".into()],
        });
        assert!(offline
            .windows(2)
            .any(|w| w[0] == "--network" && w[1] == "none"));
        assert!(offline
            .iter()
            .any(|a| a.contains("/tmp/ws-mcp:/workspace:ro")));
        assert!(offline
            .iter()
            .any(|a| a.contains("/host/pkg-guard:/pkg-guard-cache:ro")));
        assert!(!offline.contains(&"-t".into()));
    }

    #[test]
    fn missing_sandbox_refuses_unless_env_only_or_container() {
        assert!(choose_launch(false, OsSandbox::None, false).is_err());
        assert_eq!(
            choose_launch(false, OsSandbox::None, true).expect("env"),
            LaunchMode::EnvOnly
        );
        assert_eq!(
            choose_launch(true, OsSandbox::None, false).expect("container"),
            LaunchMode::Container
        );
        assert_eq!(
            detect_os_sandbox(true, true),
            if cfg!(target_os = "macos") {
                OsSandbox::Seatbelt
            } else {
                OsSandbox::Bwrap
            }
        );
        assert_eq!(detect_os_sandbox(false, false), OsSandbox::None);
        assert_eq!(LaunchMode::Seatbelt.label(), "seatbelt");
    }

    #[test]
    fn print_plan_has_no_secret_values() {
        let text = format_mcp_plan(&McpPlanView {
            isolation: "seatbelt".into(),
            would_refuse: false,
            net_none: false,
            share: WorkspaceShare::None,
            home_display: "(ephemeral, wiped on exit)".into(),
            pkg_guard_cache: "/host/cache/pkg-guard".into(),
            container_note: None,
            open_urls: OpenUrls::Off,
            command: vec!["uvx".into(), "server==1.2.3".into()],
        });
        assert!(text.contains("mode: mcp"));
        assert!(text.contains("agent_isolated: false"));
        assert!(text.contains("share_workspace: none"));
        assert!(text.contains("open_urls: off"));
        assert!(text.contains("keep_injected_env: true"));
        assert!(text.contains("SSH_AUTH_SOCK"));
        assert!(!text.contains("test-key"));
        assert!(!text.contains("ANTHROPIC_API_KEY="));
    }

    #[test]
    fn shim_path_prepend_is_stable() {
        let home = Path::new("/no/such/home");
        assert_eq!(prepend_shim_path(home, "/usr/bin"), "/usr/bin");
    }

    #[cfg(unix)]
    #[test]
    fn path_symlink_into_sibling_lib_adds_prefix_not_home_or_secrets() {
        use std::os::unix::fs::symlink;

        let root = mcp_temp("nvm-prefix");
        let home = root.join("home");
        let prefix = root.join("node").join("v20");
        let bin = prefix.join("bin");
        let npx_js = prefix.join("lib/node_modules/npm/bin/npx-cli.js");
        fs::create_dir_all(npx_js.parent().expect("parent")).expect("lib");
        fs::create_dir_all(&bin).expect("bin");
        fs::write(&npx_js, "#!/usr/bin/sh\necho NPX-OK\n").expect("npx js");
        symlink("../lib/node_modules/npm/bin/npx-cli.js", bin.join("npx")).expect("npx link");

        let cargo = root.join("cargo");
        fs::create_dir_all(cargo.join("bin")).expect("cargo bin");
        fs::create_dir_all(cargo.join("lib")).expect("cargo lib");
        fs::write(cargo.join("bin/rustup"), "x").expect("rustup");
        symlink("rustup", cargo.join("bin/cargo")).expect("cargo link");

        let grok = root.join("grok");
        fs::create_dir_all(grok.join("bin")).expect("grok bin");
        fs::create_dir_all(grok.join("lib")).expect("grok lib");
        fs::create_dir_all(grok.join("downloads")).expect("downloads");
        fs::write(grok.join("downloads/grok-bin"), "x").expect("grok bin file");
        symlink("../downloads/grok-bin", grok.join("bin/grok")).expect("grok link");

        fs::create_dir_all(home.join("bin")).expect("home bin");
        fs::create_dir_all(home.join("lib")).expect("home lib");
        fs::write(home.join("lib/tool"), "x").expect("home tool");
        symlink("../lib/tool", home.join("bin/tool")).expect("home link");

        fs::create_dir_all(home.join(".aws/bin")).expect("aws bin");
        fs::create_dir_all(home.join(".aws/lib")).expect("aws lib");
        fs::create_dir_all(home.join(".ssh")).expect("ssh");
        fs::write(home.join(".aws/lib/x"), "x").expect("aws lib file");
        symlink("../lib/x", home.join(".aws/bin/x")).expect("aws link");

        let path_env = [
            bin.clone(),
            cargo.join("bin"),
            grok.join("bin"),
            home.join("bin"),
            home.join(".aws/bin"),
        ]
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(":");
        let got = super::extra_paths_from(&home, &root.join("cache"), &["npx".into()], &path_env);

        assert!(got.iter().any(|path| path == &prefix), "{got:?}");
        assert!(got.iter().any(|path| path == &bin), "{got:?}");
        assert!(!got.iter().any(|path| path == &cargo), "{got:?}");
        assert!(!got.iter().any(|path| path == &grok), "{got:?}");
        assert!(!got.iter().any(|path| path == &home), "{got:?}");
        assert!(
            !got.iter().any(|path| path.starts_with(home.join(".aws"))),
            "{got:?}"
        );
        assert!(
            !got.iter().any(|path| path.starts_with(home.join(".ssh"))),
            "{got:?}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_bwrap_runs_npx_when_prefix_is_mounted() {
        use std::os::unix::fs::symlink;
        use std::process::Command;

        if !crate::util::which("bwrap") {
            return;
        }
        let root = mcp_temp("bwrap-npx");
        let home = root.join("home");
        fs::create_dir_all(&home).expect("home");
        let prefix = root.join("node").join("v20");
        let bin = prefix.join("bin");
        let npx_js = prefix.join("lib/node_modules/npm/bin/npx-cli.js");
        fs::create_dir_all(npx_js.parent().expect("parent")).expect("lib");
        fs::create_dir_all(&bin).expect("bin");
        fs::write(&npx_js, "#!/usr/bin/sh\necho NPX-OK\n").expect("npx js");
        symlink("../lib/node_modules/npm/bin/npx-cli.js", bin.join("npx")).expect("npx link");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&npx_js).expect("meta").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&npx_js, perms).expect("chmod");
        }

        let mounts = super::extra_paths_from(
            &home,
            &root.join("cache"),
            &[bin.join("npx").display().to_string()],
            &bin.display().to_string(),
        );
        let mut cmd = Command::new("bwrap");
        cmd.args([
            "--die-with-parent",
            "--ro-bind",
            "/usr",
            "/usr",
            "--ro-bind",
            "/lib",
            "/lib",
            "--ro-bind",
            "/lib64",
            "/lib64",
        ]);
        for mount in mounts.iter().filter(|path| path.is_dir()) {
            let text = mount.display().to_string();
            cmd.args(["--ro-bind", &text, &text]);
        }
        let npx = bin.join("npx");
        cmd.args(["--", &npx.display().to_string()]);
        let output = cmd.output().expect("bwrap");
        let _ = fs::remove_dir_all(&root);
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            if err.contains("namespace") || err.contains("uid map") {
                eprintln!("skip: {err}");
                return;
            }
            panic!("bwrap failed: {err}");
        }
        assert_eq!(String::from_utf8_lossy(&output.stdout), "NPX-OK\n");
    }

    fn mcp_temp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "devcage-mcp-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).expect("mkdir");
        dir
    }
}
