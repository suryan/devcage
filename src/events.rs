//! Append-only session event log (JSON lines).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// One structured event for a session.
#[derive(Debug, Serialize)]
pub struct SessionEvent {
    /// Unix seconds.
    pub ts: u64,
    /// Event type.
    pub kind: &'static str,
    /// Optional message.
    pub message: String,
    /// Optional extra JSON-friendly detail.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Path to the event log for a session.
#[must_use]
pub fn events_path(data_dir: &Path, session_id: &str) -> PathBuf {
    data_dir
        .join("sessions")
        .join(session_id)
        .join("events.jsonl")
}

/// Append one event line.
pub fn append_event(
    data_dir: &Path,
    session_id: &str,
    kind: &'static str,
    message: impl Into<String>,
    detail: Option<String>,
) -> anyhow::Result<()> {
    let path = events_path(data_dir, session_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let ev = SessionEvent {
        ts: unix_now(),
        kind,
        message: message.into(),
        detail,
    };
    let line = serde_json::to_string(&ev)?;
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{line}")?;
    Ok(())
}

/// Read event log text (empty if missing).
pub fn read_events(data_dir: &Path, session_id: &str) -> anyhow::Result<String> {
    let path = events_path(data_dir, session_id);
    if !path.exists() {
        return Ok(String::new());
    }
    Ok(std::fs::read_to_string(path)?)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Ensure file handle type is used (quiet unused import if `OpenOptions` path changes).
#[allow(dead_code)]
fn _file_ty(_: File) {}

#[cfg(test)]
mod tests {
    use super::{append_event, read_events};
    use crate::session::new_session_id;

    #[test]
    fn append_and_read() {
        let dir = std::env::temp_dir().join(format!("devcage-ev-{}", new_session_id()));
        let id = new_session_id();
        append_event(&dir, &id, "start", "hello", None).expect("append");
        append_event(&dir, &id, "end", "bye", Some("code=0".to_owned())).expect("append2");
        let text = read_events(&dir, &id).expect("read");
        assert!(text.contains("hello"));
        assert!(text.contains("bye"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
