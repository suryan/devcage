//! High-level run / agent / shell / stop / status / doctor / verify / clean / pull.

use std::process::{Command, ExitCode};

use serde_json::json;

use crate::backend::{
    default_shell_command, detect_container_runtime, existing_package_caches, select_backend,
    ExecRequest,
};
use crate::banner::print_start_banner;
use crate::config::{resolve_workspace, Config};
use crate::plan::{build_plan, ensure_workspace_safe, BackendKind, PlanRequest};
use crate::profiles::{profile_list_json, profile_show_json, resolve_policy};
use crate::session::{new_session_id, SessionStatus, SessionStore};
use crate::util::which;

/// Explain isolation plan for a profile without running anything.
pub fn explain(
    cfg: &Config,
    profile: &str,
    workspace: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    let policy = resolve_policy(profile)?;
    let workspace = resolve_workspace(workspace)?;
    ensure_workspace_safe(&workspace)?;
    let backend = select_backend(
        policy.backend,
        cfg.container_runtime.as_deref(),
        policy.name,
    );
    let backend_name = backend.as_ref().map(|b| b.name()).unwrap_or("unavailable");
    let backend_ok = backend.as_ref().is_ok_and(|b| b.available());
    let share_caches = cfg.share_package_caches && policy.allow_package_caches();
    let caches = existing_package_caches(share_caches);
    let mut env_extras = Vec::new();
    if policy.net == crate::plan::NetMode::Allowlist {
        if let Ok(al) = load_allowlist(cfg) {
            env_extras.push(("DEVCAGE_NET_ALLOWLIST".to_owned(), al.joined()));
            env_extras.push(("DEVCAGE_NET_ENFORCEMENT".to_owned(), "advisory".to_owned()));
        }
    }
    let plan = build_plan(PlanRequest {
        profile: policy.name.to_owned(),
        backend: policy.backend,
        net: policy.net,
        workspace,
        ephemeral_home: cfg.data_dir.join("sessions").join("<id>").join("home"),
        package_cache_sources: caches,
        break_glass: policy.break_glass(),
        harden_container: policy.harden_container(),
        env_extras,
    });
    let report = json!({
        "profile": policy.name,
        "backend": backend_name,
        "backend_available": backend_ok,
        "network": plan.net.label(),
        "workspace": plan.workspace,
        "ephemeral_home_pattern": plan.ephemeral_home,
        "harden_container": plan.harden_container,
        "always_wipe": policy.always_wipe(),
        "allow_package_caches": policy.allow_package_caches(),
        "break_glass": policy.break_glass(),
        "extra_mounts": plan.extra_mounts.iter().map(|m| json!({
            "source": m.source,
            "target": m.target,
            "read_only": m.read_only,
        })).collect::<Vec<_>>(),
        "env_extras": plan.env_extras,
        "image": (policy.backend == BackendKind::Container).then_some(&cfg.container_image),
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// Run `doctor` JSON report.
pub fn doctor(cfg: &Config) -> anyhow::Result<()> {
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
        "core_profiles": ["agent", "agent-host", "offline", "untrusted", "net-allowlist", "host"],
        "agent_host_fallback": cfg.agent_host_fallback,
        "net_allowlist_file": cfg.net_allowlist_file,
        "seatbelt": {
            "planned": true,
            "macos_agent_host": cfg!(target_os = "macos"),
            "sandbox_exec": which("sandbox-exec"),
        },
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
            "Profiles agent/offline/untrusted require Docker or Podman (unless agent host-fallback).",
            "devcage agent uses agent-host when no container runtime and DEVCAGE_AGENT_HOST_FALLBACK is not off.",
            "agent-host: ephemeral HOME + env scrub on the host OS — not a container.",
            "offline and untrusted use network=none + container hardening (cap-drop ALL).",
            "Network for profile `agent` is full container net (not an allowlist).",
            "See docs/agent-images.md for container images.",
        ],
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// List sessions as JSON.
pub fn status(cfg: &Config) -> anyhow::Result<()> {
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
pub fn profile_cmd(action: crate::cli::ProfileCmd) -> anyhow::Result<()> {
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
pub fn pull(cfg: &Config) -> anyhow::Result<()> {
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
pub fn clean(cfg: &Config, all: bool) -> anyhow::Result<()> {
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
pub fn verify(cfg: &Config) -> anyhow::Result<ExitCode> {
    let mut checks = Vec::new();
    let mut failed = 0_usize;
    failed += push_workspace_checks(&mut checks);
    push_ssh_note(&mut checks);
    failed += push_container_checks(cfg, &mut checks);
    failed += push_host_true_check(&mut checks);

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

fn push_workspace_checks(checks: &mut Vec<serde_json::Value>) -> usize {
    match resolve_workspace(None) {
        Ok(ws) => match ensure_workspace_safe(&ws) {
            Ok(()) => {
                checks.push(json!({"id": "workspace_safe", "ok": true, "path": ws}));
                0
            },
            Err(e) => {
                checks.push(json!({"id": "workspace_safe", "ok": false, "error": e.to_string()}));
                1
            },
        },
        Err(e) => {
            checks.push(json!({"id": "workspace_resolve", "ok": false, "error": e.to_string()}));
            1
        },
    }
}

fn push_ssh_note(checks: &mut Vec<serde_json::Value>) {
    if let Some(home) = std::env::var_os("HOME") {
        let ssh = std::path::PathBuf::from(&home).join(".ssh");
        checks.push(json!({
            "id": "host_ssh_present",
            "ok": true,
            "exists": ssh.is_dir(),
            "note": "cage must not mount this; offline/untrusted use ephemeral HOME only",
        }));
    }
}

fn push_container_checks(cfg: &Config, checks: &mut Vec<serde_json::Value>) -> usize {
    match detect_container_runtime(cfg.container_runtime.as_deref()) {
        Ok(rt) => {
            checks.push(json!({"id": "container_runtime", "ok": true, "runtime": rt.bin()}));
            push_offline_probe(cfg, rt.bin(), checks)
        },
        Err(e) => {
            checks.push(json!({
                "id": "container_runtime",
                "ok": false,
                "error": e.to_string(),
                "note": "host profile still works; isolated profiles need Docker/Podman",
            }));
            1
        },
    }
}

fn push_offline_probe(
    cfg: &Config,
    runtime_bin: &str,
    checks: &mut Vec<serde_json::Value>,
) -> usize {
    let probe = Command::new(runtime_bin)
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
            0
        },
        Ok(out) => {
            checks.push(json!({
                "id": "offline_container_probe",
                "ok": false,
                "stderr": String::from_utf8_lossy(&out.stderr),
                "hint": "try: devcage pull",
            }));
            1
        },
        Err(e) => {
            checks.push(json!({
                "id": "offline_container_probe",
                "ok": false,
                "error": e.to_string(),
            }));
            1
        },
    }
}

fn push_host_true_check(checks: &mut Vec<serde_json::Value>) -> usize {
    let host_ok = Command::new("/usr/bin/true")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if host_ok {
        checks.push(json!({"id": "host_true", "ok": true}));
        0
    } else {
        checks.push(json!({"id": "host_true", "ok": false}));
        1
    }
}

/// Parameters for a cage run.
#[derive(Debug)]
pub struct RunParams {
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
///
/// # Errors
///
/// Returns an error when the profile cannot be applied, the backend is missing,
/// or the child process cannot be started.
pub fn run(cfg: &Config, params: RunParams) -> anyhow::Result<ExitCode> {
    let policy = resolve_policy(&params.profile)?;
    if params.command.is_empty() {
        anyhow::bail!("no command specified; example: devcage run -p offline -- echo hi");
    }
    let backend = select_backend(
        policy.backend,
        cfg.container_runtime.as_deref(),
        policy.name,
    )?;
    if !backend.available() {
        anyhow::bail!(
            "backend `{}` is not available; run `devcage doctor`",
            backend.name()
        );
    }

    let workspace = resolve_workspace(params.workspace)?;
    ensure_workspace_safe(&workspace)?;
    let env_extras = allowlist_env_extras(cfg, policy.net)?;

    let store = SessionStore::new(&cfg.data_dir);
    store.ensure()?;
    let id = new_session_id();
    let home = store.prepare_home(&id)?;
    let share_caches = cfg.share_package_caches && policy.allow_package_caches();
    let caches = existing_package_caches(share_caches);
    let plan = build_plan(PlanRequest {
        profile: policy.name.to_owned(),
        backend: policy.backend,
        net: policy.net,
        workspace: workspace.clone(),
        ephemeral_home: home.clone(),
        package_cache_sources: caches,
        break_glass: policy.break_glass(),
        harden_container: policy.harden_container(),
        env_extras,
    });
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
    let _ = crate::events::append_event(
        &cfg.data_dir,
        &id,
        "start",
        format!("profile={} backend={}", policy.name, backend.name()),
        None,
    );

    let image = (policy.backend == BackendKind::Container).then_some(cfg.container_image.as_str());
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
    let _ = crate::events::append_event(
        &cfg.data_dir,
        &id,
        "exit",
        format!("code={}", session.exit_code.unwrap_or(1)),
        None,
    );

    let wipe = !params.keep && (params.wipe_after || policy.always_wipe());
    if wipe {
        let _ = backend.stop(&session);
        store.wipe(&id)?;
        eprintln!("devcage: wiped session {id}");
    }

    let code = outcome?.exit_code;
    Ok(exit_code_from_i32(code))
}

fn allowlist_env_extras(
    cfg: &Config,
    net: crate::plan::NetMode,
) -> anyhow::Result<Vec<(String, String)>> {
    if net != crate::plan::NetMode::Allowlist {
        return Ok(Vec::new());
    }
    let al = load_allowlist(cfg)?;
    if al.hosts.is_empty() {
        anyhow::bail!(
            "profile net-allowlist requires a non-empty allowlist file \
             (set DEVCAGE_NET_ALLOWLIST_FILE); see examples/net-allowlist.txt"
        );
    }
    eprintln!(
        "devcage: net-allowlist advisory ({} hosts) — not kernel-enforced yet",
        al.hosts.len()
    );
    Ok(vec![
        ("DEVCAGE_NET_ALLOWLIST".to_owned(), al.joined()),
        ("DEVCAGE_NET_ENFORCEMENT".to_owned(), "advisory".to_owned()),
    ])
}

fn load_allowlist(cfg: &Config) -> anyhow::Result<crate::net_policy::NetAllowlist> {
    let Some(path) = cfg.net_allowlist_file.as_ref() else {
        // Ship default embedded example hosts if no file set.
        return Ok(crate::net_policy::NetAllowlist::parse(
            crate::net_policy::default_allowlist_example(),
        ));
    };
    crate::net_policy::NetAllowlist::load_file(path)
}

/// `devcage agent` happy path.
pub fn agent(
    cfg: &Config,
    agent: Option<String>,
    offline: bool,
    host_restricted: bool,
    workspace: Option<std::path::PathBuf>,
    args: Vec<String>,
) -> anyhow::Result<ExitCode> {
    if offline && host_restricted {
        anyhow::bail!("--offline and --host-restricted are mutually exclusive");
    }
    let profile = if offline {
        "offline".to_owned()
    } else if host_restricted {
        "agent-host".to_owned()
    } else if container_runtime_available(cfg) {
        "agent".to_owned()
    } else if cfg.agent_host_fallback {
        eprintln!(
            "devcage: no container runtime; falling back to agent-host \
             (set DEVCAGE_AGENT_HOST_FALLBACK=0 to disable)"
        );
        "agent-host".to_owned()
    } else {
        anyhow::bail!(
            "no container runtime for profile `agent`; install Docker/Podman, \
             use `devcage agent --host-restricted`, or set DEVCAGE_AGENT_HOST_FALLBACK=1"
        );
    };
    let name = agent.unwrap_or_else(|| cfg.default_agent.clone());
    let mut command = vec![name];
    command.extend(args);
    run(
        cfg,
        RunParams {
            profile,
            workspace,
            command,
            tty: true,
            wipe_after: offline,
            keep: false,
        },
    )
}

fn container_runtime_available(cfg: &Config) -> bool {
    detect_container_runtime(cfg.container_runtime.as_deref()).is_ok()
}

/// Print session event log (JSON lines).
pub fn logs(cfg: &Config, session_id: &str) -> anyhow::Result<()> {
    let text = crate::events::read_events(&cfg.data_dir, session_id)?;
    if text.is_empty() {
        eprintln!("devcage: no events for session {session_id}");
        return Ok(());
    }
    print!("{text}");
    Ok(())
}

/// Write the default net-allowlist example to a path.
pub fn init_allowlist(path: &std::path::Path) -> anyhow::Result<()> {
    if path.exists() {
        anyhow::bail!("refusing to overwrite existing file: {}", path.display());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, crate::net_policy::default_allowlist_example())?;
    eprintln!("devcage: wrote allowlist example {}", path.display());
    Ok(())
}

/// Copy a file/dir from a session home (or under workspace) to a host destination.
pub fn export(
    cfg: &Config,
    session_id: &str,
    source: &std::path::Path,
    dest: &std::path::Path,
) -> anyhow::Result<()> {
    let store = SessionStore::new(&cfg.data_dir);
    let rec = store.load(session_id)?;
    let resolved = resolve_export_source(&rec, source)?;
    if !resolved.exists() {
        anyhow::bail!("source does not exist: {}", resolved.display());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if resolved.is_dir() {
        copy_dir_recursive(&resolved, dest)?;
    } else {
        std::fs::copy(&resolved, dest)?;
    }
    eprintln!(
        "devcage: exported {} -> {}",
        resolved.display(),
        dest.display()
    );
    Ok(())
}

fn resolve_export_source(
    rec: &crate::session::SessionRecord,
    source: &std::path::Path,
) -> anyhow::Result<std::path::PathBuf> {
    if source.is_absolute() {
        // Must stay under session home or workspace.
        let under_home = source.starts_with(&rec.home);
        let under_ws = source.starts_with(&rec.workspace);
        if under_home || under_ws {
            return Ok(source.to_path_buf());
        }
        anyhow::bail!(
            "absolute export source must be under session home or workspace\n  home={}\n  workspace={}",
            rec.home.display(),
            rec.workspace.display()
        );
    }
    let from_home = rec.home.join(source);
    if from_home.exists() {
        return Ok(from_home);
    }
    let from_ws = rec.workspace.join(source);
    if from_ws.exists() {
        return Ok(from_ws);
    }
    anyhow::bail!(
        "relative source not found under session home or workspace: {}",
        source.display()
    )
}

fn copy_dir_recursive(src: &std::path::Path, dest: &std::path::Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dest)?;
    for ent in std::fs::read_dir(src)? {
        let ent = ent?;
        let ty = ent.file_type()?;
        let to = dest.join(ent.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&ent.path(), &to)?;
        } else if ty.is_file() {
            std::fs::copy(ent.path(), to)?;
        } else {
            // Skip symlinks/specials for export safety.
        }
    }
    Ok(())
}

/// Interactive shell in a profile.
pub fn shell(
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
pub fn stop(cfg: &Config, session_id: Option<String>, wipe: bool) -> anyhow::Result<()> {
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
    let backend = select_backend(kind, cfg.container_runtime.as_deref(), rec.profile.as_str());
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

    #[test]
    fn export_from_session_home() {
        use super::export;
        let (cfg, dir) = temp_cfg();
        let store = SessionStore::new(&cfg.data_dir);
        let id = new_session_id();
        let home = store.prepare_home(&id).expect("home");
        let artifact = home.join("out.txt");
        std::fs::write(&artifact, b"hello").expect("write");
        let rec = SessionRecord::new(
            id.clone(),
            "agent-host",
            BackendKind::Host,
            PathBuf::from("/ws"),
            home,
            None,
        );
        store.save(&rec).expect("save");
        let dest = dir.join("exported.txt");
        export(&cfg, &id, PathBuf::from("out.txt").as_path(), &dest).expect("export");
        assert_eq!(std::fs::read_to_string(dest).expect("read"), "hello");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn agent_host_restricted_runs_true() {
        use super::agent;
        let (cfg, dir) = temp_cfg();
        // Use /usr/bin/true as the "agent"
        let code = agent(
            &cfg,
            Some("/usr/bin/true".to_owned()),
            false,
            true,
            Some(std::env::current_dir().expect("cwd")),
            vec![],
        )
        .expect("agent");
        assert_eq!(code, ExitCode::SUCCESS);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn logs_and_init_allowlist() {
        use super::{init_allowlist, logs, run, RunParams};
        let (cfg, dir) = temp_cfg();
        let code = run(
            &cfg,
            RunParams {
                profile: "agent-host".to_owned(),
                workspace: Some(std::env::current_dir().expect("cwd")),
                command: vec!["/usr/bin/true".to_owned()],
                tty: false,
                wipe_after: false,
                keep: true,
            },
        )
        .expect("run");
        assert_eq!(code, ExitCode::SUCCESS);
        let store = SessionStore::new(&cfg.data_dir);
        let sessions = store.list().expect("list");
        let id = sessions[0].id.clone();
        logs(&cfg, &id).expect("logs");
        let al = dir.join("allow.txt");
        init_allowlist(&al).expect("init");
        assert!(al.is_file());
        assert!(init_allowlist(&al).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_allowlist_embedded_default() {
        use super::load_allowlist;
        let (cfg, dir) = temp_cfg();
        let al = load_allowlist(&cfg).expect("load");
        assert!(!al.hosts.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn explain_offline_json() {
        use super::explain;
        let (cfg, dir) = temp_cfg();
        explain(&cfg, "offline", Some(std::env::current_dir().expect("cwd"))).expect("explain");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn net_allowlist_profile_requires_hosts() {
        use super::{run, RunParams};
        let (mut cfg, dir) = temp_cfg();
        // Empty custom file should fail
        let empty = dir.join("empty.txt");
        std::fs::write(&empty, "# only comments\n").expect("write");
        cfg.net_allowlist_file = Some(empty);
        let err = run(
            &cfg,
            RunParams {
                profile: "net-allowlist".to_owned(),
                workspace: Some(std::env::current_dir().expect("cwd")),
                command: vec!["/usr/bin/true".to_owned()],
                tty: false,
                wipe_after: true,
                keep: false,
            },
        )
        .expect_err("empty allowlist");
        assert!(err.to_string().contains("allowlist") || err.to_string().contains("container"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
