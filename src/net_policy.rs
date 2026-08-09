//! Network allowlist policy loading (enforcement is phased).

use std::fs;
use std::path::Path;

/// Loaded allowlist (domains / hostnames).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct NetAllowlist {
    /// Allowed hostnames (exact or suffix with leading `.`).
    pub hosts: Vec<String>,
}

impl NetAllowlist {
    /// Load from a text file: one host per line, `#` comments, blank lines skipped.
    pub(crate) fn load_file(path: &Path) -> anyhow::Result<Self> {
        let text = fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("read allowlist {}: {e}", path.display()))?;
        Ok(Self::parse(&text))
    }

    /// Parse allowlist body.
    #[must_use]
    pub(crate) fn parse(text: &str) -> Self {
        let mut hosts = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // Take first token only.
            let host = line.split_whitespace().next().unwrap_or("");
            if !host.is_empty() {
                hosts.push(host.to_ascii_lowercase());
            }
        }
        hosts.sort();
        hosts.dedup();
        Self { hosts }
    }

    /// Whether `host` matches an allowlist entry (exact or suffix).
    #[must_use]
    #[cfg_attr(not(test), allow(dead_code))] // used by future enforcement + tests
    pub(crate) fn allows(&self, host: &str) -> bool {
        let host = host.to_ascii_lowercase();
        self.hosts.iter().any(|entry| {
            if entry.starts_with('.') {
                host.ends_with(entry.as_str()) || host == entry.trim_start_matches('.')
            } else {
                host == *entry || host.ends_with(&format!(".{entry}"))
            }
        })
    }

    /// Join for env / display.
    #[must_use]
    pub(crate) fn joined(&self) -> String {
        self.hosts.join(",")
    }
}

/// Default starter list path relative to config (documented only unless set).
#[must_use]
pub(crate) const fn default_allowlist_example() -> &'static str {
    r"# DevCage net allowlist example (one host per line)
# Enforcement is phase 2 — loaded lists are attached as DEVCAGE_NET_ALLOWLIST env.
api.openai.com
api.anthropic.com
api.x.ai
github.com
registry.npmjs.org
pypi.org
files.pythonhosted.org
"
}

#[cfg(test)]
mod tests {
    use super::NetAllowlist;

    #[test]
    fn parse_and_match() {
        let al =
            NetAllowlist::parse("# c\napi.openai.com\n.github.com\n\nregistry.npmjs.org extra\n");
        assert!(al.allows("api.openai.com"));
        assert!(al.allows("foo.github.com"));
        assert!(al.allows("github.com"));
        assert!(!al.allows("evil.com"));
        assert!(al.joined().contains("api.openai.com"));
    }

    #[test]
    fn empty_denies_all() {
        let al = NetAllowlist::default();
        assert!(!al.allows("anything.com"));
    }
}
