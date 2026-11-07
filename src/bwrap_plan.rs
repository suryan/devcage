//! Linux `bwrap` argument plans for [`crate::wrap_mode`] and [`crate::mcp_mode`].
//!
//! Pure argv construction so it can be **unit-tested on macOS**. Execution only
//! happens on Linux when `bwrap` is on `PATH`.

use std::path::{Path, PathBuf};

/// Inputs for a wrap-style bubblewrap launch.
#[derive(Debug, Clone)]
pub struct BwrapPlan {
    /// Project workspace (read-write bind).
    pub workspace: PathBuf,
    /// `DevCage` data dir (read-write bind).
    pub data_dir: PathBuf,
    /// Ephemeral home when remapped (read-write bind).
    pub ephemeral_home: Option<PathBuf>,
    /// Deny network (`--unshare-net`).
    pub net_none: bool,
    /// Guest argv (`prog` + args).
    pub command: Vec<String>,
}

/// Build `bwrap` argv (program name first: `bwrap`).
///
/// # Errors
///
/// Empty guest command.
pub fn bwrap_argv(plan: &BwrapPlan) -> anyhow::Result<Vec<String>> {
    let Some(prog) = plan.command.first() else {
        anyhow::bail!("bwrap plan has empty command");
    };

    let mut argv = vec![
        "bwrap".to_owned(),
        "--die-with-parent".to_owned(),
        "--unshare-pid".to_owned(),
        "--unshare-uts".to_owned(),
        "--unshare-ipc".to_owned(),
        "--proc".to_owned(),
        "/proc".to_owned(),
        "--dev".to_owned(),
        "/dev".to_owned(),
        "--tmpfs".to_owned(),
        "/tmp".to_owned(),
    ];
    if plan.net_none {
        argv.push("--unshare-net".to_owned());
    }

    // Host system (read-only). Missing paths are skipped at exec time by the caller
    // if needed; the plan always lists common Linux prefixes.
    for p in ["/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc"] {
        argv.extend(ro_bind_if_abs(p));
    }

    argv.extend(rw_bind(&plan.workspace));
    argv.extend(rw_bind(&plan.data_dir));
    if let Some(ref home) = plan.ephemeral_home {
        argv.extend(rw_bind(home));
    }

    if let Some(real_home) = std::env::var_os("HOME") {
        let h = PathBuf::from(real_home);
        for rel in [
            ".local/bin",
            ".grok",
            ".claude",
            ".cursor",
            ".codex",
            ".cargo",
            ".rustup",
        ] {
            let p = h.join(rel);
            if p.is_dir() {
                argv.extend(rw_bind(&p));
            }
        }
        // Tooling only (read-only): never bind whole $HOME.
        for rel in [".colima", ".docker"] {
            let p = h.join(rel);
            if p.is_dir() {
                argv.extend(ro_bind_path(&p));
            }
        }
    }

    argv.push("--chdir".to_owned());
    argv.push(plan.workspace.display().to_string());
    argv.push("--".to_owned());
    argv.push(prog.clone());
    argv.extend(plan.command.iter().skip(1).cloned());
    Ok(argv)
}

/// How an optional MCP workspace is bound into the bubble.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpBindMode {
    /// Workspace is visible and not writable.
    ReadOnly,
    /// Workspace is visible and writable.
    ReadWrite,
}

/// Bubblewrap plan for one MCP connector (`devcage mcp`).
///
/// Does not bind agent state directories. Caller supplies only the paths this
/// connector is allowed to see.
#[derive(Debug, Clone)]
pub struct McpBwrapPlan {
    /// Ephemeral home (read-write). Also the working directory when no workspace is shared.
    pub ephemeral_home: PathBuf,
    /// Workspace bind when the operator opted in.
    pub workspace: Option<(PathBuf, McpBindMode)>,
    /// Extra read-only binds (pkg-guard config and cache, shim dir).
    pub ro_mounts: Vec<PathBuf>,
    /// Deny network (`--unshare-net`).
    pub net_none: bool,
    /// Guest argv (`prog` + args).
    pub command: Vec<String>,
}

