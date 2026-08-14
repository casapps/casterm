//! UI-agnostic state for the built-in nano-like text editor.
//!
//! Deliberately non-modal: every printable key inserts immediately, there
//! is no vim-style mode switching. Both front ends (`ui::tui::editor`,
//! `ui::gui::editor`) translate their own key events into these methods so
//! the actual buffer-editing logic isn't duplicated between them.
//!
//! Landed in Phase 1 of `.claude/plans/inherited-painting-lark.md`; wired
//! into the TUI in Phase 4 (`ui::tui::editor`) and the GUI in Phase 5
//! (`ui::gui::editor`). `dispatch_editor_key` below (added in Phase 4) is
//! the shared key-routing table for both front ends — both use
//! `crossterm::event::KeyCode` for key events, so the dispatch table lives
//! here instead of being duplicated per front end.

use std::path::PathBuf;

use crossterm::event::KeyCode;

use crate::support::error::Result;

/// In-memory buffer + cursor state for one open file.
pub struct EditorState {
    path: PathBuf,
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    dirty: bool,
}

impl EditorState {
    /// Load `path` into a fresh editor buffer. A missing file starts as an
    /// empty single-line buffer (so the editor can also be used to create a
    /// new file); any other read error is propagated.
    pub fn load(path: PathBuf) -> Result<Self> {
        let lines = match std::fs::read_to_string(&path) {
            Ok(content) => split_lines(&content),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![String::new()],
            Err(e) => return Err(e.into()),
        };
        Ok(Self {
            path,
            lines,
            cursor_row: 0,
            cursor_col: 0,
            dirty: false,
        })
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor_row, self.cursor_col)
    }

    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// Write the buffer back to `path`, joining lines with `\n` and a
    /// trailing newline.
    pub fn save(&mut self) -> Result<()> {
        let mut content = self.lines.join("\n");
        content.push('\n');
        std::fs::write(&self.path, content)?;
        self.dirty = false;
        Ok(())
    }

    /// Insert one character at the cursor, advancing the cursor past it.
    pub fn insert_char(&mut self, ch: char) {
        if ch == '\n' {
            self.newline();
            return;
        }
        let line = &mut self.lines[self.cursor_row];
        let byte_idx = char_to_byte_idx(line, self.cursor_col);
        line.insert(byte_idx, ch);
        self.cursor_col += 1;
        self.dirty = true;
    }

    /// Split the current line at the cursor into two lines.
    pub fn newline(&mut self) {
        let line = &mut self.lines[self.cursor_row];
        let byte_idx = char_to_byte_idx(line, self.cursor_col);
        let rest = line.split_off(byte_idx);
        self.lines.insert(self.cursor_row + 1, rest);
        self.cursor_row += 1;
        self.cursor_col = 0;
        self.dirty = true;
    }

    /// Delete the character before the cursor, joining with the previous
    /// line if the cursor is at column 0 of a non-first line.
    pub fn backspace(&mut self) {
        if self.cursor_col > 0 {
            let line = &mut self.lines[self.cursor_row];
            let start = char_to_byte_idx(line, self.cursor_col - 1);
            let end = char_to_byte_idx(line, self.cursor_col);
            line.replace_range(start..end, "");
            self.cursor_col -= 1;
            self.dirty = true;
        } else if self.cursor_row > 0 {
            let current = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            let prev_len = self.lines[self.cursor_row].chars().count();
            self.lines[self.cursor_row].push_str(&current);
            self.cursor_col = prev_len;
            self.dirty = true;
        }
    }

    /// Delete the character at the cursor (forward delete), joining with
    /// the next line if the cursor is at the end of a non-last line.
    pub fn delete_forward(&mut self) {
        let line_len = self.lines[self.cursor_row].chars().count();
        if self.cursor_col < line_len {
            let line = &mut self.lines[self.cursor_row];
            let start = char_to_byte_idx(line, self.cursor_col);
            let end = char_to_byte_idx(line, self.cursor_col + 1);
            line.replace_range(start..end, "");
            self.dirty = true;
        } else if self.cursor_row + 1 < self.lines.len() {
            let next = self.lines.remove(self.cursor_row + 1);
            self.lines[self.cursor_row].push_str(&next);
            self.dirty = true;
        }
    }

    /// Move the cursor by `(row_delta, col_delta)`, clamping to valid
    /// positions (row delta applied first, then column clamped to the
    /// resulting line's length).
    pub fn move_cursor(&mut self, row_delta: isize, col_delta: isize) {
        let new_row =
            (self.cursor_row as isize + row_delta).clamp(0, self.lines.len() as isize - 1);
        self.cursor_row = new_row as usize;

        let line_len = self.lines[self.cursor_row].chars().count() as isize;
        let new_col = (self.cursor_col as isize + col_delta).clamp(0, line_len);
        self.cursor_col = new_col as usize;
    }

    pub fn move_home(&mut self) {
        self.cursor_col = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor_col = self.lines[self.cursor_row].chars().count();
    }
}

