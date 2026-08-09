//! High-level run / agent / shell / stop / status / doctor / verify / clean / pull.

use std::process::{Command, ExitCode};

use serde_json::json;

use crate::backend::{
    default_shell_command, detect_container_runtime, existing_package_caches, select_backend,
    ExecRequest,
};
use crate::banner::print_start_banner;
use crate::config::{resolve_workspace, Config};
use crate::plan::{build_plan, ensure_workspace_safe, BackendKind};
use crate::profiles::{profile_list_json, profile_show_json, resolve_policy};
use crate::session::{new_session_id, SessionStatus, SessionStore};
use crate::util::which;

/// Run `doctor` JSON report.
pub(crate) fn doctor(cfg: &Config) -> anyhow::Result<()> {
    let docker = which("docker");
    let podman = which("podman");
    let runtime = detect_container_runtime(cfg.container_runtime.as_deref()).ok();
    let isolated_ready = runtime.is_some();
    let readiness = if isolated_ready {
        "ready"
    } else if which("/usr/bin/true") || which("true") {
        "degraded_host_only"
    } else {
        "not_ready"
    };
    let report = json!({
        "product": "devcage",
        "version": env!("CARGO_PKG_VERSION"),
        "status": "mvp",
        "readiness": readiness,
        "focus": "AI agent development isolation",
        "core_profiles": ["agent", "offline", "untrusted", "host"],
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
            "Profiles agent/offline/untrusted require Docker or Podman.",
            "offline and untrusted use network=none + container hardening (cap-drop ALL).",
            "Host-native macOS agent binaries need a Linux image that provides them, or profile host.",
            "Network for profile `agent` is full container net (not an allowlist).",
            "Run `devcage verify` for isolation smoke checks; `devcage pull` to fetch the image.",
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

/// Pull the configured container image.
pub(crate) fn pull(cfg: &Config) -> anyhow::Result<()> {
    let rt = detect_container_runtime(cfg.container_runtime.as_deref())?;
    eprintln!("devcage: pulling {} with {}", cfg.container_image, rt.bin());
    let status = Command::new(rt.bin())
        .args(["pull", &cfg.container_image])
        .status()?;
    if !status.success() {
        anyhow::bail!(
            "failed to pull image {} (exit {:?})",
            cfg.container_image,
            status.code()
        );
    }
    eprintln!("devcage: pull ok");
    Ok(())
}

/// Wipe exited/stopped sessions (or all with `all`).
pub(crate) fn clean(cfg: &Config, all: bool) -> anyhow::Result<()> {
    let store = SessionStore::new(&cfg.data_dir);
    let sessions = store.list()?;
    let mut wiped = 0_usize;
    for s in sessions {
        let should = all || matches!(s.status, SessionStatus::Exited | SessionStatus::Stopped);
        if !should {
            continue;
        }
        if let Some(name) = s.container_name.as_deref() {
            if let Ok(rt) = detect_container_runtime(cfg.container_runtime.as_deref()) {
                let _ = Command::new(rt.bin()).args(["kill", name]).status();
            }
        }
        store.wipe(&s.id)?;
        wiped += 1;
    }
    eprintln!("devcage: cleaned {wiped} session(s)");
    Ok(())
}

/// Isolation smoke checks (host-side + optional container).
#[allow(clippy::too_many_lines)] // structured multi-check report
pub(crate) fn verify(cfg: &Config) -> anyhow::Result<ExitCode> {
    let mut checks = Vec::new();
    let mut failed = 0_usize;

    // Workspace safety
    match resolve_workspace(None) {
        Ok(ws) => match ensure_workspace_safe(&ws) {
            Ok(()) => checks.push(json!({"id": "workspace_safe", "ok": true, "path": ws})),
            Err(e) => {
                failed += 1;
                checks.push(json!({"id": "workspace_safe", "ok": false, "error": e.to_string()}));
            },
        },
        Err(e) => {
            failed += 1;
            checks.push(json!({"id": "workspace_resolve", "ok": false, "error": e.to_string()}));
        },
    }

    // Sensitive home dirs exist but are not used as default workspace (informational)
    if let Some(home) = std::env::var_os("HOME") {
        let ssh = std::path::PathBuf::from(&home).join(".ssh");
        checks.push(json!({
            "id": "host_ssh_present",
            "ok": true,
            "exists": ssh.is_dir(),
            "note": "cage must not mount this; offline/untrusted use ephemeral HOME only",
        }));
    }

    // Container runtime
    match detect_container_runtime(cfg.container_runtime.as_deref()) {
        Ok(rt) => {
            checks.push(json!({"id": "container_runtime", "ok": true, "runtime": rt.bin()}));
            // Offline network none: run a short container if possible
            let probe = Command::new(rt.bin())
                .args([
                    "run",
                    "--rm",
                    "--network",
                    "none",
                    "--security-opt",
                    "no-new-privileges",
                    "--cap-drop",
                    "ALL",
                    &cfg.container_image,
                    "sh",
                    "-c",
                    "echo ok",
                ])
                .output();
            match probe {
                Ok(out) if out.status.success() => {
                    checks.push(json!({
                        "id": "offline_container_probe",
                        "ok": true,
                        "image": cfg.container_image,
                    }));
                },
                Ok(out) => {
                    failed += 1;
                    checks.push(json!({
                        "id": "offline_container_probe",
                        "ok": false,
                        "stderr": String::from_utf8_lossy(&out.stderr),
                        "hint": "try: devcage pull",
                    }));
                },
                Err(e) => {
                    failed += 1;
                    checks.push(json!({
                        "id": "offline_container_probe",
                        "ok": false,
                        "error": e.to_string(),
                    }));
                },
            }
        },
        Err(e) => {
            checks.push(json!({
                "id": "container_runtime",
                "ok": false,
                "error": e.to_string(),
                "note": "host profile still works; isolated profiles need Docker/Podman",
            }));
            // Not a hard fail for verify when only host is available — but mark degraded
            failed += 1;
        },
    }

    // Host profile can run
    let host_ok = Command::new("/usr/bin/true")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if host_ok {
        checks.push(json!({"id": "host_true", "ok": true}));
    } else {
        failed += 1;
        checks.push(json!({"id": "host_true", "ok": false}));
    }

    let report = json!({
        "product": "devcage",
        "version": env!("CARGO_PKG_VERSION"),
        "failed": failed,
        "checks": checks,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    if failed == 0 {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::from(1))
    }
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
    /// Wipe ephemeral home after exit (OR with profile `always_wipe`).
    pub wipe_after: bool,
    /// Keep session home even when the profile would wipe.
    pub keep: bool,
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
    ensure_workspace_safe(&workspace)?;

    let store = SessionStore::new(&cfg.data_dir);
    store.ensure()?;
    let id = new_session_id();
    let home = store.prepare_home(&id)?;
    let share_caches = cfg.share_package_caches && policy.allow_package_caches;
    let caches = existing_package_caches(share_caches);
    let plan = build_plan(
        policy.name,
        policy.backend,
        policy.net,
        workspace.clone(),
        home.clone(),
        &caches,
        policy.break_glass,
        policy.harden_container,
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

    let wipe = if params.keep {
        false
    } else {
        params.wipe_after || policy.always_wipe
    };
    if wipe {
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
            keep: false,
        },
    )
}

/// Interactive shell in a profile.
pub(crate) fn shell(
    cfg: &Config,
    profile: String,
    workspace: Option<std::path::PathBuf>,
    wipe: bool,
    keep: bool,
) -> anyhow::Result<ExitCode> {
    run(
        cfg,
        RunParams {
            profile,
            workspace,
            command: default_shell_command(),
            tty: true,
            wipe_after: wipe,
            keep,
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

#[cfg(test)]
mod tests {
    use super::{clean, doctor, exit_code_from_i32, profile_cmd, status, stop, verify};
    use crate::cli::ProfileCmd;
    use crate::config::Config;
    use crate::plan::BackendKind;
    use crate::session::{new_session_id, SessionRecord, SessionStatus, SessionStore};
    use std::path::PathBuf;
    use std::process::ExitCode;

    fn temp_cfg() -> (Config, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "devcage-orch-{}-{}",
            std::process::id(),
            new_session_id()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        (Config::for_test(dir.clone()), dir)
    }

    #[test]
    fn doctor_and_status_ok() {
        let (cfg, dir) = temp_cfg();
        doctor(&cfg).expect("doctor");
        status(&cfg).expect("status");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn profile_list_and_show() {
        profile_cmd(ProfileCmd::List).expect("list");
        profile_cmd(ProfileCmd::Show {
            name: "untrusted".to_owned(),
        })
        .expect("show");
    }

    #[test]
    fn clean_wipes_exited_sessions() {
        let (cfg, dir) = temp_cfg();
        let store = SessionStore::new(&cfg.data_dir);
        let id = new_session_id();
        let home = store.prepare_home(&id).expect("home");
        let mut rec = SessionRecord::new(
            id.clone(),
            "host",
            BackendKind::Host,
            PathBuf::from("/ws"),
            home,
            None,
        );
        rec.status = SessionStatus::Exited;
        store.save(&rec).expect("save");
        clean(&cfg, false).expect("clean");
        assert!(store.load(&id).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stop_missing_running_errors() {
        let (cfg, dir) = temp_cfg();
        assert!(stop(&cfg, None, false).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stop_by_id_without_wipe_keeps_meta() {
        let (cfg, dir) = temp_cfg();
        let store = SessionStore::new(&cfg.data_dir);
        let id = new_session_id();
        let home = store.prepare_home(&id).expect("home");
        let rec = SessionRecord::new(
            id.clone(),
            "host",
            BackendKind::Host,
            PathBuf::from("/ws"),
            home,
            None,
        );
        store.save(&rec).expect("save");
        stop(&cfg, Some(id.clone()), false).expect("stop");
        let loaded = store.load(&id).expect("still there");
        assert_eq!(loaded.status, SessionStatus::Stopped);
        stop(&cfg, Some(id.clone()), true).expect("wipe");
        assert!(store.load(&id).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn verify_returns_code() {
        let (cfg, dir) = temp_cfg();
        let code = verify(&cfg).expect("verify");
        // Without docker, exit code is usually 1 (failed checks).
        assert!(code == ExitCode::SUCCESS || code == ExitCode::from(1));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn exit_code_mapping() {
        assert_eq!(exit_code_from_i32(0), ExitCode::SUCCESS);
        assert_eq!(exit_code_from_i32(2), ExitCode::from(2));
        assert_eq!(exit_code_from_i32(-1), ExitCode::from(1));
    }
}
