//! Autosave and crash recovery. While the open board has unsaved changes, a copy is written every few minutes to
//! `<data dir>/autosave/<id>.brd` with a small `<id>.json` sidecar (original path, time, app version). The user's own
//! file is never written. The files go away when the board is saved, closed or replaced, or the app quits normally.
//!
//! Each running app keeps `session-<session>.lock` open with an OS lock on it, which the system releases when the
//! process ends, crash included. At startup, autosaves whose session's lock is free or gone (the app crashed or was
//! killed) are offered for recovery.

use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use surfcraft_core::Board;

use crate::document::Document;

/// The lock file's time is refreshed this often: on file systems without locks, a lock file not refreshed for
/// [`STALE`] belongs to an app that is no longer running.
pub const HEARTBEAT: Duration = Duration::from_secs(30);
pub const STALE: Duration = Duration::from_secs(120);
const UNTITLED: &str = "untitled";

pub fn autosave_dir(base: &Path) -> PathBuf {
    base.join("autosave")
}

fn lock_path(dir: &Path, session: &str) -> PathBuf {
    dir.join(format!("session-{session}.lock"))
}

/// Sidecar of an autosaved board.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    /// Session (running app) that wrote it.
    pub session: String,
    /// The board's own file, or `"untitled"`.
    pub original_path: String,
    pub name: String,
    /// Unix seconds.
    pub saved_at: i64,
    pub app_version: String,
}

impl Meta {
    pub fn original(&self) -> Option<PathBuf> {
        (self.original_path != UNTITLED).then(|| PathBuf::from(&self.original_path))
    }
}

/// An autosave found at startup.
#[derive(Clone, Debug)]
pub struct Entry {
    pub id: String,
    pub meta: Meta,
    /// `None` when the file cannot be read back.
    pub board: Option<Board>,
}

/// Write one autosave (`.brd` and sidecar), each through a temporary file renamed into place.
pub fn write_entry(
    dir: &Path,
    id: &str,
    session: &str,
    board: &Board,
    original: Option<&Path>,
) -> std::io::Result<u64> {
    std::fs::create_dir_all(dir)?;
    let text = surfcraft_io::brd::write(board);
    let meta = Meta {
        session: session.to_owned(),
        original_path: original.map_or(UNTITLED.into(), |p| p.display().to_string()),
        name: if board.info.name.is_empty() {
            original
                .and_then(Path::file_stem)
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            board.info.name.clone()
        },
        saved_at: crate::logging::now().0,
        app_version: env!("CARGO_PKG_VERSION").into(),
    };
    let json = serde_json::to_string_pretty(&meta).map_err(std::io::Error::other)?;
    for (ext, bytes) in [("brd", text.as_bytes()), ("json", json.as_bytes())] {
        let tmp = dir.join(format!("{id}.{ext}.tmp"));
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, dir.join(format!("{id}.{ext}")))?;
    }
    Ok(text.len() as u64)
}

/// Delete one autosave's files.
pub fn remove_entry(dir: &Path, id: &str) {
    for ext in ["brd", "json"] {
        let _ = std::fs::remove_file(dir.join(format!("{id}.{ext}")));
    }
}

/// Whether the app that wrote `session`'s autosaves is still running.
fn session_alive(dir: &Path, session: &str) -> bool {
    let path = lock_path(dir, session);
    let Ok(file) = File::options().read(true).write(true).open(&path) else {
        return false;
    };
    match file.try_lock() {
        Ok(()) => false,
        Err(TryLockError::WouldBlock) => true,
        // No locking on this file system: fall back to the heartbeat.
        Err(TryLockError::Error(_)) => std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age < STALE),
    }
}

/// Autosaves left by apps that are no longer running, newest first.
pub fn scan(dir: &Path) -> Vec<Entry> {
    let mut found: Vec<Entry> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let id = name.strip_suffix(".json")?.to_owned();
            let meta: Meta = serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok()?;
            if session_alive(dir, &meta.session) {
                return None;
            }
            let bytes = std::fs::read(dir.join(format!("{id}.brd"))).ok()?;
            let board = surfcraft_io::brd::read(&bytes).ok();
            Some(Entry { id, meta, board })
        })
        .collect();
    found.sort_by_key(|e| std::cmp::Reverse(e.meta.saved_at));
    found
}

/// Autosave state of the running app.
pub struct Autosave {
    pub dir: PathBuf,
    session: String,
    serial: u32,
    /// Id of the open document's autosave.
    id: String,
    /// Its files exist.
    written: bool,
    written_revision: Option<u64>,
    /// Last write, or when the document was opened.
    last_write: Instant,
    last_heartbeat: Instant,
    /// The session's lock file, held open (and locked) while the app runs.
    lock: Option<File>,
}