/// Build `bwrap` argv for connector isolation.
///
/// # Errors
///
/// Empty guest command.
pub fn mcp_bwrap_argv(plan: &McpBwrapPlan) -> anyhow::Result<Vec<String>> {
    let Some(prog) = plan.command.first() else {
        anyhow::bail!("bwrap plan has empty command");
    };

    let mut argv = vec![
        "bwrap".to_owned(),
        "--die-with-parent".to_owned(),
        "--unshare-pid".to_owned(),
        "--unshare-uts".to_owned(),
        "--unshare-ipc".to_owned(),
        "--proc".to_owned(),
        "/proc".to_owned(),
        "--dev".to_owned(),
        "/dev".to_owned(),
        "--tmpfs".to_owned(),
        "/tmp".to_owned(),
    ];
    if plan.net_none {
        argv.push("--unshare-net".to_owned());
    }

    for p in ["/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc"] {
        argv.extend(ro_bind_if_abs(p));
    }

    argv.extend(rw_bind(&plan.ephemeral_home));
    for mount in &plan.ro_mounts {
        argv.extend(ro_bind_path(mount));
    }
    if let Some((ref workspace, mode)) = plan.workspace {
        match mode {
            McpBindMode::ReadOnly => argv.extend(ro_bind_path(workspace)),
            McpBindMode::ReadWrite => argv.extend(rw_bind(workspace)),
        }
    }

    let chdir = plan
        .workspace
        .as_ref()
        .map_or(plan.ephemeral_home.as_path(), |(workspace, _)| {
            workspace.as_path()
        });
    argv.push("--chdir".to_owned());
    argv.push(chdir.display().to_string());
    argv.push("--".to_owned());
    argv.push(prog.clone());
    argv.extend(plan.command.iter().skip(1).cloned());
    Ok(argv)
}

fn ro_bind_if_abs(p: &str) -> Vec<String> {
    if Path::new(p).is_dir() {
        ro_bind_path(Path::new(p))
    } else {
        Vec::new()
    }
}

fn ro_bind_path(p: &Path) -> Vec<String> {
    let s = p.display().to_string();
    vec!["--ro-bind".to_owned(), s.clone(), s]
}

fn rw_bind(p: &Path) -> Vec<String> {
    let s = p.display().to_string();
    vec!["--bind".to_owned(), s.clone(), s]
}

#[cfg(test)]
mod tests {
    use super::{bwrap_argv, mcp_bwrap_argv, BwrapPlan, McpBindMode, McpBwrapPlan};
    use std::path::PathBuf;

    fn base_plan(net_none: bool) -> BwrapPlan {
        BwrapPlan {
            workspace: PathBuf::from("/tmp/ws"),
            data_dir: PathBuf::from("/tmp/data"),
            ephemeral_home: Some(PathBuf::from("/tmp/eph")),
            net_none,
            command: vec!["claude".into(), "-p".into(), "hi".into()],
        }
    }

    #[test]
    fn argv_starts_with_bwrap_and_guest() {
        let argv = bwrap_argv(&base_plan(false)).expect("argv");
        assert_eq!(argv.first().map(String::as_str), Some("bwrap"));
        assert!(argv.contains(&"--die-with-parent".into()));
        let dash = argv.iter().position(|a| a == "--").expect("sep");
        assert_eq!(argv.get(dash + 1).map(String::as_str), Some("claude"));
        assert_eq!(argv.get(dash + 2).map(String::as_str), Some("-p"));
        assert!(!argv.contains(&"--unshare-net".into()));
    }

    #[test]
    fn net_none_adds_unshare_net() {
        let argv = bwrap_argv(&base_plan(true)).expect("argv");
        assert!(argv.contains(&"--unshare-net".into()));
    }

    #[test]
    fn binds_workspace_and_data_not_wholesale_home_string() {
        let argv = bwrap_argv(&base_plan(false)).expect("argv");
        assert!(argv
            .windows(3)
            .any(|w| { w[0] == "--bind" && w[1] == "/tmp/ws" && w[2] == "/tmp/ws" }));
        assert!(argv
            .windows(3)
            .any(|w| { w[0] == "--bind" && w[1] == "/tmp/data" && w[2] == "/tmp/data" }));
        // Must not `--bind $HOME $HOME` even if HOME is set.
        if let Ok(home) = std::env::var("HOME") {
            let wholesale = argv
                .windows(3)
                .any(|w| (w[0] == "--bind" || w[0] == "--ro-bind") && w[1] == home && w[2] == home);
            assert!(!wholesale, "must not bind entire HOME");
        }
    }

