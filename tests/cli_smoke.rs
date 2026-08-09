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
    assert!(stdout.contains("scaffold"));
}

#[test]
fn profile_list_succeeds() {
    let output = bin().arg("profile").arg("list").output().expect("run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("agent"));
    assert!(stdout.contains("offline"));
    assert!(stdout.contains("host"));
}

#[test]
fn agent_not_implemented_fails() {
    let output = bin().arg("agent").output().expect("run agent");
    assert!(!output.status.success());
    let err = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        err.contains("not implemented") || err.contains("agent"),
        "unexpected output: {err}"
    );
}

#[test]
fn status_succeeds() {
    let output = bin().arg("status").output().expect("run status");
    assert!(output.status.success());
}
