//! Small pure helpers shared by the CLI (kept unit-testable).

use std::path::Path;

/// Return `true` if `bin` exists as a file on `PATH`.
#[must_use]
pub(crate) fn which(bin: &str) -> bool {
    which_in_path(bin, std::env::var_os("PATH").as_deref())
}

/// PATH-aware lookup used by [`which`] and tests.
#[must_use]
pub(crate) fn which_in_path(bin: &str, path_os: Option<&std::ffi::OsStr>) -> bool {
    let Some(paths) = path_os else {
        return false;
    };
    std::env::split_paths(paths).any(|dir| {
        let candidate = dir.join(bin);
        path_is_executable_file(&candidate)
    })
}

fn path_is_executable_file(path: &Path) -> bool {
    path.is_file()
}

/// Build a standard "not available" style message (tests + future stubs).
#[must_use]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn not_implemented_message(cmd: &str, detail: Option<&str>) -> String {
    let mut msg = format!(
        "devcage {cmd}: not implemented yet (scaffold {version}). See docs/product_requirements.md and docs/development.md.",
        version = env!("CARGO_PKG_VERSION"),
    );
    if let Some(d) = detail {
        msg.push(' ');
        msg.push_str(d);
    }
    msg
}

#[cfg(test)]
mod tests {
    use super::{not_implemented_message, which_in_path};
    use std::path::PathBuf;

    #[test]
    fn which_in_path_finds_existing_file() {
        let dir = tempfile_dir();
        let bin_name = "devcage-test-bin-which";
        let bin_path = dir.join(bin_name);
        std::fs::write(&bin_path, b"#!/bin/sh\n").expect("write bin");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bin_path).expect("meta").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bin_path, perms).expect("chmod");
        }
        assert!(which_in_path(bin_name, Some(dir.as_os_str())));
        let _ = std::fs::remove_file(&bin_path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn which_in_path_missing_returns_false() {
        assert!(!which_in_path(
            "definitely-not-a-real-devcage-binary-zzz",
            Some(std::ffi::OsStr::new("/nonexistent/devcage/path"))
        ));
        assert!(!which_in_path("anything", None));
    }

    #[test]
    fn not_implemented_message_includes_cmd_and_detail() {
        let msg = not_implemented_message("agent", Some("offline=true"));
        assert!(msg.contains("agent"));
        assert!(msg.contains("offline=true"));
        assert!(msg.contains(env!("CARGO_PKG_VERSION")));
    }

    fn tempfile_dir() -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "devcage-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }
}