/// What happened when a key was routed into the editor via
/// `dispatch_editor_key`, so a front end's own key handler can update its
/// UI-layer state (e.g. a transient status message, or switching back to
/// the tree view) without duplicating the dispatch table.
#[derive(Debug, PartialEq)]
pub enum EditorKeyOutcome {
    /// The key was handled entirely inside `EditorState` (edit, cursor
    /// move, or an unmapped key that fell through to nothing).
    Handled,
    /// `Ctrl+S`: buffer saved (`Ok`) or failed (`Err` with a message).
    Saved(std::result::Result<(), String>),
    /// `Ctrl+X`: exit the editor back to the tree view.
    Exit,
}

/// Route one key event into `EditorState`'s edit methods — the nano-style
/// hint-bar bindings (`Ctrl+S` save, `Ctrl+X` exit) plus non-modal
/// insert/delete/navigate for everything else. A free function (no
/// front-end dependency) shared by both `ui::tui::mod::TuiApp` and
/// `ui::gui::window::WindowState` so the dispatch table itself is
/// unit-testable without constructing a full `TuiApp`/`WindowState`.
pub fn dispatch_editor_key(
    editor: &mut EditorState,
    code: KeyCode,
    ctrl: bool,
) -> EditorKeyOutcome {
    match code {
        KeyCode::Char('s') if ctrl => {
            EditorKeyOutcome::Saved(editor.save().map_err(|e| e.to_string()))
        }
        KeyCode::Char('x') if ctrl => EditorKeyOutcome::Exit,
        KeyCode::Char(ch) => {
            editor.insert_char(ch);
            EditorKeyOutcome::Handled
        }
        KeyCode::Enter => {
            editor.newline();
            EditorKeyOutcome::Handled
        }
        KeyCode::Backspace => {
            editor.backspace();
            EditorKeyOutcome::Handled
        }
        KeyCode::Delete => {
            editor.delete_forward();
            EditorKeyOutcome::Handled
        }
        KeyCode::Left => {
            editor.move_cursor(0, -1);
            EditorKeyOutcome::Handled
        }
        KeyCode::Right => {
            editor.move_cursor(0, 1);
            EditorKeyOutcome::Handled
        }
        KeyCode::Up => {
            editor.move_cursor(-1, 0);
            EditorKeyOutcome::Handled
        }
        KeyCode::Down => {
            editor.move_cursor(1, 0);
            EditorKeyOutcome::Handled
        }
        KeyCode::Home => {
            editor.move_home();
            EditorKeyOutcome::Handled
        }
        KeyCode::End => {
            editor.move_end();
            EditorKeyOutcome::Handled
        }
        KeyCode::Tab => {
            editor.insert_char('\t');
            EditorKeyOutcome::Handled
        }
        _ => EditorKeyOutcome::Handled,
    }
}

fn split_lines(content: &str) -> Vec<String> {
    if content.is_empty() {
        return vec![String::new()];
    }
    content.split('\n').map(|s| s.to_string()).collect()
}

