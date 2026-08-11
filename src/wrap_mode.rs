//! Generic process wrap: run **any** agent/command under `DevCage` isolation.
//!
//! Unlike [`crate::grok_mode`], this does **not** rely on the agent having
//! `--sandbox`. On macOS it applies `sandbox-exec` (Seatbelt). Elsewhere it
//! still applies env scrub, data dir, and host-deny wiring (weaker FS isolation).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use crate::config::{data_dir_for_grok_sandbox_child, resolve_workspace, Config};
use crate::util::which;

/// Ambient secrets removed from the wrapped process (same idea as `devcage grok`).
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
];

/// Parameters for [`run_wrap`].
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct WrapParams {
    /// Project workspace.
    pub workspace: Option<PathBuf>,
    /// Print plan only.
    pub print_only: bool,
    /// Allow nested `devcage run -p host`.
    pub allow_host_profile: bool,
    /// Skip Seatbelt even on macOS (env wiring only).
    pub no_seatbelt: bool,
    /// Remap `HOME` to an ephemeral session home (stricter; may break agent auth).
    pub ephemeral_home: bool,
    /// Command argv (must be non-empty).
    pub command: Vec<String>,
}

/// Run any command under wrap isolation.
///
/// # Errors
///
/// Empty command, missing binary, workspace resolution, or spawn failure.
#[allow(clippy::needless_pass_by_value)]
pub fn run_wrap(_cfg: &Config, params: WrapParams) -> anyhow::Result<ExitCode> {
    if params.command.is_empty() {
        anyhow::bail!("usage: devcage wrap -- <command> [args…]\nexample: devcage wrap -- claude");
    }
    let workspace = resolve_workspace(params.workspace.clone())?;
    let (data_dir, data_note) = data_dir_for_grok_sandbox_child();
    let data_dir_str = data_dir.display().to_string();
    let deny_host = if params.allow_host_profile { "0" } else { "1" };
    let session_id = new_session_id();
    let eph_home = data_dir.join("sessions").join(&session_id).join("home");
    if params.ephemeral_home {
        fs::create_dir_all(&eph_home)?;
    }

    let nested = already_under_os_sandbox();
    let use_seatbelt = !params.no_seatbelt && seatbelt_available() && !nested;
    let isolation = isolation_label(use_seatbelt, params.no_seatbelt, nested);

    if params.print_only {
        print_plan(
            &workspace,
            &data_dir_str,
            deny_host,
            isolation,
            params.ephemeral_home,
            &params.command,
            data_note.as_deref(),
        );
        return Ok(ExitCode::SUCCESS);
    }

    let Some(prog) = params.command.first().cloned() else {
        anyhow::bail!("empty command");
    };
    if Path::new(&prog).components().count() == 1 && !which(&prog) {
        anyhow::bail!("command `{prog}` not found on PATH");
    }

    print_banner(
        &params,
        &workspace,
        &data_dir_str,
        isolation,
        deny_host,
        &eph_home,
        nested,
        use_seatbelt,
        data_note.as_deref(),
    );

    let seatbelt_profile_path = if use_seatbelt {
        Some(write_wrap_seatbelt_profile(
            &session_id,
            &workspace,
            &data_dir,
            &eph_home,
        )?)
    } else {
        None
    };

    let rest = params.command.get(1..).unwrap_or(&[]);
    let mut cmd = build_command(seatbelt_profile_path.as_deref(), &prog, rest);
    apply_wrap_env(
        &mut cmd,
        &workspace,
        &data_dir_str,
        deny_host,
        isolation,
        params.ephemeral_home,
        &eph_home,
    );

    let status = cmd
        .status()
        .map_err(|e| anyhow::anyhow!("failed to start `{}`: {e}", params.command.join(" ")))?;

    if let Some(p) = seatbelt_profile_path {
        let _ = fs::remove_file(p);
    }
    Ok(exit_code_from_i32(status.code().unwrap_or(1)))
}

fn new_session_id() -> String {
    format!(
        "wrap-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    )
}

const fn isolation_label(use_seatbelt: bool, no_seatbelt: bool, nested: bool) -> &'static str {
    if use_seatbelt {
        "seatbelt"
    } else if no_seatbelt {
        "env-only"
    } else if nested {
        "env-only-nested"
    } else {
        "env-only-no-seatbelt"
    }
}

