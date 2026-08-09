//! Isolation backends.

mod container;
mod host;

use std::path::PathBuf;
use std::process::Command;

pub(crate) use container::{detect_container_runtime, ContainerBackend, ContainerRuntime};
pub(crate) use host::HostBackend;

use crate::plan::{BackendKind, IsolationPlan};
use crate::session::SessionRecord;

/// Request to execute a command inside an isolation plan.
#[derive(Debug, Clone)]
pub(crate) struct ExecRequest {
    /// Session metadata.
    pub session: SessionRecord,
    /// Isolation plan.
    pub plan: IsolationPlan,
    /// Command argv (non-empty).
    pub command: Vec<String>,
    /// Allocate a TTY when true (shell / interactive agent).
    pub tty: bool,
    /// Container image (container backend).
    pub image: String,
}

/// Outcome of an execution attempt.
#[derive(Debug, Clone)]
pub(crate) struct ExecOutcome {
    /// Process exit code (or 1 if terminated by signal).
    pub exit_code: i32,
}

/// Common backend operations.
pub(crate) trait Backend {
    /// Human name for banners / doctor.
    fn name(&self) -> &'static str;

    /// Whether this backend can run now.
    fn available(&self) -> bool;

    /// Run a command to completion (foreground).
    fn exec(&self, req: &ExecRequest) -> anyhow::Result<ExecOutcome>;

    /// Stop a session's running workload if any.
    fn stop(&self, session: &SessionRecord) -> anyhow::Result<()>;
}

/// Pick a backend implementation for a plan.
pub(crate) fn select_backend(
    kind: BackendKind,
    preferred_runtime: Option<&str>,
) -> anyhow::Result<Box<dyn Backend>> {
    match kind {
        BackendKind::Container => {
            let rt = detect_container_runtime(preferred_runtime)?;
            Ok(Box::new(ContainerBackend::new(rt)))
        },
        BackendKind::Host => Ok(Box::new(HostBackend)),
    }
}

/// Helper used by backends to run a command inheriting stdio.
pub(crate) fn run_command(mut cmd: Command) -> anyhow::Result<ExecOutcome> {
    let status = cmd.status()?;
    let exit_code = status.code().unwrap_or(1);
    Ok(ExecOutcome { exit_code })
}

/// Default shell command inside a cage when user asks for `shell`.
#[must_use]
pub(crate) fn default_shell_command() -> Vec<String> {
    vec!["/bin/sh".to_owned(), "-l".to_owned()]
}

/// Resolve package cache dirs that exist on this host.
#[must_use]
pub(crate) fn existing_package_caches(share: bool) -> Vec<PathBuf> {
    if !share {
        return Vec::new();
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let Some(home) = home else {
        return Vec::new();
    };
    crate::plan::default_package_cache_candidates(&home)
        .into_iter()
        .filter(|p| p.is_dir())
        .collect()
}
