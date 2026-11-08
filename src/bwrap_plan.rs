//! Linux `bwrap` argument plans for [`crate::wrap_mode`] and [`crate::mcp_mode`].
//!
//! Pure argv construction so it can be **unit-tested on macOS**. Execution only
//! happens on Linux when `bwrap` is on `PATH`.
//!
//! `/etc` is bind-mounted read-only. On systemd-resolved hosts `/etc/resolv.conf`
//! is a symlink onto `/run` (for example `stub-resolv.conf`). The plan bind-mounts
//! that resolved file read-only and recreates intermediate directories or symlinks
//! so the guest can follow the link. It does not mount all of `/run`.

use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path, PathBuf};

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

    // Host system (read-only), plus the resolved `/etc/resolv.conf` target when
    // that symlink points outside `/etc`.
    append_system_binds(&mut argv);

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

    append_system_binds(&mut argv);

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

/// System directories mirrored read-only into the cage.
const SYSTEM_RO_PREFIXES: &[&str] = &["/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc"];

/// Stop walking a symlink loop. Host `resolv.conf` chains are a handful of links.
const MAX_RESOLV_SYMLINKS: usize = 40;

/// One step while recreating a path the guest must walk to open `resolv.conf`.
#[derive(Debug)]
enum ComponentAction {
    /// This prefix is already inside a system bind.
    Skip,
    /// Create an empty directory.
    Dir(String),
    /// Bind-mount this regular file read-only onto the same path.
    File(String),
    /// Recreate a symlink with its original link text.
    Symlink { path: String, link: String },
}

fn append_system_binds(argv: &mut Vec<String>) {
    let visible = visible_system_prefixes();
    for prefix in &visible {
        argv.extend(ro_bind_path(prefix));
    }
    argv.extend(resolv_conf_bwrap_args(
        Path::new("/etc/resolv.conf"),
        &visible,
    ));
}

fn visible_system_prefixes() -> Vec<PathBuf> {
    SYSTEM_RO_PREFIXES
        .iter()
        .filter(|prefix| Path::new(prefix).is_dir())
        .map(PathBuf::from)
        .collect()
}

/// Args that publish the file `/etc/resolv.conf` resolves to, when that file sits
/// outside the read-only system binds.
///
/// A missing or unreadable `resolv.conf` yields no extra mounts. Name lookup may
/// fail in that case; the cage still starts.
fn resolv_conf_bwrap_args(resolv: &Path, visible: &[PathBuf]) -> Vec<String> {
    follow_resolv(resolv, visible, 0).unwrap_or_default()
}

fn follow_resolv(path: &Path, visible: &[PathBuf], depth: usize) -> Option<Vec<String>> {
    if depth > MAX_RESOLV_SYMLINKS {
        return None;
    }
    let meta = fs::symlink_metadata(path).ok()?;
    if meta.file_type().is_symlink() {
        return follow_symlink(path, visible, depth);
    }
    if is_visible(path, visible) || !meta.file_type().is_file() {
        return Some(Vec::new());
    }
    let parts = normal_parts(path)?;
    materialize(&parts, visible, depth)
}

fn follow_symlink(path: &Path, visible: &[PathBuf], depth: usize) -> Option<Vec<String>> {
    let link = fs::read_link(path).ok()?;
    if is_visible(path, visible) {
        let next = join_symlink(path, &link)?;
        return follow_resolv(&next, visible, depth + 1);
    }
    let parts = normal_parts(path)?;
    materialize(&parts, visible, depth)
}

/// Recreate `parts` inside the cage: directories, symlinks, then a read-only file.
fn materialize(parts: &[OsString], visible: &[PathBuf], depth: usize) -> Option<Vec<String>> {
    if depth > MAX_RESOLV_SYMLINKS {
        return None;
    }
    if is_visible(&path_from_parts(parts), visible) {
        return Some(Vec::new());
    }
    let mut args = Vec::new();
    let mut acc = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        acc.push(part.clone());
        let last = index + 1 == parts.len();
        match component_action(&acc, visible, last)? {
            ComponentAction::Skip => {},
            ComponentAction::Dir(path) => {
                args.push("--dir".to_owned());
                args.push(path);
            },
            ComponentAction::File(path) => {
                args.extend(ro_bind_path(Path::new(&path)));
            },
            ComponentAction::Symlink { path, link } => {
                args.push("--symlink".to_owned());
                args.push(link.clone());
                args.push(path.clone());
                let rest = parts.get(index + 1..)?;
                let next = symlink_remainder(Path::new(&path), Path::new(&link), rest)?;
                args.extend(materialize(&next, visible, depth + 1)?);
                return Some(args);
            },
        }
    }
    Some(args)
}

