//! The open board: file path, undo history, dirty tracking, safe saving.

use std::path::{Path, PathBuf};

use surfcraft_core::Board;

const HISTORY_LIMIT: usize = 200;

pub struct Document {
    pub board: Board,
    pub path: Option<PathBuf>,
    /// Snapshot shown as the "original" overlay: the board as loaded or last saved-and-refreshed.
    pub original: Board,
    saved: Board,
    undo: Vec<Board>,
    redo: Vec<Board>,
    /// Bumped on every change so cached results can tell when they are stale.
    pub revision: u64,
    /// Board before an open edit group (typing in a field), committed as one undo step by [`Self::end_group`].
    group_before: Option<Board>,
}

impl Document {
    pub fn new(board: Board, path: Option<PathBuf>) -> Self {
        Self {
            original: board.clone(),
            // A freshly opened or created board has nothing unsaved until it is edited.
            saved: board.clone(),
            board,
            path,
            undo: Vec::new(),
            redo: Vec::new(),
            revision: 0,
            group_before: None,
        }
    }

    /// Apply a change that belongs to an ongoing edit (e.g. each keystroke in a text field). All changes until
    /// [`Self::end_group`] become one undo step.
    pub fn edit_grouped(&mut self, f: impl FnOnce(&mut Board)) {
        if self.group_before.is_none() {
            self.group_before = Some(self.board.clone());
        }
        f(&mut self.board);
        self.revision += 1;
    }

    /// Close the current edit group, if any, recording it as one undo step.
    pub fn end_group(&mut self) {
        if let Some(before) = self.group_before.take() {
            self.commit(before);
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.board != self.saved
    }

    pub fn title(&self) -> String {
        let name = self
            .path
            .as_deref()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                if self.board.info.name.is_empty() {
                    "Untitled".into()
                } else {
                    self.board.info.name.clone()
                }
            });
        if self.is_dirty() {
            format!("{name} *")
        } else {
            name
        }
    }

    /// Record `before` as an undo step if the board changed since then.
    pub fn commit(&mut self, before: Board) {
        if before == self.board {
            return;
        }
        self.undo.push(before);
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.revision += 1;
    }

    /// Apply a change as one undo step.
    pub fn edit(&mut self, f: impl FnOnce(&mut Board)) {
        let before = self.board.clone();
        f(&mut self.board);
        self.commit(before);
    }

    /// Mark the board as changed without an undo step (live drag preview).
    pub fn touch(&mut self) {
        self.revision += 1;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        self.end_group();
        if let Some(b) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.board, b));
            self.revision += 1;
        }
    }

    pub fn redo(&mut self) {
        self.end_group();
        if let Some(b) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.board, b));
            self.revision += 1;
        }
    }

    /// Save as `.brd`. Writes a temp file and renames it into place; the previous file is kept as `<name>.bak`.
    pub fn save_to(&mut self, path: &Path) -> std::io::Result<()> {
        write_with_backup(path, surfcraft_io::brd::write(&self.board).as_bytes())?;
        self.path = Some(path.to_path_buf());
        self.saved = self.board.clone();
        Ok(())
    }

    /// Reset the "original" overlay to the current board.
    pub fn refresh_original(&mut self) {
        self.original = self.board.clone();
    }
}

/// Never clobber an existing file without a backup: old content goes to `<file>.bak`, new content is written to a
/// temporary file first and moved into place.
pub fn write_with_backup(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{file_name}.tmp"));
    std::fs::write(&tmp, bytes)?;
    if path.exists() {
        let backup = dir.join(format!("{file_name}.bak"));
        std::fs::copy(path, &backup)?;
    }
    std::fs::rename(&tmp, path)
}
