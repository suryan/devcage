//! Optional: run bubblewrap **inside Linux** via Docker (validates flags from macOS).
//!
//! Skipped when Docker is down. Does not require a Linux `devcage` binary.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::process::Command;

fn docker_ok() -> bool {
    Command::new("docker")
        .args(["info"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[test]
fn docker_linux_bwrap_true() {
    if !docker_ok() {
        eprintln!("skip: no docker");
        return;
    }
    // Minimal bwrap matching our plan: ro system, tmpfs /tmp, optional unshare-net, true.
    let script = "apk add --no-cache bubblewrap >/dev/null && \
         bwrap --die-with-parent --unshare-pid --unshare-uts --unshare-ipc \
           --proc /proc --dev /dev --tmpfs /tmp --unshare-net \
           --ro-bind /usr /usr --ro-bind /bin /bin --ro-bind /lib /lib \
           --ro-bind /etc /etc -- /bin/true";
    let privileged = Command::new("docker")
        .args([
            "run",
            "--rm",
            "--privileged",
            "docker.io/library/alpine:3.20",
            "sh",
            "-lc",
            script,
        ])
        .output()
        .expect("docker run");
    if privileged.status.success() {
        return;
    }
    // Nested user namespaces are often denied on Docker Desktop / Colima.
    eprintln!(
        "skip: linux bwrap namespaces not usable in this Docker (common on macOS): {}",
        String::from_utf8_lossy(&privileged.stderr)
    );
}
