//! High-level run / agent / shell / stop / status / doctor flows.

use std::process::ExitCode;

use serde_json::json;

use crate::backend::{default_shell_command, existing_package_caches, select_backend, ExecRequest};
use crate::banner::print_start_banner;
use crate::config::{resolve_workspace, Config};
use crate::plan::{build_plan, BackendKind};
use crate::profiles::{profile_list_json, profile_show_json, resolve_policy};
use crate::session::{new_session_id, SessionStatus, SessionStore};
use crate::util::which;

/// Run `doctor` JSON report.
pub(crate) fn doctor(cfg: &Config) -> anyhow::Result<()> {
    let docker = which("docker");
    let podman = which("podman");
    let runtime = crate::backend::detect_container_runtime(cfg.container_runtime.as_deref()).ok();
    let report = json!({
        "product": "devcage",
        "version": env!("CARGO_PKG_VERSION"),
        "status": "mvp",
        "focus": "AI agent development isolation",
        "core_profiles": ["agent", "offline", "host"],
        "backends": {
            "container": {
                "mvp": true,
                "docker": docker,
                "podman": podman,
                "selected": runtime.map(crate::backend::ContainerRuntime::bin),
                "image": cfg.container_image,
            },
            "host": { "mvp": true, "break_glass": true },
            "bwrap": { "mvp": false, "planned": true, "available": which("bwrap") },
        },
        "data_dir": cfg.data_dir,
        "notes": [
            "Profiles agent/offline require Docker or Podman (Linux container).",
            "Host-native agent binaries (macOS) only run inside a Linux image if the image provides them — set DEVCAGE_CONTAINER_IMAGE.",
            "Network for profile `agent` is full container net (not an allowlist).",
            "Profile `host` is break-glass with no real isolation.",
        ],
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// List sessions as JSON.
pub(crate) fn status(cfg: &Config) -> anyhow::Result<()> {
    let store = SessionStore::new(&cfg.data_dir);
    let sessions = store.list()?;
    let report = json!({
        "sessions": sessions,
        "data_dir": cfg.data_dir,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// Profile list/show.
pub(crate) fn profile_cmd(action: crate::cli::ProfileCmd) -> anyhow::Result<()> {
    match action {
        crate::cli::ProfileCmd::List => {
            println!("{}", serde_json::to_string_pretty(&profile_list_json())?);
        },
        crate::cli::ProfileCmd::Show { name } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&profile_show_json(&name))?
            );
        },
    }
    Ok(())
}

/// Parameters for a cage run.
#[derive(Debug)]
pub(crate) struct RunParams {
    /// Profile name.
    pub profile: String,
    /// Workspace override.
    pub workspace: Option<std::path::PathBuf>,
    /// Command argv.
    pub command: Vec<String>,
    /// Allocate TTY.
    pub tty: bool,
    /// Wipe ephemeral home after exit.
    pub wipe_after: bool,
}

/// Execute a command in a profile; returns process exit code.
pub(crate) fn run(cfg: &Config, params: RunParams) -> anyhow::Result<ExitCode> {
    let policy = resolve_policy(&params.profile)?;
    if params.command.is_empty() {
        anyhow::bail!("no command specified; example: devcage run -p offline -- echo hi");
    }
    // Fail closed before creating session state.
    let backend = select_backend(policy.backend, cfg.container_runtime.as_deref())?;
    if !backend.available() {
        anyhow::bail!(
            "backend `{}` is not available; run `devcage doctor`",
            backend.name()
        );
    }

    let workspace = resolve_workspace(params.workspace)?;
    let store = SessionStore::new(&cfg.data_dir);
    store.ensure()?;
    let id = new_session_id();
    let home = store.prepare_home(&id)?;
    let caches = existing_package_caches(cfg.share_package_caches);
    let plan = build_plan(
        policy.name,
        policy.backend,
        policy.net,
        workspace.clone(),
        home.clone(),
        &caches,
        policy.break_glass,
    );
    let container_name = match policy.backend {
        BackendKind::Container => Some(format!("devcage-{id}")),
        BackendKind::Host => None,
    };
    let mut session = crate::session::SessionRecord::new(
        id.clone(),
        policy.name,
        policy.backend,
        workspace,
        home,
        container_name,
    );
    store.save(&session)?;

    let image = match policy.backend {
        BackendKind::Container => Some(cfg.container_image.as_str()),
        BackendKind::Host => None,
    };
    print_start_banner(&session, &plan, backend.name(), image);

    let req = ExecRequest {
        session: session.clone(),
        plan,
        command: params.command,
        tty: params.tty,
        image: cfg.container_image.clone(),
    };
    let outcome = backend.exec(&req);
    session.status = SessionStatus::Exited;
    session.exit_code = Some(outcome.as_ref().map_or(1, |o| o.exit_code));
    store.save(&session)?;

    if params.wipe_after {
        let _ = backend.stop(&session);
        store.wipe(&id)?;
        eprintln!("devcage: wiped session {id}");
    }

    let code = outcome?.exit_code;
    Ok(exit_code_from_i32(code))
}

/// `devcage agent` happy path.
pub(crate) fn agent(
    cfg: &Config,
    agent: Option<String>,
    offline: bool,
    workspace: Option<std::path::PathBuf>,
    args: Vec<String>,
) -> anyhow::Result<ExitCode> {
    let profile = if offline { "offline" } else { "agent" };
    let name = agent.unwrap_or_else(|| cfg.default_agent.clone());
    let mut command = vec![name];
    command.extend(args);
    run(
        cfg,
        RunParams {
            profile: profile.to_owned(),
            workspace,
            command,
            tty: true,
            wipe_after: offline,
        },
    )
}

/// Interactive shell in a profile.
pub(crate) fn shell(
    cfg: &Config,
    profile: String,
    workspace: Option<std::path::PathBuf>,
) -> anyhow::Result<ExitCode> {
    run(
        cfg,
        RunParams {
            profile,
            workspace,
            command: default_shell_command(),
            tty: true,
            wipe_after: false,
        },
    )
}

/// Stop a session and optionally wipe.
pub(crate) fn stop(cfg: &Config, session_id: Option<String>, wipe: bool) -> anyhow::Result<()> {
    let store = SessionStore::new(&cfg.data_dir);
    let id = if let Some(id) = session_id {
        id
    } else {
        let list = store.list()?;
        let running = list
            .into_iter()
            .find(|s| s.status == SessionStatus::Running);
        running
            .map(|s| s.id)
            .ok_or_else(|| anyhow::anyhow!("no running session; pass a session id"))?
    };
    let mut rec = store.load(&id)?;
    let kind = match rec.backend.as_str() {
        "host" => BackendKind::Host,
        _ => BackendKind::Container,
    };
    let backend = select_backend(kind, cfg.container_runtime.as_deref());
    if let Ok(backend) = backend {
        backend.stop(&rec)?;
    }
    rec.status = SessionStatus::Stopped;
    store.save(&rec)?;
    if wipe {
        store.wipe(&id)?;
        eprintln!("devcage: stopped and wiped session {id}");
    } else {
        eprintln!("devcage: stopped session {id}");
    }
    Ok(())
}

fn exit_code_from_i32(code: i32) -> ExitCode {
    match u8::try_from(code) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(c) => ExitCode::from(c),
        Err(_) => ExitCode::from(1),
    }
}