fn component_action(
    acc: &[OsString],
    visible: &[PathBuf],
    is_last: bool,
) -> Option<ComponentAction> {
    let path = path_from_parts(acc);
    if is_visible(&path, visible) {
        return Some(ComponentAction::Skip);
    }
    let meta = fs::symlink_metadata(&path).ok()?;
    let rendered = argv_path(&path)?;
    let kind = meta.file_type();
    if kind.is_symlink() {
        let link = argv_path(&fs::read_link(&path).ok()?)?;
        return Some(ComponentAction::Symlink {
            path: rendered,
            link,
        });
    }
    if kind.is_dir() {
        return Some(ComponentAction::Dir(rendered));
    }
    if kind.is_file() && is_last {
        return Some(ComponentAction::File(rendered));
    }
    None
}

fn symlink_remainder(
    link_path: &Path,
    link_text: &Path,
    rest: &[OsString],
) -> Option<Vec<OsString>> {
    let mut next = normal_parts(&join_symlink(link_path, link_text)?)?;
    next.extend(rest.iter().cloned());
    Some(next)
}

fn join_symlink(link_path: &Path, link_text: &Path) -> Option<PathBuf> {
    let joined = if link_text.is_absolute() {
        link_text.to_path_buf()
    } else {
        link_path.parent()?.join(link_text)
    };
    lexical_normalize(&joined)
}

fn lexical_normalize(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let mut parts = Vec::new();
    for comp in path.components() {
        match comp {
            Component::RootDir | Component::CurDir => {},
            Component::ParentDir => {
                parts.pop()?;
            },
            Component::Normal(part) => parts.push(part.to_os_string()),
            Component::Prefix(_) => return None,
        }
    }
    Some(path_from_parts(&parts))
}

fn normal_parts(path: &Path) -> Option<Vec<OsString>> {
    let normalized = lexical_normalize(path)?;
    let mut parts = Vec::new();
    for comp in normalized.components() {
        match comp {
            Component::RootDir => {},
            Component::Normal(part) => parts.push(part.to_os_string()),
            Component::CurDir | Component::ParentDir | Component::Prefix(_) => return None,
        }
    }
    Some(parts)
}

fn path_from_parts(parts: &[OsString]) -> PathBuf {
    let mut path = PathBuf::from("/");
    for part in parts {
        path.push(part);
    }
    path
}

fn is_visible(path: &Path, visible: &[PathBuf]) -> bool {
    visible.iter().any(|prefix| path.starts_with(prefix))
}

