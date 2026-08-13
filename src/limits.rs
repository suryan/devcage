//! Isolation limit probes for Grok sandbox + `DevCage` composition.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use serde_json::{json, Value};

use crate::backend::detect_container_runtime;
use crate::config::{host_profile_denied, Config};
use crate::util::which;

/// Run probes and print JSON; exit 0 if critical checks pass.
///
/// # Errors
///
/// Returns an error only on JSON serialization failure.
pub fn run_limits(cfg: &Config) -> anyhow::Result<ExitCode> {
    let mut checks = Vec::new();
    push_fs_checks(cfg, &mut checks);
    push_env_mode_checks(&mut checks);
    push_wrap_checks(cfg, &mut checks);
    push_container_checks(cfg, &mut checks);

    let critical_failed = checks.iter().any(|c| {
        c.get("critical").and_then(Value::as_bool).unwrap_or(false)
            && c.get("ok").and_then(Value::as_bool) == Some(false)
    });
    let report = json!({
        "product": "devcage",
        "command": "limits",
        "mode": std::env::var("DEVCAGE_MODE").ok(),
        "grok_sandbox": std::env::var("DEVCAGE_GROK_SANDBOX").ok(),
        "data_dir": cfg.data_dir,
        "data_dir_note": cfg.data_dir_note,
        "deny_host": host_profile_denied(),
        "checks": checks,
        "pass": !critical_failed,
        "notes": [
            "On macOS, Grok sandbox often cannot block host outbound HTTPS; use offline profile or wrap --net none.",
            "Keychain API may still be reachable from the host process; use mcp-secret-launcher + mcp-guard.",
            "Re-seed recipes after upgrades: devcage init-grok --force",
            "devcage wrap: Seatbelt without wholesale $HOME read; --net none denies process network.",
        ],
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(if critical_failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

fn push_fs_checks(cfg: &Config, out: &mut Vec<Value>) {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let home = std::env::var_os("HOME").map(PathBuf::from);

    out.push(check(
        "data_dir_writable",
        write_probe(&cfg.data_dir),
        true,
        format!("{}", cfg.data_dir.display()),
    ));

    if let Some(ref h) = home {
        let local = h.join(".local/share/devcage");
        let local_ok = write_probe(&local);
        out.push(check(
            "default_local_share_writable",
            local_ok,
            false,
            if local_ok {
                "writable (normal mode or broad sandbox)"
            } else {
                "blocked (expected under Grok Seatbelt)"
            },
        ));
    }

    let proj = cwd.join(format!(".devcage-limits-write-{}", std::process::id()));
    let proj_ok = fs::write(&proj, b"ok").is_ok();
    let _ = fs::remove_file(&proj);
    out.push(check(
        "project_write",
        proj_ok,
        true,
        "write under workspace CWD",
    ));

    let secret = cwd.join(".env");
    let secret_write_blocked = fs::write(&secret, b"x").is_err();
    let _ = fs::remove_file(&secret);
    out.push(check(
        "project_secret_write_blocked",
        secret_write_blocked,
        false,
        if secret_write_blocked {
            ".env write denied (kernel deny active)"
        } else {
            ".env write allowed — re-seed: devcage init-grok --force"
        },
    ));

    if let Some(ref h) = home {
        let outside = h
            .join("Desktop")
            .join(format!(".devcage-limits-{}", std::process::id()));
        let outside_blocked = fs::write(&outside, b"x").is_err();
        let _ = fs::remove_file(&outside);
        out.push(check(
            "outside_project_write_blocked",
            outside_blocked,
            false,
            if outside_blocked {
                "Desktop write blocked"
            } else {
                "Desktop write allowed — not under Grok sandbox?"
            },
        ));

        let sibling_ok = fs::read_dir(h.join("workspace")).is_ok();
        let ssh_readable = path_readable(&h.join(".ssh").join("config"));
        let limited = !sibling_ok || !ssh_readable || fs::read_dir(h.join(".ssh")).is_err();
        out.push(check(
            "home_read_limited",
            limited,
            false,
            if limited {
                "sibling home / .ssh limited (strict-like)"
            } else {
                "can read ~/workspace or ~/.ssh — re-seed keys-first extends=strict"
            },
        ));
    }
}

fn push_env_mode_checks(out: &mut Vec<Value>) {
    let mode = std::env::var("DEVCAGE_MODE").unwrap_or_default();
    let net_ok = host_https_ok();
    out.push(check(
        "host_https_probe",
        true,
        false,
        if net_ok {
            "host HTTPS reachable (macOS sandbox usually allows this)"
        } else {
            "host HTTPS not reachable"
        },
    ));

    let sock_expected = mode == "grok-sandbox" || mode == "wrap";
    let sock_scrubbed = !sock_expected || std::env::var_os("SSH_AUTH_SOCK").is_none();
    out.push(check(
        "ssh_auth_sock_scrubbed_in_isolated_mode",
        sock_scrubbed,
        false,
        if !sock_expected {
            "not in grok-sandbox/wrap mode"
        } else if sock_scrubbed {
            "SSH_AUTH_SOCK unset"
        } else {
            "SSH_AUTH_SOCK still set — restart with latest devcage grok|wrap"
        },
    ));

    let deny = host_profile_denied();
    let deny_ok = (mode != "grok-sandbox" && mode != "wrap") || deny;
    out.push(check(
        "deny_host_profile_armed",
        deny_ok,
        false,
        format!(
            "DEVCAGE_DENY_HOST={:?} denied={deny}",
            std::env::var("DEVCAGE_DENY_HOST").ok()
        ),
    ));
}

fn push_wrap_checks(_cfg: &Config, out: &mut Vec<Value>) {
    let Ok(bin) = std::env::current_exe() else {
        out.push(check(
            "wrap_print",
            false,
            false,
            "cannot resolve current exe",
        ));
        return;
    };
    let output = Command::new(&bin)
        .args(["wrap", "--print", "--", "/usr/bin/true"])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            let ok = s.contains("mode: wrap") && s.contains("home_read:");
            out.push(check(
                "wrap_print",
                ok,
                false,
                if ok {
                    "wrap --print reports mode + home_read policy"
                } else {
                    "wrap --print missing expected fields"
                },
            ));
            let net_none = Command::new(&bin)
                .args(["wrap", "--print", "--net", "none", "--", "/usr/bin/true"])
                .output();
            let net_ok = net_none
                .ok()
                .filter(|o| o.status.success())
                .is_some_and(|o| String::from_utf8_lossy(&o.stdout).contains("net: none"));
            out.push(check(
                "wrap_net_none_print",
                net_ok,
                false,
                if net_ok {
                    "wrap --net none advertised"
                } else {
                    "wrap --net none print failed"
                },
            ));
        },
        Ok(o) => {
            out.push(check(
                "wrap_print",
                false,
                false,
                format!(
                    "wrap --print failed: {}",
                    String::from_utf8_lossy(&o.stderr)
                ),
            ));
        },
        Err(e) => {
            out.push(check(
                "wrap_print",
                false,
                false,
                format!("wrap --print spawn error: {e}"),
            ));
        },
    }
}

fn push_container_checks(cfg: &Config, out: &mut Vec<Value>) {
    match detect_container_runtime(cfg.container_runtime.as_deref()) {
        Ok(rt) => {
            out.push(check("container_runtime", true, true, rt.bin()));
            let offline_ok = offline_echo_ok(cfg);
            out.push(check(
                "offline_container_echo",
                offline_ok,
                true,
                if offline_ok {
                    "devcage run -p offline works"
                } else {
                    "offline run failed"
                },
            ));
            let offline_net_blocked = offline_net_blocked(cfg);
            out.push(check(
                "offline_network_blocked",
                offline_net_blocked,
                true,
                if offline_net_blocked {
                    "container has no outbound net"
                } else {
                    "offline container reached network (unexpected)"
                },
            ));
            if host_profile_denied() {
                let host_denied = host_profile_refused(cfg);
                out.push(check(
                    "host_profile_refused_when_denied",
                    host_denied,
                    false,
                    if host_denied {
                        "run -p host refused as expected"
                    } else {
                        "run -p host was allowed despite DEVCAGE_DENY_HOST"
                    },
                ));
            }
        },
        Err(_) => {
            out.push(check(
                "container_runtime",
                false,
                false,
                "docker/podman not available",
            ));
        },
    }
}

fn check(name: &str, ok: bool, critical: bool, detail: impl AsRef<str>) -> Value {
    json!({
        "name": name,
        "ok": ok,
        "critical": critical,
        "detail": detail.as_ref(),
    })
}

fn write_probe(dir: &Path) -> bool {
    if fs::create_dir_all(dir).is_err() {
        return false;
    }
    let p = dir.join(format!(".limits-probe-{}", std::process::id()));
    match fs::write(&p, b"ok") {
        Ok(()) => {
            let _ = fs::remove_file(&p);
            true
        },
        Err(_) => false,
    }
}

fn path_readable(path: &Path) -> bool {
    fs::File::open(path).is_ok()
}

fn host_https_ok() -> bool {
    if !which("curl") {
        return false;
    }
    Command::new("curl")
        .args(["-sS", "-m", "3", "-o", "/dev/null", "https://example.com"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn offline_echo_ok(cfg: &Config) -> bool {
    let Ok(bin) = std::env::current_exe() else {
        return false;
    };
    Command::new(bin)
        .args(["run", "-p", "offline", "--", "echo", "limits-ok"])
        .env("DEVCAGE_DATA_DIR", &cfg.data_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn offline_net_blocked(cfg: &Config) -> bool {
    let Ok(bin) = std::env::current_exe() else {
        return false;
    };
    let output = Command::new(bin)
        .args([
            "run",
            "-p",
            "offline",
            "--",
            "sh",
            "-c",
            "wget -q -O- -T 2 https://example.com 2>/dev/null && echo NET_OK || echo NET_BLOCKED",
        ])
        .env("DEVCAGE_DATA_DIR", &cfg.data_dir)
        .output();
    match output {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout);
            s.contains("NET_BLOCKED") && !s.contains("NET_OK")
        },
        Err(_) => false,
    }
}

fn host_profile_refused(cfg: &Config) -> bool {
    let Ok(bin) = std::env::current_exe() else {
        return false;
    };
    let output = Command::new(bin)
        .args(["run", "-p", "host", "--", "/usr/bin/true"])
        .env("DEVCAGE_DATA_DIR", &cfg.data_dir)
        .env("DEVCAGE_DENY_HOST", "1")
        .output();
    match output {
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            !o.status.success() && err.contains("denied")
        },
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::write_probe;

    #[test]
    fn write_probe_tmp_ok() {
        let dir = std::env::temp_dir().join(format!(
            "devcage-limits-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        assert!(write_probe(&dir));
        let _ = std::fs::remove_dir_all(dir);
    }
}
