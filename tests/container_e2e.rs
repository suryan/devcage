//! Container e2e tests — skipped when Docker/Podman is unavailable.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_devcage"))
}

fn has_container_runtime() -> bool {
    Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
        || Command::new("podman")
            .arg("info")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
}

#[test]
fn offline_blocks_network() {
    if !has_container_runtime() {
        eprintln!("skip: no docker/podman");
        return;
    }
    let output = bin()
        .args([
            "run",
            "-p",
            "offline",
            "--wipe",
            "--",
            "wget",
            "-q",
            "-T",
            "2",
            "-O-",
            "http://1.1.1.1",
        ])
        .output()
        .expect("run");
    // wget fails (network unreachable) → non-zero, or our shell prints error
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // Session should start; network should fail for wget
    assert!(
        !output.status.success()
            || combined.contains("Network unreachable")
            || combined.contains("bad address")
            || combined.contains("wget:"),
        "expected network failure, got success with: {combined}"
    );
}

#[test]
fn offline_echo_succeeds() {
    if !has_container_runtime() {
        return;
    }
    let output = bin()
        .args(["run", "-p", "offline", "--wipe", "--", "echo", "cage-ok"])
        .output()
        .expect("run");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("cage-ok"));
}

#[test]
fn doctor_ready_when_docker() {
    if !has_container_runtime() {
        return;
    }
    let output = bin().arg("doctor").output().expect("doctor");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"readiness\": \"ready\"") || stdout.contains("ready"));
}