impl Autosave {
    /// Start the session in `dir`: hold a lock, and remove the locks of sessions that ended.
    pub fn start(dir: PathBuf, now: Instant) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if let Some(session) = name
                .strip_prefix("session-")
                .and_then(|s| s.strip_suffix(".lock"))
                && !session_alive(&dir, session)
            {
                let _ = std::fs::remove_file(e.path());
            }
        }
        let session = format!("{}-{}", std::process::id(), crate::logging::now().0);
        let lock = File::create(lock_path(&dir, &session)).ok().inspect(|f| {
            if let Err(e) = f.try_lock() {
                log::warn!("autosave lock not available, using its time instead: {e}");
            }
        });
        Self {
            id: format!("{session}-0"),
            dir,
            session,
            serial: 0,
            written: false,
            written_revision: None,
            last_write: now,
            last_heartbeat: now,
            lock,
        }
    }

    #[cfg(test)]
    pub fn id(&self) -> &str {
        &self.id
    }

    fn heartbeat(&mut self, now: Instant) {
        if now - self.last_heartbeat >= HEARTBEAT {
            if let Some(f) = &self.lock {
                let _ = f.set_modified(SystemTime::now());
            }
            self.last_heartbeat = now;
        }
    }

    /// Call every frame. Writes the document when it has unsaved changes and the interval has passed; removes the
    /// autosave once nothing is unsaved. Returns how long until a write is due, to wake the app then.
    pub fn tick(&mut self, doc: &Document, minutes: u32, now: Instant) -> Option<Duration> {
        self.heartbeat(now);
        if !doc.is_dirty() {
            if self.written {
                log::info!("autosave of {} removed: nothing unsaved", doc.title());
                self.clear();
            }
            return None;
        }
        if minutes == 0 || self.written_revision == Some(doc.revision) {
            return None;
        }
        let interval = Duration::from_secs(u64::from(minutes) * 60);
        let due = self.last_write + interval;
        if now < due {
            return Some(due - now);
        }
        self.write(doc, now);
        None
    }

    /// Write the document's autosave now.
    pub fn write(&mut self, doc: &Document, now: Instant) {
        let start = Instant::now();
        match write_entry(
            &self.dir,
            &self.id,
            &self.session,
            &doc.board,
            doc.path.as_deref(),
        ) {
            Ok(bytes) => {
                log::info!(
                    "autosaved {} ({bytes} bytes, {} ms) to {}",
                    doc.title(),
                    start.elapsed().as_millis(),
                    crate::logging::path(&self.dir.join(format!("{}.brd", self.id)))
                );
                self.written = true;
            }
            Err(e) => log::error!("autosave failed: {e}"),
        }
        self.written_revision = Some(doc.revision);
        self.last_write = now;
    }

    /// Remove the open document's autosave.
    pub fn clear(&mut self) {
        if self.written {
            remove_entry(&self.dir, &self.id);
            self.written = false;
        }
        self.written_revision = None;
    }

    /// The document was closed or replaced: drop its autosave and start a new one for the next.
    pub fn new_document(&mut self, now: Instant) {
        self.clear();
        self.serial += 1;
        self.id = format!("{}-{}", self.session, self.serial);
        self.last_write = now;
    }

    /// Continue a recovered autosave: the open document now owns it, and it is rewritten under this session.
    pub fn adopt(&mut self, id: &str, doc: &Document, now: Instant) {
        self.id = id.to_owned();
        self.write(doc, now);
    }

    /// Normal exit: remove the autosave and the lock.
    pub fn shutdown(&mut self) {
        self.clear();
        // Closing the file releases the lock (and lets Windows delete it).
        self.lock = None;
        let _ = std::fs::remove_file(lock_path(&self.dir, &self.session));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/unit-tmp")
            .join(name);
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn board(name: &str) -> Board {
        let mut b = surfcraft_io::templates::TEMPLATES[0].board();
        b.info.name = name.into();
        b
    }

    fn files(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| !n.starts_with("session-"))
            .collect();
        v.sort();
        v
    }

    #[test]
    fn writes_on_the_interval_only_while_unsaved() {
        let dir = temp("autosave-tick");
        let t0 = Instant::now();
        let mut a = Autosave::start(dir.clone(), t0);
        let mut doc = Document::new(board("Fish"), Some(dir.join("fish.brd")));
        assert_eq!(a.tick(&doc, 2, t0), None, "nothing unsaved");
        doc.edit(|b| b.flip());
        let wait = a.tick(&doc, 2, t0 + Duration::from_secs(30)).unwrap();
        assert_eq!(wait, Duration::from_secs(90));
        assert!(files(&dir).is_empty());
        assert_eq!(a.tick(&doc, 2, t0 + Duration::from_secs(120)), None);
        let id = a.id().to_owned();
        assert_eq!(files(&dir), [format!("{id}.brd"), format!("{id}.json")]);
        assert!(!dir.join("fish.brd").exists(), "never the user's file");
        let saved = std::fs::read(dir.join(format!("{id}.brd"))).unwrap();
        assert_eq!(surfcraft_io::brd::read(&saved).unwrap(), doc.board);
        // Same revision: no rewrite. Turned off: nothing.
        let at = std::fs::metadata(dir.join(format!("{id}.json")))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(a.tick(&doc, 2, t0 + Duration::from_secs(600)), None);
        assert_eq!(
            std::fs::metadata(dir.join(format!("{id}.json")))
                .unwrap()
                .modified()
                .unwrap(),
            at
        );
        doc.edit(|b| b.flip());
        assert_eq!(a.tick(&doc, 0, t0 + Duration::from_secs(900)), None);
        // Undo back to the saved board: the autosave goes.
        doc.undo();
        doc.undo();
        assert!(!doc.is_dirty());
        a.tick(&doc, 2, t0 + Duration::from_secs(901));
        assert!(files(&dir).is_empty());
    }

    #[test]
    fn save_new_document_and_shutdown_clear_the_autosave() {
        let dir = temp("autosave-clear");
        let t0 = Instant::now();
        let mut a = Autosave::start(dir.clone(), t0);
        let mut doc = Document::new(board("A"), None);
        doc.edit(|b| b.flip());
        a.write(&doc, t0);
        assert_eq!(files(&dir).len(), 2);
        // Saved: the next tick removes it.
        let out = dir.join("a.brd");
        doc.save_to(&out).unwrap();
        a.tick(&doc, 2, t0);
        assert!(files(&dir).iter().all(|f| f == "a.brd"));
        // Replaced by another document.
        doc.edit(|b| b.flip());
        a.write(&doc, t0);
        let first = a.id().to_owned();
        a.new_document(t0);
        assert_ne!(a.id(), first);
        assert!(files(&dir).iter().all(|f| f == "a.brd"));
        // Normal exit removes files and the lock.
        a.write(&doc, t0);
        a.shutdown();
        assert_eq!(files(&dir), ["a.brd"]);
        assert!(
            std::fs::read_dir(&dir)
                .unwrap()
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().starts_with("session-"))
        );
    }

    #[test]
    fn scan_lists_autosaves_of_ended_sessions_only() {
        let dir = temp("autosave-scan");
        let t0 = Instant::now();
        // A running app (holding its lock) and a crashed one whose lock file was left behind, just written: the
        // system released its lock, so it is offered even when SurfCraft is restarted right after the crash.
        let mut live = Autosave::start(dir.clone(), t0);
        let mut doc = Document::new(board("Live"), None);
        doc.edit(|b| b.flip());
        live.write(&doc, t0);
        let original = dir.join("crashed.brd");
        std::fs::write(lock_path(&dir, "999-1"), "999-1").unwrap();
        write_entry(&dir, "999-1-0", "999-1", &board("Crashed"), Some(&original)).unwrap();
        write_entry(&dir, "999-1-1", "999-1", &board(""), None).unwrap();
        std::fs::write(dir.join("broken.json"), "not json").unwrap();
        let found = scan(&dir);
        assert_eq!(found.len(), 2, "{found:?}");
        let crashed = found.iter().find(|e| e.id == "999-1-0").unwrap();
        assert_eq!(crashed.meta.original(), Some(original));
        assert_eq!(crashed.meta.name, "Crashed");
        assert_eq!(crashed.board.as_ref().unwrap().info.name, "Crashed");
        assert_eq!(crashed.meta.app_version, env!("CARGO_PKG_VERSION"));
        let untitled = found.iter().find(|e| e.id == "999-1-1").unwrap();
        assert_eq!(untitled.meta.original(), None);
        // Once the live app quits normally, nothing of it is left to recover.
        live.shutdown();
        assert_eq!(scan(&dir).len(), 2);
        remove_entry(&dir, "999-1-0");
        assert_eq!(scan(&dir).len(), 1);
    }

    #[test]
    fn adopted_autosave_is_rewritten_under_the_new_session() {
        let dir = temp("autosave-adopt");
        let t0 = Instant::now();
        write_entry(&dir, "999-1-0", "999-1", &board("Old"), None).unwrap();
        let mut a = Autosave::start(dir.clone(), t0);
        let entry = scan(&dir).pop().unwrap();
        let mut doc = Document::new(entry.board.clone().unwrap(), None);
        doc.mark_unsaved();
        a.adopt(&entry.id, &doc, t0);
        assert!(scan(&dir).is_empty(), "owned by this running app now");
        a.shutdown();
        assert!(files(&dir).is_empty());
    }
}