fn argv_path(path: &Path) -> Option<String> {
    path.as_os_str().to_str().map(str::to_owned)
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
    use super::{
        bwrap_argv, is_visible, mcp_bwrap_argv, resolv_conf_bwrap_args, visible_system_prefixes,
        BwrapPlan, McpBindMode, McpBwrapPlan,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    #[cfg(unix)]
    use std::os::unix::fs::symlink;

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

    #[test]
    fn resolv_args_empty_when_missing_regular_or_inside_etc() {
        let missing = Path::new("/tmp/devcage-resolv-does-not-exist");
        assert!(resolv_conf_bwrap_args(missing, &[]).is_empty());

        let root = temp_root("plain");
        let etc = root.join("etc");
        fs::create_dir_all(&etc).expect("etc");
        let resolv = etc.join("resolv.conf");
        fs::write(&resolv, "nameserver 1.1.1.1\n").expect("write");
        assert!(resolv_conf_bwrap_args(&resolv, std::slice::from_ref(&etc)).is_empty());

        #[cfg(unix)]
        {
            let real = etc.join("resolv.conf.real");
            fs::write(&real, "nameserver 9.9.9.9\n").expect("real");
            let linked = etc.join("resolv.link");
            symlink("resolv.conf.real", &linked).expect("symlink");
            assert!(resolv_conf_bwrap_args(&linked, std::slice::from_ref(&etc)).is_empty());
            let dotted = etc.join("resolv.dot");
            symlink("./resolv.conf.real", &dotted).expect("dot symlink");
            assert!(resolv_conf_bwrap_args(&dotted, &[etc]).is_empty());
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn resolv_relative_stub_is_ro_bound_not_whole_run() {
        let root = temp_root("stub");
        let etc = root.join("etc");
        let stub = root.join("run/systemd/resolve/stub-resolv.conf");
        fs::create_dir_all(stub.parent().expect("parent")).expect("stub dir");
        fs::create_dir_all(&etc).expect("etc");
        fs::write(&stub, "nameserver 127.0.0.53\n").expect("stub");
        let resolv = etc.join("resolv.conf");
        symlink("../run/systemd/resolve/stub-resolv.conf", &resolv).expect("link");

        let args = resolv_conf_bwrap_args(&resolv, &[etc]);
        let stub_s = stub.display().to_string();
        let run_s = root.join("run").display().to_string();
        assert!(
            args.windows(3)
                .any(|w| w[0] == "--ro-bind" && w[1] == stub_s && w[2] == stub_s),
            "{args:?}"
        );
        assert!(
            args.windows(2).any(|w| w[0] == "--dir" && w[1] == run_s),
            "{args:?}"
        );
        assert!(
            !args
                .windows(3)
                .any(|w| w[0] == "--ro-bind" && w[1] == run_s),
            "must not bind all of {run_s}: {args:?}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn resolv_var_run_symlink_chain_is_recreated() {
        let root = temp_root("var-run");
        let etc = root.join("etc");
        let real_run = root.join("run");
        let stub = real_run.join("systemd/resolve/stub-resolv.conf");
        fs::create_dir_all(stub.parent().expect("parent")).expect("stub dir");
        fs::create_dir_all(&etc).expect("etc");
        fs::create_dir_all(root.join("var")).expect("var");
        fs::write(&stub, "nameserver 127.0.0.53\n").expect("stub");
        let var_run = root.join("var/run");
        symlink(real_run.as_os_str(), &var_run).expect("var/run");
        let resolv = etc.join("resolv.conf");
        symlink(var_run.join("systemd/resolve/stub-resolv.conf"), &resolv).expect("resolv");

        let args = resolv_conf_bwrap_args(&resolv, &[etc]);
        let var_run_s = var_run.display().to_string();
        let real_run_s = real_run.display().to_string();
        let stub_s = stub.display().to_string();
        assert!(
            args.windows(3)
                .any(|w| w[0] == "--symlink" && w[1] == real_run_s && w[2] == var_run_s),
            "{args:?}"
        );
        assert!(
            args.windows(3)
                .any(|w| w[0] == "--ro-bind" && w[1] == stub_s && w[2] == stub_s),
            "{args:?}"
        );
        assert!(!args
            .windows(3)
            .any(|w| w[0] == "--ro-bind" && w[1] == real_run_s));
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn resolv_symlink_loop_and_broken_chain_add_no_unbounded_mounts() {
        let root = temp_root("loop");
        let etc = root.join("etc");
        let run = root.join("run");
        fs::create_dir_all(&etc).expect("etc");
        fs::create_dir_all(&run).expect("run");
        let resolv = etc.join("resolv.conf");
        let run_resolv = run.join("resolv.conf");
        symlink("../run/resolv.conf", &resolv).expect("resolv");
        symlink("../etc/resolv.conf", &run_resolv).expect("run resolv");
        let args = resolv_conf_bwrap_args(&resolv, std::slice::from_ref(&etc));
        assert!(args.len() < 20, "{args:?}");
        assert!(args.windows(2).any(|w| w[0] == "--symlink"));

        let broken = root.join("broken");
        fs::create_dir_all(&broken).expect("broken");
        let not_dir = broken.join("not-a-dir");
        fs::write(&not_dir, "x").expect("file");
        let bad = etc.join("bad.conf");
        symlink(not_dir.join("stub-resolv.conf"), &bad).expect("bad link");
        assert!(resolv_conf_bwrap_args(&bad, &[etc]).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn resolv_symlink_cycle_stops_at_the_depth_guard() {
        let root = temp_root("cycle");
        let etc = root.join("etc");
        fs::create_dir_all(&etc).expect("etc");
        let count = super::MAX_RESOLV_SYMLINKS + 5;
        for index in 0..count {
            let next = if index + 1 == count {
                "r0".to_owned()
            } else {
                format!("r{}", index + 1)
            };
            symlink(&next, etc.join(format!("r{index}"))).expect("cycle link");
        }
        assert!(resolv_conf_bwrap_args(&etc.join("r0"), std::slice::from_ref(&etc)).is_empty());

        let outside = root.join("out");
        fs::create_dir_all(&outside).expect("out");
        for index in 0..count {
            let next = if index + 1 == count {
                "s0".to_owned()
            } else {
                format!("s{}", index + 1)
            };
            symlink(&next, outside.join(format!("s{index}"))).expect("outside cycle");
        }
        assert!(resolv_conf_bwrap_args(&outside.join("s0"), &[]).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn resolv_file_component_in_the_middle_is_rejected() {
        let root = temp_root("mid-file");
        let file = root.join("not-a-directory");
        fs::write(&file, b"x").expect("file");
        let parts = super::normal_parts(&file).expect("parts");
        assert!(super::component_action(&parts, &[], false).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolv_directory_and_parent_escape_add_nothing() {
        let root = temp_root("dir");
        fs::create_dir_all(&root).expect("root");
        assert!(resolv_conf_bwrap_args(&root, &[]).is_empty());
        #[cfg(unix)]
        {
            let etc = root.join("etc");
            fs::create_dir_all(&etc).expect("etc");
            let resolv = etc.join("resolv.conf");
            symlink("../..", &resolv).expect("escape");
            assert!(resolv_conf_bwrap_args(&resolv, &[etc]).is_empty());
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn wrap_and_mcp_plans_include_host_resolv_mounts() {
        let visible = visible_system_prefixes();
        let resolv = Path::new("/etc/resolv.conf");
        let extra = resolv_conf_bwrap_args(resolv, &visible);
        let wrap = bwrap_argv(&base_plan(true)).expect("wrap");
        let mcp = mcp_bwrap_argv(&McpBwrapPlan {
            ephemeral_home: PathBuf::from("/tmp/eph-mcp"),
            workspace: None,
            ro_mounts: Vec::new(),
            net_none: false,
            command: vec!["true".into()],
        })
        .expect("mcp");
        assert!(
            argv_has_sequence(&wrap, &extra),
            "wrap missing {extra:?}\n{wrap:?}"
        );
        assert!(
            argv_has_sequence(&mcp, &extra),
            "mcp missing {extra:?}\n{mcp:?}"
        );
        let Ok(meta) = fs::symlink_metadata(resolv) else {
            return;
        };
        if !meta.file_type().is_symlink() {
            return;
        }
        let Ok(canon) = fs::canonicalize(resolv) else {
            return;
        };
        if is_visible(&canon, &visible) {
            return;
        }
        let canon_s = canon.display().to_string();
        assert!(
            extra
                .windows(3)
                .any(|w| w[0] == "--ro-bind" && w[1] == canon_s && w[2] == canon_s),
            "missing {canon_s} in {extra:?}"
        );
        if let Some(parent) = canon.parent() {
            let parent_s = parent.display().to_string();
            assert!(
                !extra
                    .windows(3)
                    .any(|w| w[0] == "--ro-bind" && w[1] == parent_s),
                "must not ro-bind {parent_s}"
            );
        }
        let dash = wrap.iter().position(|arg| arg == "--").expect("sep");
        let at = wrap
            .windows(extra.len())
            .position(|window| window == extra.as_slice())
            .expect("sequence");
        assert!(at < dash);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_bwrap_reads_host_resolv_conf() {
        if !crate::util::which("bwrap") {
            return;
        }
        let resolv = Path::new("/etc/resolv.conf");
        if !resolv.exists() {
            return;
        }
        let Some(cat) = cat_bin() else {
            return;
        };
        let expected = fs::read(resolv).expect("read resolv");
        let data = temp_root("bwrap-data");
        let argv = bwrap_argv(&BwrapPlan {
            workspace: std::env::temp_dir(),
            data_dir: data.clone(),
            ephemeral_home: None,
            net_none: false,
            command: vec![cat.to_owned(), "/etc/resolv.conf".into()],
        })
        .expect("argv");
        let output = run_bwrap(&argv);
        let _ = fs::remove_dir_all(&data);
        if skip_namespace(&output) {
            return;
        }
        assert!(
            output.status.success(),
            "bwrap failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, expected);
    }

    #[cfg(all(unix, target_os = "linux"))]
    #[test]
    fn linux_bwrap_follows_var_run_resolv_symlink() {
        if !crate::util::which("bwrap") {
            return;
        }
        let Some(cat) = cat_bin() else {
            return;
        };
        let root = temp_root("bwrap-var");
        let etc = root.join("etc");
        let real_run = root.join("run");
        let stub = real_run.join("stub-resolv.conf");
        fs::create_dir_all(&etc).expect("etc");
        fs::create_dir_all(&real_run).expect("run");
        fs::create_dir_all(root.join("var")).expect("var");
        let body = b"nameserver 127.0.0.53\n";
        fs::write(&stub, body).expect("stub");
        let var_run = root.join("var/run");
        symlink(&real_run, &var_run).expect("var/run");
        let resolv = etc.join("resolv.conf");
        symlink(var_run.join("stub-resolv.conf"), &resolv).expect("resolv");
        let extra = resolv_conf_bwrap_args(&resolv, std::slice::from_ref(&etc));
        let output = run_fixture_cat(&etc, &resolv, cat, &extra);
        let _ = fs::remove_dir_all(&root);
        if skip_namespace(&output) {
            return;
        }
        assert!(
            output.status.success(),
            "bwrap failed: {}\nargs={extra:?}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, body);
    }

    fn argv_has_sequence(argv: &[String], needle: &[String]) -> bool {
        if needle.is_empty() {
            return true;
        }
        argv.windows(needle.len()).any(|window| window == needle)
    }

    fn temp_root(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "devcage-resolv-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    #[cfg(target_os = "linux")]
    fn cat_bin() -> Option<&'static str> {
        if Path::new("/usr/bin/cat").is_file() {
            Some("/usr/bin/cat")
        } else if Path::new("/bin/cat").is_file() {
            Some("/bin/cat")
        } else {
            None
        }
    }

    #[cfg(target_os = "linux")]
    fn run_bwrap(argv: &[String]) -> std::process::Output {
        let (prog, rest) = argv.split_first().expect("bwrap argv");
        Command::new(prog).args(rest).output().expect("spawn bwrap")
    }

    #[cfg(target_os = "linux")]
    fn skip_namespace(output: &std::process::Output) -> bool {
        if output.status.success() {
            return false;
        }
        let err = String::from_utf8_lossy(&output.stderr);
        let blocked = err.contains("namespace") || err.contains("uid map");
        if blocked {
            eprintln!("skip: {err}");
        }
        blocked
    }

    #[cfg(target_os = "linux")]
    fn run_fixture_cat(
        etc: &Path,
        resolv: &Path,
        cat: &str,
        extra: &[String],
    ) -> std::process::Output {
        let etc_s = etc.display().to_string();
        let resolv_s = resolv.display().to_string();
        let mut cmd = Command::new("bwrap");
        cmd.args([
            "--die-with-parent",
            "--ro-bind",
            "/usr",
            "/usr",
            "--ro-bind",
            "/lib",
            "/lib",
            "--ro-bind",
            "/lib64",
            "/lib64",
            "--ro-bind",
            &etc_s,
            &etc_s,
        ]);
        cmd.args(extra);
        cmd.args(["--", cat, &resolv_s]);
        cmd.output().expect("spawn bwrap")
    }
}
