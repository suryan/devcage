//! Filesystem and network plan derived from a profile.

use std::path::{Path, PathBuf};

/// Network mode applied by a backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetMode {
    /// No network (`offline`).
    None,
    /// Full container network (honest label for `agent`).
    Full,
    /// Domain allowlist attached as env (enforcement phased — advisory until proxy/net filter).
    Allowlist,
    /// Unrestricted host network / host process (`host` profile).
    Host,
}

impl NetMode {
    /// Banner / JSON label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "none (enforced)",
            Self::Full => "full (container) — not an allowlist",
            Self::Allowlist => "allowlist (advisory — not kernel-enforced yet)",
            Self::Host => "host (unrestricted)",
        }
    }
}

/// Backend kind selected for a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// Docker/Podman container cage.
    Container,
    /// Break-glass host process.
    Host,
}

/// Bind mount request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindMount {
    /// Host path.
    pub source: PathBuf,
    /// Path inside the cage.
    pub target: PathBuf,
    /// Read-only mount when true.
    pub read_only: bool,
}

/// Concrete isolation plan for a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsolationPlan {
    /// Profile name.
    pub profile: String,
    /// Backend.
    pub backend: BackendKind,
    /// Network mode.
    pub net: NetMode,
    /// Host workspace (project).
    pub workspace: PathBuf,
    /// Path of workspace inside container (always `/workspace` for container).
    pub workspace_in_cage: PathBuf,
    /// Ephemeral home on host (session data).
    pub ephemeral_home: PathBuf,
    /// Home path inside cage.
    pub home_in_cage: PathBuf,
    /// Extra binds (e.g. RO package caches).
    pub extra_mounts: Vec<BindMount>,
    /// Loud break-glass warning required.
    pub break_glass: bool,
    /// Apply container hardening flags (cap-drop, no-new-privileges).
    pub harden_container: bool,
    /// Extra environment variables for the child (e.g. allowlist).
    pub env_extras: Vec<(String, String)>,
}

/// Input bundle for [`build_plan`] (avoids too-many-arguments).
#[derive(Debug, Clone)]
pub struct PlanRequest {
    /// Profile name.
    pub profile: String,
    /// Backend.
    pub backend: BackendKind,
    /// Network mode.
    pub net: NetMode,
    /// Host workspace.
    pub workspace: PathBuf,
    /// Ephemeral home on host.
    pub ephemeral_home: PathBuf,
    /// Optional RO package cache host paths.
    pub package_cache_sources: Vec<PathBuf>,
    /// Break-glass banner.
    pub break_glass: bool,
    /// Harden container.
    pub harden_container: bool,
    /// Extra env for the child.
    pub env_extras: Vec<(String, String)>,
}

/// Build mount list for the workspace + home + optional caches.
#[must_use]
pub fn build_plan(req: PlanRequest) -> IsolationPlan {
    let mut extra_mounts = Vec::new();
    for (i, src) in req.package_cache_sources.iter().enumerate() {
        if src.is_dir() {
            extra_mounts.push(BindMount {
                source: src.clone(),
                target: PathBuf::from(format!("/cache/pkg{i}")),
                read_only: true,
            });
        }
    }
    IsolationPlan {
        profile: req.profile,
        backend: req.backend,
        net: req.net,
        workspace: req.workspace,
        workspace_in_cage: PathBuf::from("/workspace"),
        ephemeral_home: req.ephemeral_home,
        home_in_cage: PathBuf::from("/home/devcage"),
        extra_mounts,
        break_glass: req.break_glass,
        harden_container: req.harden_container,
        env_extras: req.env_extras,
    }
}

/// Reject using sensitive host directories as the project workspace.
///
/// # Errors
///
/// Returns an error when the workspace path is under a sensitive directory.
pub fn ensure_workspace_safe(workspace: &Path) -> anyhow::Result<()> {
    if is_sensitive_path(workspace) {
        anyhow::bail!(
            "refusing workspace under sensitive path: {} \
             (choose a project directory, not secrets/config dirs)",
            workspace.display()
        );
    }
    for c in workspace.components() {
        if let Some(s) = c.as_os_str().to_str() {
            if matches!(s, ".ssh" | ".gnupg" | ".aws" | ".azure" | ".kube") {
                anyhow::bail!(
                    "refusing workspace path containing `{s}`: {}",
                    workspace.display()
                );
            }
        }
    }
    Ok(())
}

/// Host paths that must never be auto-mounted as the project (secret dirs).
#[must_use]
pub fn is_sensitive_path(path: &Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    matches!(
        name,
        ".ssh" | ".gnupg" | ".aws" | ".azure" | ".config" | ".kube" | "Library"
    ) || path
        .components()
        .any(|c| matches!(c.as_os_str().to_str(), Some(".ssh" | ".gnupg" | ".aws")))
}

/// Default package cache locations under the user home (RO share candidates).
#[must_use]
pub fn default_package_cache_candidates(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".npm"),
        home.join(".cache").join("npm"),
        home.join(".cache").join("uv"),
        home.join(".cache").join("pip"),
        home.join(".local").join("share").join("uv"),
        home.join(".cargo").join("registry"),
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        build_plan, default_package_cache_candidates, ensure_workspace_safe, is_sensitive_path,
        BackendKind, NetMode, PlanRequest,
    };
    use std::path::PathBuf;

    #[test]
    fn net_labels_are_honest() {
        assert!(NetMode::Full.label().contains("not an allowlist"));
        assert!(NetMode::None.label().contains("none"));
    }

    #[test]
    fn sensitive_paths_detected() {
        assert!(is_sensitive_path(PathBuf::from("/home/u/.ssh").as_path()));
        assert!(is_sensitive_path(PathBuf::from("/home/u/.aws").as_path()));
        assert!(!is_sensitive_path(
            PathBuf::from("/home/u/projects/app").as_path()
        ));
    }

    #[test]
    fn ensure_workspace_safe_rejects_ssh() {
        assert!(ensure_workspace_safe(PathBuf::from("/home/u/.ssh").as_path()).is_err());
        assert!(ensure_workspace_safe(PathBuf::from("/home/u/projects/app").as_path()).is_ok());
    }

    #[test]
    fn build_plan_adds_ro_caches_when_dirs_exist() {
        let dir = std::env::temp_dir().join(format!("devcage-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let plan = build_plan(PlanRequest {
            profile: "agent".to_owned(),
            backend: BackendKind::Container,
            net: NetMode::Full,
            workspace: PathBuf::from("/proj"),
            ephemeral_home: PathBuf::from("/tmp/home"),
            package_cache_sources: vec![dir.clone()],
            break_glass: false,
            harden_container: false,
            env_extras: vec![],
        });
        assert_eq!(plan.extra_mounts.len(), 1);
        assert!(plan.extra_mounts[0].read_only);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn package_cache_candidates_nonempty() {
        let c = default_package_cache_candidates(PathBuf::from("/home/dev").as_path());
        assert!(c.len() >= 3);
    }
}
