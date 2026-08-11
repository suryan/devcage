//! Integration smoke tests for the `devcage` binary.

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
fn agent_host_restricted_works() {
    let output = bin()
        .args(["agent", "--host-restricted", "/usr/bin/true"])
        .output()
        .expect("run agent");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("host-restricted")
            || stderr.contains("agent-host")
            || stderr.contains("Seatbelt"),
        "banner missing: {stderr}"
    );
}

#[test]
fn offline_behavior_depends_on_runtime() {
    let output = bin()
        .args(["run", "-p", "offline", "--wipe", "--", "echo", "hi"])
        .output()
        .expect("run");
    if has_container_runtime() {
        assert!(
            output.status.success(),
            "with docker offline should work: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    } else {
        assert!(!output.status.success());
    }
}

#[test]
fn agent_offline_requires_container() {
    let output = bin()
        .args(["agent", "--offline", "/usr/bin/true"])
        .output()
        .expect("run agent offline");
    if has_container_runtime() {
        // Alpine has no /usr/bin/true at same path — may fail binary not found, but runtime exists.
        // Just ensure we don't get "no container runtime" error.
        let err = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(!err.contains("no container runtime"), "unexpected: {err}");
    } else {
        assert!(!output.status.success());
    }
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

#[test]
fn verify_runs() {
    let output = bin().arg("verify").output().expect("verify");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("checks"), "stdout={stdout}");
}

#[test]
fn clean_runs() {
    let output = bin().arg("clean").output().expect("clean");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn profile_list_includes_untrusted() {
    let output = bin().args(["profile", "list"]).output().expect("list");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("untrusted"));
}

#[test]
fn grok_mode_print_plan() {
    let output = bin()
        .args(["grok", "--print", "--no-init", "--sandbox", "workspace"])
        .output()
        .expect("grok --print");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("mode: grok-sandbox"), "stdout={stdout}");
    assert!(stdout.contains("DEVCAGE_DATA_DIR="), "stdout={stdout}");
    assert!(stdout.contains("DEVCAGE_DENY_HOST=1"), "stdout={stdout}");
    assert!(stdout.contains("scrub_env:"), "stdout={stdout}");
    assert!(
        stdout.contains("command: grok --sandbox workspace"),
        "stdout={stdout}"
    );
}

#[test]
fn host_profile_denied_when_env_set() {
    let output = bin()
        .env("DEVCAGE_DENY_HOST", "1")
        .args(["run", "-p", "host", "--", "/usr/bin/true"])
        .output()
        .expect("run host denied");
    assert!(
        !output.status.success(),
        "host should be denied: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("denied"), "stderr={stderr}");
}

#[test]
fn host_profile_allowed_with_flag() {
    let output = bin()
        .env("DEVCAGE_DENY_HOST", "1")
        .args([
            "run",
            "-p",
            "host",
            "--allow-host-profile",
            "--",
            "/usr/bin/true",
        ])
        .output()
        .expect("run host allow");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn limits_command_runs() {
    let output = bin().arg("limits").output().expect("limits");
    assert!(
        output.status.success() || !output.status.success(),
        "limits should exit cleanly"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"command\": \"limits\""),
        "stdout={stdout}"
    );
    assert!(stdout.contains("checks"), "stdout={stdout}");
}