#[allow(clippy::too_many_arguments)]
fn print_banner(
    params: &WrapParams,
    workspace: &Path,
    data_dir: &str,
    isolation: &str,
    deny_host: &str,
    eph_home: &Path,
    nested: bool,
    use_seatbelt: bool,
    data_note: Option<&str>,
) {
    eprintln!("devcage mode: wrap");
    eprintln!("  command:   {}", params.command.join(" "));
    eprintln!("  workspace: {}", workspace.display());
    eprintln!("  data_dir:  {data_dir}");
    eprintln!("  isolation: {isolation}");
    eprintln!("  deny_host: {deny_host}");
    eprintln!("  scrub:     SSH_AUTH_SOCK, AWS_* secrets, GH/OpenAI/Anthropic tokens");
    if params.ephemeral_home {
        eprintln!("  HOME:      {} (ephemeral)", eph_home.display());
    } else {
        eprintln!("  HOME:      real host home (agent auth/skills keep working)");
    }
    if nested && !params.no_seatbelt {
        eprintln!(
            "  warning:   already under an OS sandbox (e.g. devcage grok); Seatbelt wrap skipped"
        );
    } else if !use_seatbelt && cfg!(target_os = "macos") && !params.no_seatbelt {
        eprintln!("  warning:   sandbox-exec not found; env-only wrap");
    } else if !cfg!(target_os = "macos") && !params.no_seatbelt && !nested {
        eprintln!(
            "  warning:   Seatbelt is macOS-only; Linux bwrap wrap is not enabled yet (env-only)"
        );
    } else {
        // Seatbelt active or user requested env-only: no extra warning.
    }
    if let Some(note) = data_note {
        eprintln!("  note:      {note}");
    }
    eprintln!("  deep cage: devcage run -p offline|untrusted|agent -- <cmd>");
    eprintln!();
}

fn build_command(seatbelt_profile: Option<&Path>, prog: &str, rest: &[String]) -> Command {
    seatbelt_profile.map_or_else(
        || {
            let mut c = Command::new(prog);
            c.args(rest);
            c
        },
        |sb| {
            let mut c = Command::new("sandbox-exec");
            c.arg("-f").arg(sb);
            c.arg(prog);
            c.args(rest);
            c
        },
    )
}

fn apply_wrap_env(
    cmd: &mut Command,
    workspace: &Path,
    data_dir: &str,
    deny_host: &str,
    isolation: &str,
    ephemeral_home: bool,
    eph_home: &Path,
) {
    cmd.current_dir(workspace);
    cmd.env("DEVCAGE_DATA_DIR", data_dir);
    cmd.env("DEVCAGE_MODE", "wrap");
    cmd.env("DEVCAGE_DENY_HOST", deny_host);
    cmd.env("DEVCAGE_ISOLATION", isolation);
    cmd.env("DEVCAGE_WORKSPACE", workspace.display().to_string());
    cmd.env("PATH", child_path_env());
    if ephemeral_home {
        cmd.env("HOME", eph_home);
        cmd.env(
            "DEVCAGE_REAL_HOME",
            std::env::var("HOME").unwrap_or_default(),
        );
    }
    for key in SCRUB_ENV {
        cmd.env_remove(*key);
    }
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
}

fn seatbelt_available() -> bool {
    cfg!(target_os = "macos") && which("sandbox-exec")
}

/// True when nesting `sandbox-exec` would fail (already under Grok/DevCage Seatbelt).
fn already_under_os_sandbox() -> bool {
    matches!(
        std::env::var("DEVCAGE_MODE").ok().as_deref(),
        Some("grok-sandbox" | "wrap")
    ) || std::env::var_os("APP_SANDBOX_CONTAINER_ID").is_some()
}

