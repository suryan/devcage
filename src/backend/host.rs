//! Break-glass host backend (no isolation).

use std::process::Command;

use super::{run_command, Backend, ExecOutcome, ExecRequest};
use crate::session::SessionRecord;

/// Host process backend — explicit break-glass only.
#[derive(Debug, Clone, Copy, Default)]
pub struct HostBackend;

/// Env keys cleared for host-profile children (defense in depth only).
pub const SENSITIVE_ENV: &[&str] = &[
    "AWS_SECRET_ACCESS_KEY",
    "AWS_ACCESS_KEY_ID",
    "AWS_SESSION_TOKEN",
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "SSH_AUTH_SOCK",
];

impl Backend for HostBackend {
    fn name(&self) -> &'static str {
        "host"
    }

    fn available(&self) -> bool {
        true
    }

    fn exec(&self, req: &ExecRequest) -> anyhow::Result<ExecOutcome> {
        let Some((prog, args)) = req.command.split_first() else {
            anyhow::bail!("empty command");
        };
        let mut cmd = Command::new(prog);
        cmd.args(args);
        cmd.current_dir(&req.plan.workspace);
        // Ephemeral HOME so tools prefer session home when they honor $HOME.
        cmd.env("HOME", &req.plan.ephemeral_home);
        cmd.env("DEVCAGE_PROFILE", &req.plan.profile);
        if req.plan.profile == "agent-host" {
            cmd.env("DEVCAGE_ISOLATION", "host-restricted");
        } else {
            cmd.env("DEVCAGE_BREAK_GLASS", "1");
            cmd.env("DEVCAGE_ISOLATION", "host");
        }
        // Best-effort: drop common secret env vars from the child (not a real sandbox).
        for key in SENSITIVE_ENV {
            cmd.env_remove(*key);
        }
        for (k, v) in &req.plan.env_extras {
            cmd.env(k, v);
        }
        run_command(cmd)
    }

    fn stop(&self, _session: &SessionRecord) -> anyhow::Result<()> {
        // Foreground host processes are not tracked as daemons in v0.1.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::HostBackend;
    use crate::backend::Backend;

    #[test]
    fn host_backend_always_available() {
        let b = HostBackend;
        assert!(b.available());
        assert_eq!(b.name(), "host");
    }
}
