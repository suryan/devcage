//! Combined mode: Grok OS sandbox + DevCage-ready environment.
//!
//! `devcage grok` launches the host Grok CLI with a sandbox profile and
//! session data defaults that cooperate with nested `devcage run`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use crate::config::{data_dir_for_grok_sandbox_child, resolve_workspace, Config};
use crate::util::which;

/// Embedded recipes (same as `examples/grok/`).
const EMBEDDED_SANDBOX_TOML: &str = include_str!("../examples/grok/sandbox.toml");
const EMBEDDED_CONFIG_TOML: &str = include_str!("../examples/grok/config.toml");

/// Ambient secrets / agent sockets removed from the Grok child environment.
const SCRUB_ENV: &[&str] = &[
    "SSH_AUTH_SOCK",
    "SSH_AGENT_PID",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_ACCESS_KEY_ID",
    "AWS_SESSION_TOKEN",
    "AWS_SECURITY_TOKEN",
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    // Keep XAI / Grok usable; do not scrub XAI_API_KEY.
];

/// Parameters for [`run_grok_mode`].
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct GrokModeParams {
    /// Grok `--sandbox` profile name (e.g. `keys-first`, `workspace`).
    pub sandbox: String,
    /// Do not create/update `.grok/` recipes.
    pub no_init: bool,
    /// Overwrite existing recipe files when seeding.
    pub force_init: bool,
    /// Allow nested `devcage run -p host` (sets `DEVCAGE_DENY_HOST=0`).
    pub allow_host_profile: bool,
    /// Print command/env only; do not exec Grok.
    pub print_only: bool,
    /// Project workspace (default: cwd / git root).
    pub workspace: Option<PathBuf>,
    /// Extra args forwarded to `grok` after `--sandbox <profile>`.
    pub args: Vec<String>,
}

/// Seed project `.grok/` sandbox + permission recipes from embedded templates.
///
/// # Errors
///
/// Returns an error if directories or files cannot be written.
pub fn init_grok_recipes(workspace: &Path, force: bool) -> anyhow::Result<Vec<String>> {
    let grok_dir = workspace.join(".grok");
    fs::create_dir_all(&grok_dir)?;
    let mut written = Vec::new();
    for (name, body) in [
        ("sandbox.toml", EMBEDDED_SANDBOX_TOML),
        ("config.toml", EMBEDDED_CONFIG_TOML),
    ] {
        let path = grok_dir.join(name);
        if path.exists() && !force {
            continue;
        }
        fs::write(&path, body)?;
        written.push(format!(".grok/{name}"));
    }
    Ok(written)
}

/// Launch Grok in the combined sandbox + DevCage-ready mode.
///
/// # Errors
///
/// Returns an error when the workspace cannot be resolved, recipes fail to seed,
/// or the Grok binary cannot be started.
pub fn run_grok_mode(cfg: &Config, params: GrokModeParams) -> anyhow::Result<ExitCode> {
    let workspace = resolve_workspace(params.workspace)?;
    if !params.no_init {
        let written = init_grok_recipes(&workspace, params.force_init)?;
        if !written.is_empty() {
            eprintln!("devcage: seeded Grok recipes ({})", written.join(", "));
        }
    }

    let grok_bin = resolve_grok_bin(cfg);
    let (data_dir, data_note) = data_dir_for_grok_sandbox_child();
    let data_dir_str = data_dir.display().to_string();
    let deny_host = if params.allow_host_profile { "0" } else { "1" };

    let mut argv = vec![
        grok_bin.clone(),
        "--sandbox".to_owned(),
        params.sandbox.clone(),
    ];
    argv.extend(params.args.iter().cloned());

    if params.print_only {
        print_plan(
            &workspace,
            &data_dir_str,
            deny_host,
            &argv,
            data_note.as_deref().or(cfg.data_dir_note.as_deref()),
        );
        return Ok(ExitCode::SUCCESS);
    }

    if Path::new(&grok_bin) == Path::new("grok") && !which("grok") {
        anyhow::bail!("grok not found on PATH; install Grok Build or set DEVCAGE_DEFAULT_AGENT");
    }

    eprintln!("devcage mode: grok-sandbox");
    eprintln!("  sandbox:   {}", params.sandbox);
    eprintln!("  workspace: {}", workspace.display());
    eprintln!("  data_dir:  {data_dir_str}");
    eprintln!("  deny_host: {deny_host} (profile `host` blocked unless --allow-host-profile)");
    eprintln!("  scrub:     SSH_AUTH_SOCK, AWS_* secrets, GH/OpenAI/Anthropic tokens");
    if let Some(note) = data_note.as_ref().or(cfg.data_dir_note.as_ref()) {
        eprintln!("  note:      {note}");
    }
    eprintln!("  deep cage: devcage run -p offline|untrusted|agent -- <cmd>");
    eprintln!("  limits:    devcage limits");
    eprintln!();

    let mut cmd = Command::new(&grok_bin);
    cmd.arg("--sandbox").arg(&params.sandbox);
    cmd.args(&params.args);
    cmd.current_dir(&workspace);
    cmd.env("DEVCAGE_DATA_DIR", &data_dir_str);
    cmd.env("DEVCAGE_MODE", "grok-sandbox");
    cmd.env("DEVCAGE_GROK_SANDBOX", &params.sandbox);
    cmd.env("DEVCAGE_DENY_HOST", deny_host);
    // Stable PATH: system first, then this binary's dir, then existing PATH.
    cmd.env("PATH", child_path_env());
    for key in SCRUB_ENV {
        cmd.env_remove(*key);
    }
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    let status = cmd
        .status()
        .map_err(|e| anyhow::anyhow!("failed to start `{grok_bin}`: {e}"))?;
    let code = status.code().unwrap_or(1);
    Ok(exit_code_from_i32(code))
}

