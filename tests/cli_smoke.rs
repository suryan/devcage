//! Integration smoke tests for the `devcage` binary.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_devcage"))
}

#[test]
fn doctor_succeeds_and_prints_json() {
    let output = bin().arg("doctor").output().expect("run doctor");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("devcage"));
    assert!(stdout.contains("mvp") || stdout.contains("container"));
    assert!(stdout.contains("agent"));
}

#[test]
fn profile_list_succeeds() {
    let output = bin().arg("profile").arg("list").output().expect("run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("agent"));
    assert!(stdout.contains("offline"));
    assert!(stdout.contains("host"));
    assert!(stdout.contains("\"implemented\": true") || stdout.contains("implemented"));
}

#[test]
fn agent_without_runtime_fails_closed() {
    let output = bin().arg("agent").output().expect("run agent");
    assert!(!output.status.success());
    let err = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        err.contains("container")
            || err.contains("docker")
            || err.contains("podman")
            || err.contains("runtime"),
        "unexpected output: {err}"
    );
}

#[test]
fn offline_without_runtime_fails_closed() {
    let output = bin()
        .args(["run", "-p", "offline", "--", "echo", "hi"])
        .output()
        .expect("run");
    assert!(!output.status.success());
}

#[test]
fn host_profile_runs_true() {
    let output = bin()
        .args(["run", "-p", "host", "--", "/usr/bin/true"])
        .output()
        .expect("run host");
    assert!(
        output.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("break-glass") || stderr.contains("host"),
        "banner missing: {stderr}"
    );
}

#[test]
fn status_succeeds() {
    let output = bin().arg("status").output().expect("run status");
    assert!(output.status.success());
}
