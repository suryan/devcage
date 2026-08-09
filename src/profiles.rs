//! Built-in profile catalog (scaffold metadata).

use serde_json::{json, Value};

/// A built-in profile descriptor for `profile list` / docs alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BuiltinProfile {
    /// Profile name (`agent`, `offline`, …).
    pub name: &'static str,
    /// One-line summary for humans and JSON output.
    pub summary: &'static str,
    /// `core` (v0.1) or roadmap tier label.
    pub tier: &'static str,
}

/// Core profiles shipped in the product wedge.
pub(crate) const CORE_PROFILES: &[BuiltinProfile] = &[
    BuiltinProfile {
        name: "agent",
        summary: "Default AI coding: project RW, ephemeral home, no host secrets; network=full (container), labeled honestly",
        tier: "core",
    },
    BuiltinProfile {
        name: "offline",
        summary: "Same FS isolation as agent; network=none (enforced)",
        tier: "core",
    },
    BuiltinProfile {
        name: "host",
        summary: "Break-glass near-host access (explicit only; loud warnings)",
        tier: "core",
    },
];

/// Roadmap profile names (not v0.1 product surface).
pub(crate) const ROADMAP_PROFILES: &[BuiltinProfile] = &[
    BuiltinProfile {
        name: "untrusted",
        summary: "Stricter wipe-oriented session for one-off package experiments",
        tier: "phase2",
    },
    BuiltinProfile {
        name: "net-allowlist",
        summary: "Legacy name / future egress policy — not claimed until enforceable",
        tier: "phase2",
    },
];

/// JSON document for `devcage profile list`.
#[must_use]
pub(crate) fn profile_list_json() -> Value {
    let mut profiles = Vec::new();
    for p in CORE_PROFILES.iter().chain(ROADMAP_PROFILES.iter()) {
        profiles.push(json!({
            "name": p.name,
            "builtin": true,
            "tier": p.tier,
            "summary": p.summary,
            "implemented": false,
        }));
    }
    json!({
        "profiles": profiles,
        "message": "Core profiles: agent, offline, host. GUI/mobile are phase 3 only.",
    })
}

/// JSON document for `devcage profile show`.
#[must_use]
pub(crate) fn profile_show_json(name: &str) -> Value {
    let known = CORE_PROFILES
        .iter()
        .chain(ROADMAP_PROFILES.iter())
        .find(|p| p.name == name);
    json!({
        "name": name,
        "known": known.is_some(),
        "tier": known.map(|p| p.tier),
        "summary": known.map(|p| p.summary),
        "implemented": false,
        "message": "Profile loaders not implemented yet — see docs/architecture.md",
    })
}

#[cfg(test)]
mod tests {
    use super::{profile_list_json, profile_show_json, CORE_PROFILES};
    use serde_json::Value;

    #[test]
    fn core_profiles_include_agent_offline_host() {
        let names: Vec<&str> = CORE_PROFILES.iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["agent", "offline", "host"]);
    }

    #[test]
    fn profile_list_json_has_profiles_array() {
        let v = profile_list_json();
        let Some(arr) = v.get("profiles").and_then(Value::as_array) else {
            unreachable!("profiles array");
        };
        assert!(arr.len() >= 3);
    }

    #[test]
    fn profile_show_marks_known_core() {
        let v = profile_show_json("agent");
        assert_eq!(v.get("known").and_then(Value::as_bool), Some(true));
        assert_eq!(v.get("tier").and_then(Value::as_str), Some("core"));
    }

    #[test]
    fn profile_show_unknown() {
        let v = profile_show_json("not-a-profile");
        assert_eq!(v.get("known").and_then(Value::as_bool), Some(false));
    }
}
