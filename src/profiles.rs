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
    /// `core` or roadmap tier label.
    pub tier: &'static str,
}

/// Runtime policy for a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // policy flags map 1:1 to profile knobs
pub(crate) struct ProfilePolicy {
    /// Profile name.
    pub name: &'static str,
    /// Backend kind.
    pub backend: BackendKind,
    /// Network mode.
    pub net: NetMode,
    /// Emit break-glass warnings.
    pub break_glass: bool,
    /// Always wipe ephemeral home after exit.
    pub always_wipe: bool,
    /// Allow optional RO host package cache mounts.
    pub allow_package_caches: bool,
    /// Drop Linux capabilities / no-new-privileges in container.
    pub harden_container: bool,
    /// Implemented in this release.
    pub implemented: bool,
}

/// Core + implemented profiles.
pub(crate) const CORE_PROFILES: &[BuiltinProfile] = &[
    BuiltinProfile {
        name: "agent",
        summary: "Default AI coding in a Linux container; network=full (container), labeled honestly",
        tier: "core",
    },
    BuiltinProfile {
        name: "agent-host",
        summary: "Host-restricted agent: ephemeral HOME + env scrub on the host OS (not a container)",
        tier: "core",
    },
    BuiltinProfile {
        name: "offline",
        summary: "Same FS isolation as agent; network=none (enforced); wipe by default",
        tier: "core",
    },
    BuiltinProfile {
        name: "untrusted",
        summary: "Strict install/experiment cage: network=none, hardened container, always wipe, no host caches",
        tier: "core",
    },
    BuiltinProfile {
        name: "host",
        summary: "Break-glass near-host access (explicit only; loud warnings)",
        tier: "core",
    },
];

/// Roadmap profile names (not implemented).
pub(crate) const ROADMAP_PROFILES: &[BuiltinProfile] = &[BuiltinProfile {
    name: "net-allowlist",
    summary: "Future egress policy — not claimed until enforceable",
    tier: "phase2",
}];

/// Resolve policy for a profile name.
pub(crate) fn resolve_policy(name: &str) -> anyhow::Result<ProfilePolicy> {
    match name {
        "agent" => Ok(ProfilePolicy {
            name: "agent",
            backend: BackendKind::Container,
            net: NetMode::Full,
            break_glass: false,
            always_wipe: false,
            allow_package_caches: true,
            harden_container: false,
            implemented: true,
        }),
        "agent-host" => Ok(ProfilePolicy {
            name: "agent-host",
            backend: BackendKind::Host,
            net: NetMode::Host,
            break_glass: true,
            always_wipe: false,
            allow_package_caches: true,
            harden_container: false,
            implemented: true,
        }),
        "offline" => Ok(ProfilePolicy {
            name: "offline",
            backend: BackendKind::Container,
            net: NetMode::None,
            break_glass: false,
            always_wipe: true,
            allow_package_caches: true,
            harden_container: true,
            implemented: true,
        }),
        "untrusted" => Ok(ProfilePolicy {
            name: "untrusted",
            backend: BackendKind::Container,
            net: NetMode::None,
            break_glass: false,
            always_wipe: true,
            allow_package_caches: false,
            harden_container: true,
            implemented: true,
        }),
        "host" => Ok(ProfilePolicy {
            name: "host",
            backend: BackendKind::Host,
            net: NetMode::Host,
            break_glass: true,
            always_wipe: false,
            allow_package_caches: false,
            harden_container: false,
            implemented: true,
        }),
        "net-allowlist" => anyhow::bail!(
            "profile `net-allowlist` is reserved until egress allowlisting is enforceable"
        ),
        other => anyhow::bail!(
            "unknown profile `{other}`; core profiles: agent, agent-host, offline, untrusted, host"
        ),
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
        "message": "Core: agent, agent-host, offline, untrusted, host. net-allowlist is phase 2.",
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
        "always_wipe": policy.map(|p| p.always_wipe),
        "allow_package_caches": policy.map(|p| p.allow_package_caches),
        "harden_container": policy.map(|p| p.harden_container),
    })
}

#[cfg(test)]
mod tests {
    use super::{profile_list_json, profile_show_json, resolve_policy, CORE_PROFILES};
    use crate::plan::{BackendKind, NetMode};
    use serde_json::Value;

    #[test]
    fn core_profiles_include_untrusted() {
        let names: Vec<&str> = CORE_PROFILES.iter().map(|p| p.name).collect();
        assert!(names.contains(&"untrusted"));
        assert!(names.contains(&"agent"));
    }

    #[test]
    fn untrusted_is_strict() {
        let p = resolve_policy("untrusted").expect("untrusted");
        assert_eq!(p.net, NetMode::None);
        assert!(p.always_wipe);
        assert!(!p.allow_package_caches);
        assert!(p.harden_container);
        assert_eq!(p.backend, BackendKind::Container);
    }

    #[test]
    fn agent_host_is_host_backend() {
        let p = resolve_policy("agent-host").expect("agent-host");
        assert_eq!(p.backend, BackendKind::Host);
        assert!(p.break_glass);
    }

    #[test]
    fn offline_wipes_by_default() {
        let p = resolve_policy("offline").expect("offline");
        assert!(p.always_wipe);
        assert!(p.harden_container);
    }

    #[test]
    fn agent_policy_is_container_full_net() {
        let p = resolve_policy("agent").expect("agent");
        assert_eq!(p.backend, BackendKind::Container);
        assert_eq!(p.net, NetMode::Full);
        assert!(!p.always_wipe);
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
    fn profile_list_json_marks_untrusted_implemented() {
        let v = profile_list_json();
        let arr = v.get("profiles").and_then(Value::as_array).expect("arr");
        let u = arr.iter().find(|x| x["name"] == "untrusted").expect("u");
        assert_eq!(u["implemented"], true);
    }

    #[test]
    fn profile_show_marks_known_core() {
        let v = profile_show_json("agent");
        assert_eq!(v.get("known").and_then(Value::as_bool), Some(true));
        assert_eq!(v.get("implemented").and_then(Value::as_bool), Some(true));
    }
}
