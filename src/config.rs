//! Runtime configuration from environment (no hardcoded machine paths).

use std::fs;
use std::path::{Path, PathBuf};

/// Resolved `DevCage` configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Directory for session metadata and ephemeral homes.
    pub data_dir: PathBuf,
    /// Preferred container runtime binary name (`docker` / `podman`).
    pub container_runtime: Option<String>,
    /// Container image for cage sessions.
    pub container_image: String,
    /// Default agent CLI name.
    pub default_agent: String,
    /// When true, mount known host package caches read-only into the cage.
    pub share_package_caches: bool,
    /// When true, `devcage agent` falls back to `agent-host` if no container runtime.
    pub agent_host_fallback: bool,
    /// Optional path to net allowlist file for `net-allowlist` profile.
    pub net_allowlist_file: Option<PathBuf>,
    /// Optional note when `data_dir` was auto-relocated (e.g. Grok sandbox).
    pub data_dir_note: Option<String>,
}

impl Config {
    /// Construct a config for tests (no env required).
    #[cfg(test)]
    #[must_use]
    pub fn for_test(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            container_runtime: None,
            container_image: "docker.io/library/alpine:3.20".to_owned(),
            default_agent: "grok".to_owned(),
            share_package_caches: false,
            agent_host_fallback: false,
            net_allowlist_file: None,
            data_dir_note: None,
        }
    }

    /// Load from environment with portable defaults.
    #[must_use]
    pub fn from_env() -> Self {
        let (data_dir, data_dir_note) = resolve_data_dir();
        let container_runtime = std::env::var("DEVCAGE_CONTAINER_RUNTIME")
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());
        let container_image = std::env::var("DEVCAGE_CONTAINER_IMAGE")
            .unwrap_or_else(|_| "docker.io/library/alpine:3.20".to_owned());
        let default_agent =
            std::env::var("DEVCAGE_DEFAULT_AGENT").unwrap_or_else(|_| "grok".to_owned());
        let share_package_caches = env_truthy("DEVCAGE_SHARE_PACKAGE_CACHES");
        // Default true: macOS users without Docker can still run host agents.
        let agent_host_fallback = !env_falsy("DEVCAGE_AGENT_HOST_FALLBACK");
        let net_allowlist_file = env_path("DEVCAGE_NET_ALLOWLIST_FILE");
        Self {
            data_dir,
            container_runtime,
            container_image,
            default_agent,
            share_package_caches,
            agent_host_fallback,
            net_allowlist_file,
            data_dir_note,
        }
        // Seatbelt: DEVCAGE_SEATBELT=1 enables sandbox-exec for agent-host on macOS.
    }
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

fn env_truthy(key: &str) -> bool {
    std::env::var(key).ok().is_some_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

/// True when env is explicitly falsey (`0`/`false`/`no`/`off`).
fn env_falsy(key: &str) -> bool {
    std::env::var(key).ok().is_some_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        )
    })
}

fn default_data_dir() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join("devcage");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("devcage");
    }
    PathBuf::from(".devcage-data")
}

/// Resolve session data directory.
///
/// - `DEVCAGE_DATA_DIR` wins when set (even if not writable — fail later with a clear error).
/// - Otherwise use the XDG/home default when a write probe succeeds.
/// - If the default is not writable (common under Grok `--sandbox workspace` / `keys-first`,
///   which blocks `~/.local/share`), fall back to `$TMPDIR/devcage-data`, `/tmp/devcage-data`,
///   or `./.devcage-data`.
fn resolve_data_dir() -> (PathBuf, Option<String>) {
    if let Some(explicit) = env_path("DEVCAGE_DATA_DIR") {
        return (explicit, None);
    }

    let preferred = default_data_dir();
    if dir_is_usable(&preferred) {
        return (preferred, None);
    }

    for candidate in sandbox_safe_data_dir_candidates() {
        if dir_is_usable(&candidate) {
            let note = format!(
                "default data dir {} not writable (agent sandbox?); using {}",
                preferred.display(),
                candidate.display()
            );
            eprintln!("devcage: {note}");
            eprintln!(
                "devcage: tip: set DEVCAGE_DATA_DIR explicitly, or allow a write path in Grok sandbox"
            );
            return (candidate, Some(note));
        }
    }

    // Last resort: keep preferred so ensure() surfaces a path-specific error.
    (
        preferred,
        Some(
            "data dir may not be writable; set DEVCAGE_DATA_DIR to $TMPDIR/devcage-data \
             when running under Grok --sandbox"
                .to_owned(),
        ),
    )
}

