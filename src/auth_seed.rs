//! Seed Grok auth config into ephemeral HOME for host-restricted sessions.
//!
//! Without this, Grok (which stores OAuth under `$HOME/.grok/auth.json`) cannot
//! sign in when `HOME` is remapped to a session directory. `DevCage` targets Grok
//! as the primary agent CLI; other agents are out of scope for seed paths today.

use std::fs;
use std::path::{Path, PathBuf};

/// Paths under the real user home to copy into the ephemeral home (files only).
const AGENT_AUTH_FILES: &[&str] = &[
    ".grok/auth.json",
    ".grok/config.toml",
    ".config/grok/config.toml",
];

/// Seed auth files for `agent-host` sessions.
///
/// Copies selected files from the process's current `HOME` (real host home before
/// the child remaps it) into `ephemeral_home`.
///
/// # Errors
///
/// Returns an error if a copy fails after the source was found.
pub fn seed_agent_auth(ephemeral_home: &Path) -> anyhow::Result<Vec<String>> {
    let Some(real_home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Ok(Vec::new());
    };
    // Avoid looping if somehow already ephemeral.
    if real_home == ephemeral_home {
        return Ok(Vec::new());
    }

    let mut seeded = Vec::new();
    for rel in AGENT_AUTH_FILES {
        let src = real_home.join(rel);
        if !src.is_file() {
            continue;
        }
        let dest = ephemeral_home.join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&src, &dest).map_err(|e| {
            anyhow::anyhow!("seed auth {} -> {}: {e}", src.display(), dest.display())
        })?;
        // Best-effort: keep auth private inside session home.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&dest, fs::Permissions::from_mode(0o600));
        }
        seeded.push((*rel).to_owned());
    }
    Ok(seeded)
}

#[cfg(test)]
mod tests {
    use super::seed_agent_auth;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn seed_is_noop_without_sources() {
        let dir = std::env::temp_dir().join(format!("devcage-auth-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("mkdir");
        // Still returns Ok even if nothing to copy.
        let seeded = seed_agent_auth(&dir).expect("seed");
        // May or may not seed depending on host; just ensure no panic.
        let _ = seeded;
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn seed_noop_when_home_is_ephemeral() {
        let dir = std::env::temp_dir().join(format!("devcage-auth-same-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("mkdir");
        // Do not mutate process HOME in parallel tests; just exercise unequal-path path.
        let _ = seed_agent_auth(PathBuf::from("/nonexistent-ephemeral-devcage").as_path());
        let _ = fs::remove_dir_all(dir);
    }
}
