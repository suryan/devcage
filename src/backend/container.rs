//! Docker / Podman container backend.

use std::path::Path;
use std::process::Command;

use super::{run_command, Backend, ExecOutcome, ExecRequest};
use crate::plan::{IsolationPlan, NetMode};
use crate::session::SessionRecord;
use crate::util::which;

/// Detected container CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContainerRuntime {
    /// Docker CLI.
    Docker,
    /// Podman CLI.
    Podman,
}

impl ContainerRuntime {
    #[must_use]
    pub(crate) const fn bin(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
        }
    }
}

/// Container isolation backend.
#[derive(Debug, Clone)]
pub(crate) struct ContainerBackend {
    runtime: ContainerRuntime,
}

impl ContainerBackend {
    #[must_use]
    pub(crate) const fn new(runtime: ContainerRuntime) -> Self {
        Self { runtime }
    }
}

impl Backend for ContainerBackend {
    fn name(&self) -> &'static str {
        self.runtime.bin()
    }

    fn available(&self) -> bool {
        which(self.runtime.bin())
    }

    fn exec(&self, req: &ExecRequest) -> anyhow::Result<ExecOutcome> {
        if req.command.is_empty() {
            anyhow::bail!("empty command");
        }
        if !self.available() {
            anyhow::bail!(
                "container runtime `{}` not found on PATH; install Docker or Podman, or use --profile host (break-glass)",
                self.runtime.bin()
            );
        }
        let args = build_docker_args(
            req.session.container_name.as_deref(),
            &req.plan,
            &req.image,
            &req.command,
            req.tty,
        );
        let mut cmd = Command::new(self.runtime.bin());
        cmd.args(&args);
        // Map UID for non-root file ownership when possible (Linux hosts).
        #[cfg(unix)]
        {
            // Keep default user inside image unless overridden — alpine root is OK for scaffold;
            // workspace files may be root-owned on Linux; document DEVCAGE later.
        }
        run_command(cmd)
    }

    fn stop(&self, session: &SessionRecord) -> anyhow::Result<()> {
        let Some(name) = session.container_name.as_deref() else {
            return Ok(());
        };
        if !self.available() {
            return Ok(());
        }
        // Best-effort kill; ignore failures if already exited.
        let _ = Command::new(self.runtime.bin())
            .args(["kill", name])
            .status();
        Ok(())
    }
}

/// Detect docker/podman. Prefer explicit override, then podman, then docker.
pub(crate) fn detect_container_runtime(
    preferred: Option<&str>,
) -> anyhow::Result<ContainerRuntime> {
    if let Some(p) = preferred {
        return match p {
            "docker" if which("docker") => Ok(ContainerRuntime::Docker),
            "podman" if which("podman") => Ok(ContainerRuntime::Podman),
            "docker" => anyhow::bail!("DEVCAGE_CONTAINER_RUNTIME=docker but `docker` not on PATH"),
            "podman" => anyhow::bail!("DEVCAGE_CONTAINER_RUNTIME=podman but `podman` not on PATH"),
            other => {
                anyhow::bail!("unknown DEVCAGE_CONTAINER_RUNTIME `{other}` (use docker|podman)")
            },
        };
    }
    if which("podman") {
        return Ok(ContainerRuntime::Podman);
    }
    if which("docker") {
        return Ok(ContainerRuntime::Docker);
    }
    anyhow::bail!(
        "no container runtime on PATH (need `docker` or `podman`); \
         install one or use profile `host` (break-glass, no isolation)"
    )
}