/// Seatbelt profile for wrap: project + tmp + `data_dir` writable; system/tools readable.
pub fn write_wrap_seatbelt_profile(
    session_id: &str,
    workspace: &Path,
    data_dir: &Path,
    eph_home: &Path,
) -> anyhow::Result<PathBuf> {
    let dir = std::env::temp_dir().join("devcage-seatbelt");
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{session_id}.sb"));

    let ws = workspace.display().to_string();
    let dd = data_dir.display().to_string();
    let eh = eph_home.display().to_string();

    let mut read_subs = vec![
        "/usr".into(),
        "/System".into(),
        "/Library".into(),
        "/bin".into(),
        "/sbin".into(),
        "/opt".into(),
        "/Applications".into(),
        "/private/etc".into(),
        "/private/var".into(),
        "/private/tmp".into(),
        "/tmp".into(),
        "/dev".into(),
        "/var/folders".into(),
        "/private/var/folders".into(),
        ws.clone(),
        dd.clone(),
        eh.clone(),
    ];
    let mut write_subs = vec![
        ws,
        dd,
        eh,
        "/tmp".into(),
        "/private/tmp".into(),
        "/var/folders".into(),
        "/private/var/folders".into(),
        "/dev".into(),
    ];

    if let Some(home) = std::env::var_os("HOME") {
        let h = PathBuf::from(home);
        read_subs.push(h.display().to_string());
        for rel in [
            ".local/bin",
            ".grok",
            ".claude",
            ".cursor",
            ".codex",
            ".config",
            ".cache",
            ".colima",
            ".docker",
            ".cargo",
            ".rustup",
        ] {
            let p = h.join(rel);
            if p.is_dir() || !p.exists() {
                let s = p.display().to_string();
                if !read_subs.contains(&s) {
                    read_subs.push(s.clone());
                }
                if matches!(
                    rel,
                    ".grok" | ".claude" | ".cursor" | ".codex" | ".cache" | ".config"
                ) {
                    write_subs.push(s);
                }
            }
        }
    }

    let read_rules = read_subs
        .iter()
        .map(|p| format!("  (subpath \"{}\")", escape_sb(p)))
        .collect::<Vec<_>>()
        .join("\n");
    let write_rules = write_subs
        .iter()
        .map(|p| format!("  (subpath \"{}\")", escape_sb(p)))
        .collect::<Vec<_>>()
        .join("\n");

    let body = format!(
        "(version 1)\n\
         (deny default)\n\
         (allow process*)\n\
         (allow signal)\n\
         (allow sysctl-read)\n\
         (allow mach*)\n\
         (allow ipc-posix*)\n\
         (allow network*)\n\
         (allow file-read-metadata)\n\
         (allow file-read*\n\
         {read_rules}\n\
         )\n\
         (allow file-write*\n\
         {write_rules}\n\
         )\n"
    );
    fs::write(&path, body)?;
    Ok(path)
}

fn escape_sb(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
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
        parts.push(h.join(".cargo/bin").display().to_string());
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

fn print_plan(
    workspace: &Path,
    data_dir: &str,
    deny_host: &str,
    isolation: &str,
    ephemeral_home: bool,
    command: &[String],
    note: Option<&str>,
) {
    println!("mode: wrap");
    println!("workspace: {}", workspace.display());
    println!("DEVCAGE_DATA_DIR={data_dir}");
    println!("DEVCAGE_MODE=wrap");
    println!("DEVCAGE_DENY_HOST={deny_host}");
    println!("isolation: {isolation}");
    println!("ephemeral_home: {ephemeral_home}");
    println!("scrub_env: {}", SCRUB_ENV.join(","));
    if let Some(n) = note {
        println!("data_dir_note: {n}");
    }
    println!("command: {}", command.join(" "));
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
    use super::write_wrap_seatbelt_profile;
    use std::fs;
    use std::path::Path;

    #[test]
    fn wrap_profile_includes_workspace() {
        let p = write_wrap_seatbelt_profile(
            "wrap-test",
            Path::new("/tmp/ws-wrap"),
            Path::new("/tmp/data-wrap"),
            Path::new("/tmp/home-wrap"),
        )
        .expect("write");
        let s = fs::read_to_string(&p).expect("read");
        assert!(s.contains("/tmp/ws-wrap"));
        assert!(s.contains("/tmp/data-wrap"));
        assert!(s.contains("deny default"));
        let _ = fs::remove_file(p);
    }
}
