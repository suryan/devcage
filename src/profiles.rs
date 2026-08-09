//! Built-in profiles and their isolation policies.

use serde_json::{json, Value};

use crate::plan::{BackendKind, NetMode};

/// A built-in profile descriptor for `profile list` / docs alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinProfile {
    /// Profile name (`agent`, `offline`, …).
    pub name: &'static str,
    /// One-line summary for humans and JSON output.
    pub summary: &'static str,
    /// `core` or roadmap tier label.
    pub tier: &'static str,
}

/// Whether a session home is wiped after exit by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WipeMode {
    /// Always discard ephemeral home after the process exits.
    Always,
    /// Keep unless the user passes `--wipe`.
    OnRequest,
}

/// Whether host package caches may be RO-mounted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheShare {
    /// Optional RO share of named host caches when enabled in config.
    Allow,
    /// Never share host package caches.
    Deny,
}

/// Container hardening preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardenMode {
    /// Apply `--cap-drop ALL` and `no-new-privileges`.
    Hardened,
    /// Default container privileges for the image.
    Default,
}

/// Host trust framing for banners and docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustMode {
    /// Contained / profiled isolation path.
    Contained,
    /// Explicit elevated / break-glass path.
    BreakGlass,
}

/// Implementation status of a built-in name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShipStatus {
    /// Available to run.
    Ready,
}

/// Runtime policy for a profile (enums instead of bool soup — pedantic-friendly).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfilePolicy {
    /// Profile name.
    pub name: &'static str,
    /// Backend kind.
    pub backend: BackendKind,
    /// Network mode.
    pub net: NetMode,
    /// Trust framing.
    pub trust: TrustMode,
    /// Wipe policy.
    pub wipe: WipeMode,
    /// Host cache share policy.
    pub caches: CacheShare,
    /// Container harden policy.
    pub harden: HardenMode,
    /// Ship status.
    pub status: ShipStatus,
}

impl ProfilePolicy {
    /// Loud break-glass warning required.
    #[must_use]
    pub const fn break_glass(self) -> bool {
        matches!(self.trust, TrustMode::BreakGlass)
    }

    /// Always wipe after exit.
    #[must_use]
    pub const fn always_wipe(self) -> bool {
        matches!(self.wipe, WipeMode::Always)
    }

    /// Allow RO package cache mounts when config enables sharing.
    #[must_use]
    pub const fn allow_package_caches(self) -> bool {
        matches!(self.caches, CacheShare::Allow)
    }

    /// Harden container flags.
    #[must_use]
    pub const fn harden_container(self) -> bool {
        matches!(self.harden, HardenMode::Hardened)
    }

    /// Profile is implemented.
    #[must_use]
    pub const fn implemented(self) -> bool {
        matches!(self.status, ShipStatus::Ready)
    }
}

/// Core + implemented profiles.
pub const CORE_PROFILES: &[BuiltinProfile] = &[
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
    BuiltinProfile {
        name: "net-allowlist",
        summary: "Container with domain list attached as DEVCAGE_NET_ALLOWLIST (advisory until phase-2 enforcement)",
        tier: "core",
    },
];

/// Reserved names not yet implemented.
pub const ROADMAP_PROFILES: &[BuiltinProfile] = &[];