/// Convert a character-index cursor column to a byte offset for `String`
/// slicing/insertion (handles multi-byte UTF-8 correctly).
fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "casterm-editor-test-{}-{}-{name}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn load_missing_file_starts_empty() {
        let path = temp_path("missing.txt");
        let state = EditorState::load(path).unwrap();
        assert_eq!(state.lines(), &[String::new()]);
        assert!(!state.dirty());
    }

    #[test]
    fn insert_and_save_round_trip() {
        let path = temp_path("roundtrip.txt");
        let mut state = EditorState::load(path.clone()).unwrap();
        for ch in "hello".chars() {
            state.insert_char(ch);
        }
        state.newline();
        for ch in "world".chars() {
            state.insert_char(ch);
        }
        assert!(state.dirty());
        state.save().unwrap();
        assert!(!state.dirty());

        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert_eq!(on_disk, "hello\nworld\n");
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn backspace_joins_previous_line() {
        let path = temp_path("backspace.txt");
        let mut state = EditorState::load(path).unwrap();
        for ch in "ab".chars() {
            state.insert_char(ch);
        }
        state.newline();
        for ch in "cd".chars() {
            state.insert_char(ch);
        }
        state.move_cursor(0, -2);
        state.backspace();
        assert_eq!(state.lines(), &["abcd".to_string()]);
        assert_eq!(state.cursor(), (0, 2));
    }

    #[test]
    fn delete_forward_joins_next_line() {
        let path = temp_path("delete.txt");
        let mut state = EditorState::load(path).unwrap();
        state.insert_char('a');
        state.newline();
        state.insert_char('b');
        state.move_cursor(-1, 0);
        state.move_end();
        state.delete_forward();
        assert_eq!(state.lines(), &["ab".to_string()]);
    }

    #[test]
    fn move_cursor_clamps_to_bounds() {
        let path = temp_path("clamp.txt");
        let mut state = EditorState::load(path).unwrap();
        state.move_cursor(-10, -10);
        assert_eq!(state.cursor(), (0, 0));
        state.move_cursor(10, 10);
        assert_eq!(state.cursor(), (0, 0));
    }

    fn temp_editor(name: &str) -> EditorState {
        EditorState::load(temp_path(name)).unwrap()
    }

    #[test]
    fn ctrl_s_saves_and_reports_success() {
        let mut editor = temp_editor("dispatch-save.txt");
        editor.insert_char('x');
        let outcome = dispatch_editor_key(&mut editor, KeyCode::Char('s'), true);
        assert_eq!(outcome, EditorKeyOutcome::Saved(Ok(())));
        assert!(!editor.dirty());
        std::fs::remove_file(editor.path()).unwrap();
    }

    #[test]
    fn ctrl_x_exits() {
        let mut editor = temp_editor("dispatch-exit.txt");
        let outcome = dispatch_editor_key(&mut editor, KeyCode::Char('x'), true);
        assert_eq!(outcome, EditorKeyOutcome::Exit);
    }

    #[test]
    fn plain_char_falls_through_to_insert() {
        let mut editor = temp_editor("dispatch-insert.txt");
        let outcome = dispatch_editor_key(&mut editor, KeyCode::Char('a'), false);
        assert_eq!(outcome, EditorKeyOutcome::Handled);
        assert_eq!(editor.lines(), &["a".to_string()]);
    }

    #[test]
    fn ctrl_char_other_than_s_or_x_falls_through_to_insert() {
        // Only `s` and `x` are hint-bar bindings; every other Ctrl+letter
        // still inserts non-modally rather than being silently swallowed.
        let mut editor = temp_editor("dispatch-ctrl-other.txt");
        let outcome = dispatch_editor_key(&mut editor, KeyCode::Char('q'), true);
        assert_eq!(outcome, EditorKeyOutcome::Handled);
        assert_eq!(editor.lines(), &["q".to_string()]);
    }

    #[test]
    fn enter_maps_to_newline() {
        let mut editor = temp_editor("dispatch-newline.txt");
        editor.insert_char('a');
        let outcome = dispatch_editor_key(&mut editor, KeyCode::Enter, false);
        assert_eq!(outcome, EditorKeyOutcome::Handled);
        assert_eq!(editor.lines(), &["a".to_string(), String::new()]);
    }

    #[test]
    fn backspace_maps_to_backspace() {
        let mut editor = temp_editor("dispatch-backspace.txt");
        editor.insert_char('a');
        let outcome = dispatch_editor_key(&mut editor, KeyCode::Backspace, false);
        assert_eq!(outcome, EditorKeyOutcome::Handled);
        assert_eq!(editor.lines(), &[String::new()]);
    }

    #[test]
    fn arrow_and_home_end_map_to_cursor_movement() {
        let mut editor = temp_editor("dispatch-move.txt");
        for ch in "ab".chars() {
            editor.insert_char(ch);
        }
        dispatch_editor_key(&mut editor, KeyCode::Home, false);
        assert_eq!(editor.cursor(), (0, 0));
        dispatch_editor_key(&mut editor, KeyCode::End, false);
        assert_eq!(editor.cursor(), (0, 2));
        dispatch_editor_key(&mut editor, KeyCode::Left, false);
        assert_eq!(editor.cursor(), (0, 1));
        dispatch_editor_key(&mut editor, KeyCode::Right, false);
        assert_eq!(editor.cursor(), (0, 2));
    }

    #[test]
    fn unmapped_key_is_a_no_op() {
        let mut editor = temp_editor("dispatch-unmapped.txt");
        let outcome = dispatch_editor_key(&mut editor, KeyCode::F(5), false);
        assert_eq!(outcome, EditorKeyOutcome::Handled);
        assert_eq!(editor.lines(), &[String::new()]);
    }
}
