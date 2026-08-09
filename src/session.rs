//! Session metadata store (JSON files under the data directory).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::plan::BackendKind;

/// Lifecycle status of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Command is running (or was when last updated).
    Running,
    /// Command finished.
    Exited,
    /// Explicitly stopped.
    Stopped,
}

/// Persisted session record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionRecord {
    /// Session id.
    pub id: String,
    /// Profile name.
    pub profile: String,
    /// Backend name.
    pub backend: String,
    /// Workspace path on host.
    pub workspace: PathBuf,
    /// Ephemeral home on host.
    pub home: PathBuf,
    /// Container name when using container backend.
    pub container_name: Option<String>,
    /// ISO-ish started timestamp (UTC secs).
    pub started_at_unix: u64,
    /// Status.
    pub status: SessionStatus,
    /// Last process exit code if known.
    pub exit_code: Option<i32>,
}

impl SessionRecord {
    /// Create a new running session record.
    #[must_use]
    pub fn new(
        id: String,
        profile: &str,
        backend: BackendKind,
        workspace: PathBuf,
        home: PathBuf,
        container_name: Option<String>,
    ) -> Self {
        let backend = match backend {
            BackendKind::Container => "container",
            BackendKind::Host => "host",
        };
        Self {
            id,
            profile: profile.to_owned(),
            backend: backend.to_owned(),
            workspace,
            home,
            container_name,
            started_at_unix: unix_now(),
            status: SessionStatus::Running,
            exit_code: None,
        }
    }
}

/// File-backed session store.
#[derive(Debug, Clone)]
pub struct SessionStore {
    root: PathBuf,
}

impl SessionStore {
    /// Sessions live under `{data_dir}/sessions`.
    #[must_use]
    pub fn new(data_dir: &Path) -> Self {
        Self {
            root: data_dir.join("sessions"),
        }
    }

    /// Ensure the store directory exists.
    pub fn ensure(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.root)?;
        Ok(())
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.json"))
    }

    /// Session home directory path for an id.
    #[must_use]
    pub fn home_dir(&self, id: &str) -> PathBuf {
        self.root.join(id).join("home")
    }

    /// Persist a session record.
    pub fn save(&self, rec: &SessionRecord) -> anyhow::Result<()> {
        self.ensure()?;
        let path = self.path_for(&rec.id);
        let body = serde_json::to_vec_pretty(rec)?;
        fs::write(path, body)?;
        Ok(())
    }

    /// Load a session by id.
    pub fn load(&self, id: &str) -> anyhow::Result<SessionRecord> {
        let path = self.path_for(id);
        let bytes = fs::read(&path)
            .map_err(|e| anyhow::anyhow!("session `{id}` not found ({}): {e}", path.display()))?;
        let rec: SessionRecord = serde_json::from_slice(&bytes)?;
        Ok(rec)
    }

    /// List all sessions (best-effort; skips corrupt files).
    pub fn list(&self) -> anyhow::Result<Vec<SessionRecord>> {
        self.ensure()?;
        let mut out = Vec::new();
        let rd = match fs::read_dir(&self.root) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e.into()),
        };
        for ent in rd {
            let ent = ent?;
            let path = ent.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            if let Ok(bytes) = fs::read(&path) {
                if let Ok(rec) = serde_json::from_slice::<SessionRecord>(&bytes) {
                    out.push(rec);
                }
            }
        }
        out.sort_by(|a, b| b.started_at_unix.cmp(&a.started_at_unix));
        Ok(out)
    }

    /// Delete session metadata and optionally the home tree.
    pub fn wipe(&self, id: &str) -> anyhow::Result<()> {
        let meta = self.path_for(id);
        let dir = self.root.join(id);
        if meta.exists() {
            fs::remove_file(&meta)?;
        }
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }

    /// Prepare ephemeral home directory for a new session.
    pub fn prepare_home(&self, id: &str) -> anyhow::Result<PathBuf> {
        let home = self.home_dir(id);
        fs::create_dir_all(&home)?;
        Ok(home)
    }
}

/// Generate a unique session id.
#[must_use]
pub fn new_session_id() -> String {
    let t = unix_now();
    let r = random_u32();
    format!("dc-{t:x}-{r:08x}")
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn random_u32() -> u32 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);

    let mut h = DefaultHasher::new();
    std::process::id().hash(&mut h);
    unix_now().hash(&mut h);
    n.hash(&mut h);
    std::thread::current().id().hash(&mut h);
    // Truncate hash to u32 without `as` casts (clippy restriction).
    u32::try_from(h.finish() & 0xffff_ffff).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{new_session_id, SessionRecord, SessionStatus, SessionStore};
    use crate::plan::BackendKind;
    use std::path::PathBuf;

    #[test]
    fn session_id_is_unique_enough() {
        let a = new_session_id();
        let b = new_session_id();
        assert!(a.starts_with("dc-"));
        // May collide rarely; usually distinct
        assert_ne!(a, b);
    }

    #[test]
    fn store_roundtrip() {
        let root = std::env::temp_dir().join(format!(
            "devcage-sess-{}-{}",
            std::process::id(),
            new_session_id()
        ));
        let store = SessionStore::new(&root);
        let id = new_session_id();
        let home = store.prepare_home(&id).expect("home");
        let rec = SessionRecord::new(
            id.clone(),
            "offline",
            BackendKind::Container,
            PathBuf::from("/ws"),
            home,
            Some(format!("devcage-{id}")),
        );
        store.save(&rec).expect("save");
        let loaded = store.load(&id).expect("load");
        assert_eq!(loaded.profile, "offline");
        assert_eq!(loaded.status, SessionStatus::Running);
        let list = store.list().expect("list");
        assert!(list.iter().any(|s| s.id == id));
        store.wipe(&id).expect("wipe");
        assert!(store.load(&id).is_err());
        let _ = std::fs::remove_dir_all(root);
    }
}