fn child_path_env() -> String {
    let mut parts: Vec<String> = vec![
        "/usr/bin".into(),
        "/bin".into(),
        "/usr/sbin".into(),
        "/sbin".into(),
        "/opt/homebrew/bin".into(),
        "/usr/local/bin".into(),
    ];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            parts.push(dir.display().to_string());
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let h = PathBuf::from(home);
        parts.push(h.join(".local/bin").display().to_string());
        parts.push(h.join(".grok/bin").display().to_string());
    }
    if let Ok(existing) = std::env::var("PATH") {
        for p in existing.split(':') {
            if !p.is_empty() && !parts.iter().any(|x| x == p) {
                parts.push(p.to_owned());
            }
        }
    }
    parts.join(":")
}

fn resolve_grok_bin(cfg: &Config) -> String {
    let name = cfg.default_agent.as_str();
    if name == "grok" || name.ends_with("/grok") {
        name.to_owned()
    } else {
        "grok".to_owned()
    }
}

fn print_plan(
    workspace: &Path,
    data_dir: &str,
    deny_host: &str,
    argv: &[String],
    note: Option<&str>,
) {
    println!("mode: grok-sandbox");
    println!("workspace: {}", workspace.display());
    println!("DEVCAGE_DATA_DIR={data_dir}");
    println!("DEVCAGE_MODE=grok-sandbox");
    println!("DEVCAGE_DENY_HOST={deny_host}");
    println!("scrub_env: {}", SCRUB_ENV.join(","));
    if let Some(n) = note {
        println!("data_dir_note: {n}");
    }
    println!("command: {}", argv.join(" "));
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
    use super::{init_grok_recipes, EMBEDDED_CONFIG_TOML, EMBEDDED_SANDBOX_TOML};
    use std::fs;

    #[test]
    fn embedded_recipes_nonempty() {
        assert!(EMBEDDED_SANDBOX_TOML.contains("keys-first"));
        assert!(EMBEDDED_SANDBOX_TOML.contains("extends = \"strict\""));
        assert!(EMBEDDED_CONFIG_TOML.contains("devcage run -p host"));
        assert!(EMBEDDED_CONFIG_TOML.contains("[permission]"));
    }

    #[test]
    fn init_writes_once() {
        let dir = std::env::temp_dir().join(format!(
            "devcage-grok-init-{}-{}",
            std::process::id(),
            unique_nanos()
        ));
        fs::create_dir_all(&dir).expect("mkdir");
        let w1 = init_grok_recipes(&dir, false).expect("init");
        assert_eq!(w1.len(), 2);
        let w2 = init_grok_recipes(&dir, false).expect("init again");
        assert!(w2.is_empty());
        let w3 = init_grok_recipes(&dir, true).expect("force");
        assert_eq!(w3.len(), 2);
        assert!(dir.join(".grok/sandbox.toml").is_file());
        assert!(dir.join(".grok/config.toml").is_file());
        let _ = fs::remove_dir_all(dir);
    }

    fn unique_nanos() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    }
}