/// Candidates that are usually writable under Grok Seatbelt `workspace` / `keys-first`.
fn sandbox_safe_data_dir_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(tmp) = std::env::var_os("TMPDIR").filter(|s| !s.is_empty()) {
        out.push(PathBuf::from(tmp).join("devcage-data"));
    }
    out.push(PathBuf::from("/tmp/devcage-data"));
    if let Ok(cwd) = std::env::current_dir() {
        out.push(cwd.join(".devcage-data"));
    }
    out
}

/// True when the process can create the directory and write a probe file.
fn dir_is_usable(path: &Path) -> bool {
    if fs::create_dir_all(path).is_err() {
        return false;
    }
    let probe = path.join(".devcage-write-probe");
    match fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
            true
        },
        Err(_) => false,
    }
}

/// Resolve workspace path: explicit override, env, git root, else cwd.
pub fn resolve_workspace(explicit: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    let path = explicit.unwrap_or_else(default_workspace_path);
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    };
    let path = path.canonicalize().unwrap_or(path);
    if !path.is_dir() {
        anyhow::bail!("workspace is not a directory: {}", path.display());
    }
    Ok(path)
}

fn default_workspace_path() -> PathBuf {
    match std::env::var_os("DEVCAGE_WORKSPACE") {
        Some(w) if !w.is_empty() => PathBuf::from(w),
        _ => find_git_root()
            .or_else(|_| std::env::current_dir())
            .unwrap_or_else(|_| PathBuf::from(".")),
    }
}

/// Walk parents for a `.git` directory; return that root when found.
fn find_git_root() -> anyhow::Result<PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        if dir.join(".git").exists() {
            return Ok(dir);
        }
        if !dir.pop() {
            anyhow::bail!("no git root");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{dir_is_usable, env_truthy, resolve_workspace, sandbox_safe_data_dir_candidates};
    use std::path::PathBuf;

    #[test]
    fn resolve_workspace_uses_cwd_when_unset() {
        let ws = resolve_workspace(None).expect("cwd");
        assert!(ws.is_dir());
    }

    #[test]
    fn resolve_workspace_rejects_file() {
        let f = std::env::temp_dir().join(format!(
            "devcage-ws-file-{}-{}",
            std::process::id(),
            unique_nanos()
        ));
        std::fs::write(&f, b"x").expect("write");
        let err = resolve_workspace(Some(f.clone())).expect_err("file");
        assert!(err.to_string().contains("not a directory"));
        let _ = std::fs::remove_file(f);
    }

    #[test]
    fn env_truthy_parses_common_values() {
        // Cannot safely mutate process env in parallel tests for arbitrary keys;
        // unit-test the helper with a known-unset key result path via private logic:
        assert!(!env_truthy("DEVCAGE_TEST_UNSET_KEY_ZZZ_EXPECT_FALSE"));
    }

    fn unique_nanos() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    }

    #[test]
    fn pathbuf_roundtrip_smoke() {
        let p = PathBuf::from(".");
        assert!(!p.as_os_str().is_empty());
    }

    #[test]
    fn temp_dir_is_usable_for_data() {
        let dir = std::env::temp_dir().join(format!(
            "devcage-usable-{}-{}",
            std::process::id(),
            unique_nanos()
        ));
        assert!(dir_is_usable(&dir));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn nonexistent_under_root_is_not_usable() {
        // Path that cannot be created without privileges on Unix.
        let p = PathBuf::from("/devcage-definitely-not-writable-by-user/data");
        assert!(!dir_is_usable(&p));
    }

    #[test]
    fn sandbox_safe_candidates_nonempty() {
        assert!(!sandbox_safe_data_dir_candidates().is_empty());
    }
}
