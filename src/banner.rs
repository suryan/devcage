//! Human-readable start banner (security UX).

use crate::plan::IsolationPlan;
use crate::session::SessionRecord;

/// Print the session start summary to stderr.
pub(crate) fn print_start_banner(
    session: &SessionRecord,
    plan: &IsolationPlan,
    backend_name: &str,
    image: Option<&str>,
) {
    eprintln!("devcage session");
    eprintln!("  id:        {}", session.id);
    eprintln!("  profile:   {}", plan.profile);
    eprintln!("  backend:   {backend_name}");
    if let Some(img) = image {
        eprintln!("  image:     {img}");
    }
    eprintln!("  workspace: {} (rw)", plan.workspace.display());
    eprintln!("  home:      ephemeral ({})", plan.ephemeral_home.display());
    eprintln!("  network:   {}", plan.net.label());
    eprintln!("  secrets:   not mounted");
    if plan.profile == "agent-host" {
        if backend_name == "seatbelt" {
            eprintln!(
                "  WARNING:   host-restricted + Seatbelt (sandbox-exec) — stronger than plain host, not a container"
            );
        } else {
            eprintln!(
                "  WARNING:   host-restricted mode — ephemeral HOME + env scrub only; process runs on the host OS (not a container)"
            );
        }
    } else if plan.break_glass {
        eprintln!("  WARNING:   break-glass host profile — little to no isolation");
    } else if plan.net.label().contains("advisory") {
        eprintln!(
            "  WARNING:   net-allowlist is advisory (env only) — full container network still available"
        );
    } else {
        // Container / non-break-glass profiles: no extra warning.
    }
    if !plan.extra_mounts.is_empty() {
        eprintln!(
            "  caches:    {} RO host package cache mount(s)",
            plan.extra_mounts.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::print_start_banner;
    use crate::plan::{build_plan, BackendKind, NetMode};
    use crate::session::SessionRecord;
    use std::path::PathBuf;

    #[test]
    fn banner_does_not_panic() {
        let plan = build_plan(
            "offline",
            BackendKind::Container,
            NetMode::None,
            PathBuf::from("/ws"),
            PathBuf::from("/home"),
            &[],
            false,
            true,
            vec![],
        );
        let session = SessionRecord::new(
            "dc-test".to_owned(),
            "offline",
            BackendKind::Container,
            PathBuf::from("/ws"),
            PathBuf::from("/home"),
            Some("devcage-dc-test".to_owned()),
        );
        print_start_banner(&session, &plan, "docker", Some("alpine:3.20"));
    }
}
