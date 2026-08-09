//! Filesystem and network plan derived from a profile.

use std::path::{Path, PathBuf};

/// Network mode applied by a backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NetMode {
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
    pub(crate) const fn label(self) -> &'static str {
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
pub(crate) enum BackendKind {
    /// Docker/Podman container cage.
    Container,
    /// Break-glass host process.
    Host,
}

/// Bind mount request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BindMount {
    /// Host path.
    pub source: PathBuf,
    /// Path inside the cage.
    pub target: PathBuf,
    /// Read-only mount when true.
    pub read_only: bool,
}

/// Concrete isolation plan for a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IsolationPlan {
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

/// Build mount list for the workspace + home + optional caches.
#[must_use]
#[allow(clippy::too_many_arguments)] // plan builder packs orthogonal fields
pub(crate) fn build_plan(
    profile: &str,
    backend: BackendKind,
    net: NetMode,
    workspace: PathBuf,
    ephemeral_home: PathBuf,
    package_cache_sources: &[PathBuf],
    break_glass: bool,
    harden_container: bool,
    env_extras: Vec<(String, String)>,
) -> IsolationPlan {
    let mut extra_mounts = Vec::new();
    for (i, src) in package_cache_sources.iter().enumerate() {
        if src.is_dir() {
            extra_mounts.push(BindMount {
                source: src.clone(),
                target: PathBuf::from(format!("/cache/pkg{i}")),
                read_only: true,
            });
        }
    }
    IsolationPlan {
        profile: profile.to_owned(),
        backend,
        net,
        workspace,
        workspace_in_cage: PathBuf::from("/workspace"),
        ephemeral_home,
        home_in_cage: PathBuf::from("/home/devcage"),
        extra_mounts,
        break_glass,
        harden_container,
        env_extras,
    }
}

/// Reject using sensitive host directories as the project workspace.
pub(crate) fn ensure_workspace_safe(workspace: &Path) -> anyhow::Result<()> {
    if is_sensitive_path(workspace) {
        anyhow::bail!(
            "refusing workspace under sensitive path: {} \
             (choose a project directory, not secrets/config dirs)",
            workspace.display()
        );
    }
    // Also refuse if any path component is a classic secret directory name.
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
pub(crate) fn is_sensitive_path(path: &Path) -> bool {
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
pub(crate) fn default_package_cache_candidates(home: &Path) -> Vec<PathBuf> {
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
        BackendKind, NetMode,
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
        let plan = build_plan(
            "agent",
            BackendKind::Container,
            NetMode::Full,
            PathBuf::from("/proj"),
            PathBuf::from("/tmp/home"),
            std::slice::from_ref(&dir),
            false,
            false,
            vec![],
        );
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