/// Resolve policy for a profile name.
///
/// # Errors
///
/// Returns an error when the profile name is unknown.
pub fn resolve_policy(name: &str) -> anyhow::Result<ProfilePolicy> {
    match name {
        "agent" => Ok(ProfilePolicy {
            name: "agent",
            backend: BackendKind::Container,
            net: NetMode::Full,
            trust: TrustMode::Contained,
            wipe: WipeMode::OnRequest,
            caches: CacheShare::Allow,
            harden: HardenMode::Default,
            status: ShipStatus::Ready,
        }),
        "agent-host" => Ok(ProfilePolicy {
            name: "agent-host",
            backend: BackendKind::Host,
            net: NetMode::Host,
            trust: TrustMode::BreakGlass,
            wipe: WipeMode::OnRequest,
            caches: CacheShare::Allow,
            harden: HardenMode::Default,
            status: ShipStatus::Ready,
        }),
        "offline" => Ok(ProfilePolicy {
            name: "offline",
            backend: BackendKind::Container,
            net: NetMode::None,
            trust: TrustMode::Contained,
            wipe: WipeMode::Always,
            caches: CacheShare::Allow,
            harden: HardenMode::Hardened,
            status: ShipStatus::Ready,
        }),
        "untrusted" => Ok(ProfilePolicy {
            name: "untrusted",
            backend: BackendKind::Container,
            net: NetMode::None,
            trust: TrustMode::Contained,
            wipe: WipeMode::Always,
            caches: CacheShare::Deny,
            harden: HardenMode::Hardened,
            status: ShipStatus::Ready,
        }),
        "host" => Ok(ProfilePolicy {
            name: "host",
            backend: BackendKind::Host,
            net: NetMode::Host,
            trust: TrustMode::BreakGlass,
            wipe: WipeMode::OnRequest,
            caches: CacheShare::Deny,
            harden: HardenMode::Default,
            status: ShipStatus::Ready,
        }),
        "net-allowlist" => Ok(ProfilePolicy {
            name: "net-allowlist",
            backend: BackendKind::Container,
            net: NetMode::Allowlist,
            trust: TrustMode::Contained,
            wipe: WipeMode::OnRequest,
            caches: CacheShare::Allow,
            harden: HardenMode::Default,
            status: ShipStatus::Ready,
        }),
        other => anyhow::bail!(
            "unknown profile `{other}`; core: agent, agent-host, offline, untrusted, net-allowlist, host"
        ),
    }
}

/// JSON document for `devcage profile list`.
#[must_use]
pub fn profile_list_json() -> Value {
    let mut profiles = Vec::new();
    for p in CORE_PROFILES {
        let implemented = resolve_policy(p.name).is_ok_and(ProfilePolicy::implemented);
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
        "message": "Core: agent, agent-host, offline, untrusted, net-allowlist (advisory), host.",
    })
}

/// JSON document for `devcage profile show`.
#[must_use]
pub fn profile_show_json(name: &str) -> Value {
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
        "implemented": policy.is_some_and(ProfilePolicy::implemented),
        "backend": policy.map(|p| match p.backend {
            BackendKind::Container => "container",
            BackendKind::Host => "host",
        }),
        "network": policy.map(|p| p.net.label()),
        "break_glass": policy.map(ProfilePolicy::break_glass),
        "always_wipe": policy.map(ProfilePolicy::always_wipe),
        "allow_package_caches": policy.map(ProfilePolicy::allow_package_caches),
        "harden_container": policy.map(ProfilePolicy::harden_container),
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
        assert!(p.always_wipe());
        assert!(!p.allow_package_caches());
        assert!(p.harden_container());
        assert_eq!(p.backend, BackendKind::Container);
    }

    #[test]
    fn agent_host_is_host_backend() {
        let p = resolve_policy("agent-host").expect("agent-host");
        assert_eq!(p.backend, BackendKind::Host);
        assert!(p.break_glass());
    }

    #[test]
    fn offline_wipes_by_default() {
        let p = resolve_policy("offline").expect("offline");
        assert!(p.always_wipe());
        assert!(p.harden_container());
    }

    #[test]
    fn agent_policy_is_container_full_net() {
        let p = resolve_policy("agent").expect("agent");
        assert_eq!(p.backend, BackendKind::Container);
        assert_eq!(p.net, NetMode::Full);
        assert!(!p.always_wipe());
    }

    #[test]
    fn host_policy_is_break_glass() {
        let p = resolve_policy("host").expect("host");
        assert!(p.break_glass());
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
