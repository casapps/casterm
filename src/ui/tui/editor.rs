//! TUI rendering for the built-in nano-like text editor (Phase 4 of
//! `.claude/plans/inherited-painting-lark.md`). Pure rendering — buffer/
//! cursor state lives in `app::editor::EditorState` and edit operations are
//! shared with the (future) GUI editor (Phase 5).

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

use crate::app::editor::EditorState;
use crate::config::ThemePalette;

/// Renders the editor buffer as a scrollable text area, with a nano-style
/// header row (file name, `[Modified]` when `EditorState::dirty()`) on top
/// and a bottom key-hint bar (`^S Save  ^X Exit  ^T Close Panel`).
/// `status`, when `Some`, replaces the hint bar with a transient message
/// (e.g. "Saved" or a save error) for one frame.
pub struct EditorPanel<'a> {
    pub state: &'a EditorState,
    pub theme: &'a ThemePalette,
    pub status: Option<&'a str>,
}

/// Default nano-style key-hint bar text, shown when no transient `status`
/// message is set.
const HINT_BAR: &str = "^S Save  ^X Exit  ^T Close Panel";

impl<'a> Widget for EditorPanel<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let (bg_r, bg_g, bg_b) = self.theme.bg_rgb();
        let (fg_r, fg_g, fg_b) = self.theme.fg_rgb();
        let bg = Color::Rgb(bg_r, bg_g, bg_b);
        let fg = Color::Rgb(fg_r, fg_g, fg_b);
        let (bar_r, bar_g, bar_b) = self.theme.ansi_color(4);
        let bar_bg = Color::Rgb(bar_r, bar_g, bar_b);
        let (bar_fg_r, bar_fg_g, bar_fg_b) = self.theme.bg_rgb();
        let bar_fg = Color::Rgb(bar_fg_r, bar_fg_g, bar_fg_b);

        let width = area.width as usize;

        // Header row: file name plus a `[Modified]` marker while the
        // buffer has unsaved changes — nano shows the same at the top.
        let header_y = area.y;
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut((x, header_y)) {
                cell.set_symbol(" ")
                    .set_fg(bar_fg)
                    .set_bg(bar_bg)
                    .set_style(Style::default());
            }
        }
        let name = self
            .state
            .path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("[No Name]");
        let header_text = if self.state.dirty() {
            format!("{name} [Modified]")
        } else {
            name.to_string()
        };
        for (col, ch) in header_text.chars().take(width).enumerate() {
            let x = area.x + col as u16;
            let mut byte_buf = [0u8; 4];
            let sym = ch.encode_utf8(&mut byte_buf);
            if let Some(cell) = buf.cell_mut((x, header_y)) {
                cell.set_symbol(sym).set_fg(bar_fg).set_bg(bar_bg);
            }
        }

        let text_height = area.height.saturating_sub(2);
        let text_y0 = area.y + 1;

        // Blank the text area to the theme background first, since it's
        // carved out of the terminal area and won't otherwise be cleared.
        for y in text_y0..text_y0 + text_height {
            for x in area.x..area.x + area.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol(" ").set_fg(fg).set_bg(bg);
                }
            }
        }

        // Scroll so the cursor row stays within the visible window —
        // mirrors `FileBrowserPanel`'s scroll-offset clamp, computed here
        // directly since the editor has no separate stored scroll field.
        let (cursor_row, _) = self.state.cursor();
        let scroll = if text_height == 0 {
            0
        } else if cursor_row >= text_height as usize {
            cursor_row + 1 - text_height as usize
        } else {
            0
        };

        for (row, line) in self
            .state
            .lines()
            .iter()
            .enumerate()
            .skip(scroll)
            .take(text_height as usize)
        {
            let y = text_y0 + (row - scroll) as u16;
            let truncated: String = line.chars().take(width).collect();
            for (col, ch) in truncated.chars().enumerate() {
                let x = area.x + col as u16;
                let mut byte_buf = [0u8; 4];
                let sym = ch.encode_utf8(&mut byte_buf);
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol(sym).set_fg(fg).set_bg(bg);
                }
            }
        }

        // Bottom key-hint bar (or transient status message).
        let hint_y = area.y + area.height - 1;
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut((x, hint_y)) {
                cell.set_symbol(" ")
                    .set_fg(bar_fg)
                    .set_bg(bar_bg)
                    .set_style(Style::default());
            }
        }
        let hint_text = self.status.unwrap_or(HINT_BAR);
        for (col, ch) in hint_text.chars().take(width).enumerate() {
            let x = area.x + col as u16;
            let mut byte_buf = [0u8; 4];
            let sym = ch.encode_utf8(&mut byte_buf);
            if let Some(cell) = buf.cell_mut((x, hint_y)) {
                cell.set_symbol(sym).set_fg(bar_fg).set_bg(bar_bg);
            }
        }
    }
}