/// Build `docker run …` argv after the binary name.
#[must_use]
pub(crate) fn build_docker_args(
    container_name: Option<&str>,
    plan: &IsolationPlan,
    image: &str,
    command: &[String],
    tty: bool,
) -> Vec<String> {
    let mut args = vec!["run".to_owned(), "--rm".to_owned(), "-i".to_owned()];
    if tty {
        args.push("-t".to_owned());
    }
    if let Some(name) = container_name {
        args.push("--name".to_owned());
        args.push(name.to_owned());
    }
    match plan.net {
        NetMode::None => {
            args.push("--network".to_owned());
            args.push("none".to_owned());
        },
        NetMode::Full | NetMode::Allowlist | NetMode::Host => {
            // Full container network (bridge/default). Allowlist is advisory via env today.
        },
    }
    if plan.harden_container {
        args.push("--security-opt".to_owned());
        args.push("no-new-privileges".to_owned());
        args.push("--cap-drop".to_owned());
        args.push("ALL".to_owned());
    }
    // Workspace
    args.push("-v".to_owned());
    args.push(format!(
        "{}:{}:rw",
        path_for_mount(&plan.workspace),
        plan.workspace_in_cage.display()
    ));
    // Ephemeral home
    args.push("-v".to_owned());
    args.push(format!(
        "{}:{}:rw",
        path_for_mount(&plan.ephemeral_home),
        plan.home_in_cage.display()
    ));
    for m in &plan.extra_mounts {
        let mode = if m.read_only { "ro" } else { "rw" };
        args.push("-v".to_owned());
        args.push(format!(
            "{}:{}:{mode}",
            path_for_mount(&m.source),
            m.target.display()
        ));
    }
    args.push("-w".to_owned());
    args.push(plan.workspace_in_cage.display().to_string());
    args.push("-e".to_owned());
    args.push(format!("HOME={}", plan.home_in_cage.display()));
    for (k, v) in &plan.env_extras {
        args.push("-e".to_owned());
        args.push(format!("{k}={v}"));
    }
    args.push("--".to_owned());
    args.push(image.to_owned());
    for c in command {
        args.push(c.clone());
    }
    args
}

fn path_for_mount(path: &Path) -> String {
    // Docker Desktop on macOS accepts absolute host paths.
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::{build_docker_args, detect_container_runtime, ContainerRuntime};
    use crate::plan::{build_plan, BackendKind, NetMode};
    use std::path::PathBuf;

    #[test]
    fn offline_plan_includes_network_none() {
        let plan = build_plan(
            "offline",
            BackendKind::Container,
            NetMode::None,
            PathBuf::from("/proj"),
            PathBuf::from("/tmp/home"),
            &[],
            false,
            true,
            vec![],
        );
        let args = build_docker_args(
            Some("devcage-test"),
            &plan,
            "alpine:3.20",
            &["echo".to_owned(), "hi".to_owned()],
            false,
        );
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--network" && w[1] == "none"));
        assert!(args.contains(&"--rm".to_owned()));
        assert!(args.iter().any(|a| a.contains("/proj:/workspace:rw")));
        assert!(args.iter().any(|a| a.starts_with("HOME=")));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--cap-drop" && w[1] == "ALL"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--security-opt" && w[1] == "no-new-privileges"));
    }

    #[test]
    fn agent_plan_does_not_force_network_none() {
        let plan = build_plan(
            "agent",
            BackendKind::Container,
            NetMode::Full,
            PathBuf::from("/proj"),
            PathBuf::from("/tmp/home"),
            &[],
            false,
            false,
            vec![],
        );
        let args = build_docker_args(None, &plan, "alpine:3.20", &["sh".to_owned()], true);
        assert!(!args
            .windows(2)
            .any(|w| w[0] == "--network" && w[1] == "none"));
        assert!(args.contains(&"-t".to_owned()));
        assert!(!args.iter().any(|a| a == "--cap-drop"));
    }

    #[test]
    fn detect_runtime_errors_when_none() {
        // With a nonsense preferred runtime name:
        let err = detect_container_runtime(Some("not-a-runtime")).expect_err("bad");
        assert!(err.to_string().contains("unknown"));
    }

    #[test]
    fn runtime_bin_names() {
        assert_eq!(ContainerRuntime::Docker.bin(), "docker");
        assert_eq!(ContainerRuntime::Podman.bin(), "podman");
    }
}
