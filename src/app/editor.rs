//! UI-agnostic state for the built-in nano-like text editor.
//!
//! Deliberately non-modal: every printable key inserts immediately, there
//! is no vim-style mode switching. Both front ends (`ui::tui::editor`,
//! `ui::gui::editor`) translate their own key events into these methods so
//! the actual buffer-editing logic isn't duplicated between them.
//!
//! Landed in Phase 1 of `.claude/plans/inherited-painting-lark.md` with no
//! caller yet — wired into the TUI in Phase 4 and the GUI in Phase 5.
//! `#[allow(dead_code)]` is intentional and temporary; remove once those
//! phases land.

#![allow(dead_code)]

use std::path::PathBuf;

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
}