    #[test]
    fn empty_command_errors() {
        let mut p = base_plan(false);
        p.command.clear();
        assert!(bwrap_argv(&p).is_err());
    }

    #[test]
    fn mcp_plan_binds_ephemeral_home_only() {
        let argv = mcp_bwrap_argv(&McpBwrapPlan {
            ephemeral_home: PathBuf::from("/tmp/eph-mcp"),
            workspace: None,
            ro_mounts: vec![PathBuf::from("/tmp/pkg-guard-cache")],
            net_none: false,
            command: vec!["uvx".into(), "server==1.2.3".into()],
        })
        .expect("argv");
        assert!(argv
            .windows(3)
            .any(|w| { w[0] == "--bind" && w[1] == "/tmp/eph-mcp" && w[2] == "/tmp/eph-mcp" }));
        assert!(argv.windows(3).any(|w| {
            w[0] == "--ro-bind" && w[1] == "/tmp/pkg-guard-cache" && w[2] == "/tmp/pkg-guard-cache"
        }));
        assert!(!argv.iter().any(|a| a.contains("/tmp/ws-mcp")));
        assert!(!argv
            .iter()
            .any(|a| a.contains(".grok") || a.contains(".claude")));
        assert!(!argv
            .iter()
            .any(|a| a.contains(".cursor") || a.contains(".codex")));
        assert!(!argv.contains(&"--unshare-net".into()));
        let chdir = argv.iter().position(|a| a == "--chdir").expect("chdir");
        assert_eq!(
            argv.get(chdir + 1).map(String::as_str),
            Some("/tmp/eph-mcp")
        );
        let dash = argv.iter().position(|a| a == "--").expect("sep");
        assert_eq!(argv.get(dash + 1).map(String::as_str), Some("uvx"));
    }

    #[test]
    fn mcp_plan_workspace_and_net_none() {
        let ro = mcp_bwrap_argv(&McpBwrapPlan {
            ephemeral_home: PathBuf::from("/tmp/eph-mcp"),
            workspace: Some((PathBuf::from("/tmp/ws-mcp"), McpBindMode::ReadOnly)),
            ro_mounts: Vec::new(),
            net_none: true,
            command: vec!["true".into()],
        })
        .expect("argv");
        assert!(ro.contains(&"--unshare-net".into()));
        assert!(ro
            .windows(3)
            .any(|w| { w[0] == "--ro-bind" && w[1] == "/tmp/ws-mcp" && w[2] == "/tmp/ws-mcp" }));
        assert!(!ro
            .windows(3)
            .any(|w| { w[0] == "--bind" && w[1] == "/tmp/ws-mcp" }));
        let chdir = ro.iter().position(|a| a == "--chdir").expect("chdir");
        assert_eq!(ro.get(chdir + 1).map(String::as_str), Some("/tmp/ws-mcp"));

        let rw = mcp_bwrap_argv(&McpBwrapPlan {
            ephemeral_home: PathBuf::from("/tmp/eph-mcp"),
            workspace: Some((PathBuf::from("/tmp/ws-mcp"), McpBindMode::ReadWrite)),
            ro_mounts: Vec::new(),
            net_none: false,
            command: vec!["true".into()],
        })
        .expect("argv");
        assert!(rw
            .windows(3)
            .any(|w| { w[0] == "--bind" && w[1] == "/tmp/ws-mcp" && w[2] == "/tmp/ws-mcp" }));
    }

    #[test]
    fn mcp_empty_command_errors() {
        let err = mcp_bwrap_argv(&McpBwrapPlan {
            ephemeral_home: PathBuf::from("/tmp/eph-mcp"),
            workspace: None,
            ro_mounts: Vec::new(),
            net_none: false,
            command: Vec::new(),
        });
        assert!(err.is_err());
    }
}
