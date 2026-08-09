//! Best-effort macOS Seatbelt (`sandbox-exec`) for host-restricted sessions.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{run_command, Backend, ExecOutcome, ExecRequest};
use crate::session::SessionRecord;
use crate::util::which;

/// Host backend wrapped with an optional Seatbelt profile on macOS.
#[derive(Debug, Clone, Copy, Default)]
pub struct SeatbeltHostBackend;

impl Backend for SeatbeltHostBackend {
    fn name(&self) -> &'static str {
        "seatbelt"
    }

    fn available(&self) -> bool {
        cfg!(target_os = "macos") && which("sandbox-exec")
    }

    fn exec(&self, req: &ExecRequest) -> anyhow::Result<ExecOutcome> {
        let Some((prog, args)) = req.command.split_first() else {
            anyhow::bail!("empty command");
        };

        if !self.available() {
            // Fall through cannot happen if select_backend checks; use plain host path via error.
            anyhow::bail!("sandbox-exec not available");
        }

        let sb_path = write_seatbelt_profile(
            &req.session.id,
            &req.plan.workspace,
            &req.plan.ephemeral_home,
        )?;

        let mut cmd = Command::new("sandbox-exec");
        cmd.arg("-f").arg(&sb_path);
        cmd.arg(prog);
        cmd.args(args);
        cmd.current_dir(&req.plan.workspace);
        cmd.env("HOME", &req.plan.ephemeral_home);
        cmd.env("DEVCAGE_PROFILE", &req.plan.profile);
        cmd.env("DEVCAGE_ISOLATION", "seatbelt");
        for key in super::host::SENSITIVE_ENV {
            cmd.env_remove(*key);
        }
        for (k, v) in &req.plan.env_extras {
            cmd.env(k, v);
        }
        let outcome = run_command(cmd);
        let _ = fs::remove_file(sb_path);
        outcome
    }

    fn stop(&self, _session: &SessionRecord) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Generate a restrictive but workable Seatbelt profile for agent-host.
pub fn write_seatbelt_profile(
    session_id: &str,
    workspace: &Path,
    home: &Path,
) -> anyhow::Result<PathBuf> {
    let dir = std::env::temp_dir().join("devcage-seatbelt");
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{session_id}.sb"));
    let ws = workspace.display().to_string();
    let hm = home.display().to_string();
    // Seatbelt subpath rules need absolute paths without trailing slash issues.
    // Intentionally relatively open for process-exec of host toolchains; deny
    // default still blocks many unexpected writes outside allowlisted subpaths.
    let body = format!(
        r#"(version 1)
(deny default)
(allow process*)
(allow signal)
(allow sysctl-read)
(allow mach*)
(allow ipc-posix*)
(allow network*)
(allow file-read-metadata)
(allow file-read*
  (subpath "/usr")
  (subpath "/System")
  (subpath "/Library")
  (subpath "/bin")
  (subpath "/sbin")
  (subpath "/opt")
  (subpath "/Applications")
  (subpath "/private/etc")
  (subpath "/private/var")
  (subpath "/dev")
  (subpath "/tmp")
  (subpath "{ws}")
  (subpath "{hm}")
)
(allow file-write*
  (subpath "{ws}")
  (subpath "{hm}")
  (subpath "/tmp")
  (subpath "/private/tmp")
  (subpath "/private/var/folders")
  (subpath "/var/folders")
  (subpath "/dev")
)
"#
    );
    fs::write(&path, body)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::write_seatbelt_profile;
    use std::path::PathBuf;

    #[test]
    fn writes_profile_with_paths() {
        let p = write_seatbelt_profile(
            "dc-test",
            PathBuf::from("/tmp/ws").as_path(),
            PathBuf::from("/tmp/home").as_path(),
        )
        .expect("write");
        let s = std::fs::read_to_string(&p).expect("read");
        assert!(s.contains("/tmp/ws"));
        assert!(s.contains("/tmp/home"));
        assert!(s.contains("allow network*"));
        let _ = std::fs::remove_file(p);
    }
}
