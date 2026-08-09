//! Built-in profiles and their isolation policies.

use serde_json::{json, Value};

use crate::plan::{BackendKind, NetMode};

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

/// Runtime policy for a core profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProfilePolicy {
    /// Profile name.
    pub name: &'static str,
    /// Backend kind.
    pub backend: BackendKind,
    /// Network mode.
    pub net: NetMode,
    /// Emit break-glass warnings.
    pub break_glass: bool,
    /// Core product profile (implemented).
    pub implemented: bool,
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

/// Resolve policy for a core profile name.
pub(crate) fn resolve_policy(name: &str) -> anyhow::Result<ProfilePolicy> {
    match name {
        "agent" => Ok(ProfilePolicy {
            name: "agent",
            backend: BackendKind::Container,
            net: NetMode::Full,
            break_glass: false,
            implemented: true,
        }),
        "offline" => Ok(ProfilePolicy {
            name: "offline",
            backend: BackendKind::Container,
            net: NetMode::None,
            break_glass: false,
            implemented: true,
        }),
        "host" => Ok(ProfilePolicy {
            name: "host",
            backend: BackendKind::Host,
            net: NetMode::Host,
            break_glass: true,
            implemented: true,
        }),
        "untrusted" | "net-allowlist" => anyhow::bail!(
            "profile `{name}` is reserved for a later phase and is not implemented yet"
        ),
        other => anyhow::bail!("unknown profile `{other}`; core profiles: agent, offline, host"),
    }
}

/// JSON document for `devcage profile list`.
#[must_use]
pub(crate) fn profile_list_json() -> Value {
    let mut profiles = Vec::new();
    for p in CORE_PROFILES {
        let implemented = resolve_policy(p.name).is_ok_and(|pol| pol.implemented);
        profiles.push(json!({
            "name": p.name,
            "builtin": true,
            "tier": p.tier,
            "summary": p.summary,
            "implemented": implemented,
        }));
    }
    for p in ROADMAP_PROFILES {
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
    let policy = resolve_policy(name).ok();
    let known = CORE_PROFILES
        .iter()
        .chain(ROADMAP_PROFILES.iter())
        .find(|p| p.name == name);
    json!({
        "name": name,
        "known": known.is_some(),
        "tier": known.map(|p| p.tier),
        "summary": known.map(|p| p.summary),
        "implemented": policy.is_some_and(|p| p.implemented),
        "backend": policy.map(|p| match p.backend {
            BackendKind::Container => "container",
            BackendKind::Host => "host",
        }),
        "network": policy.map(|p| p.net.label()),
        "break_glass": policy.map(|p| p.break_glass),
    })
}

#[cfg(test)]
mod tests {
    use super::{profile_list_json, profile_show_json, resolve_policy, CORE_PROFILES};
    use crate::plan::{BackendKind, NetMode};
    use serde_json::Value;

    #[test]
    fn core_profiles_include_agent_offline_host() {
        let names: Vec<&str> = CORE_PROFILES.iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["agent", "offline", "host"]);
    }

    #[test]
    fn agent_policy_is_container_full_net() {
        let p = resolve_policy("agent").expect("agent");
        assert_eq!(p.backend, BackendKind::Container);
        assert_eq!(p.net, NetMode::Full);
    }

    #[test]
    fn offline_policy_is_net_none() {
        let p = resolve_policy("offline").expect("offline");
        assert_eq!(p.net, NetMode::None);
    }

    #[test]
    fn host_policy_is_break_glass() {
        let p = resolve_policy("host").expect("host");
        assert!(p.break_glass);
        assert_eq!(p.backend, BackendKind::Host);
    }

    #[test]
    fn unknown_profile_errors() {
        assert!(resolve_policy("ios-sim").is_err());
    }

    #[test]
    fn profile_list_json_marks_core_implemented() {
        let v = profile_list_json();
        let arr = v.get("profiles").and_then(Value::as_array).expect("arr");
        let agent = arr.iter().find(|x| x["name"] == "agent").expect("agent");
        assert_eq!(agent["implemented"], true);
    }

    #[test]
    fn profile_show_marks_known_core() {
        let v = profile_show_json("agent");
        assert_eq!(v.get("known").and_then(Value::as_bool), Some(true));
        assert_eq!(v.get("implemented").and_then(Value::as_bool), Some(true));
    }
}
