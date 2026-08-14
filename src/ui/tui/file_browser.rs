//! TUI rendering for the local file browser tree panel (Phase 2 of
//! `.claude/plans/inherited-painting-lark.md`). Pure rendering — all
//! navigation/expand-collapse state lives in `app::file_browser::FileBrowserState`
//! and is shared with the (future) GUI renderer.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

use crate::app::file_browser::FileBrowserState;
use crate::config::ThemePalette;

/// Renders the tree panel: one row per visible (expanded-ancestor) entry,
/// indented by depth, with a `▼`/`▶` expand marker for directories and the
/// selected row highlighted.
pub struct FileBrowserPanel<'a> {
    pub state: &'a FileBrowserState,
    pub theme: &'a ThemePalette,
}

impl<'a> Widget for FileBrowserPanel<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (bg_r, bg_g, bg_b) = self.theme.bg_rgb();
        let (fg_r, fg_g, fg_b) = self.theme.fg_rgb();
        let bg = Color::Rgb(bg_r, bg_g, bg_b);
        let fg = Color::Rgb(fg_r, fg_g, fg_b);
        let (sel_r, sel_g, sel_b) = self.theme.ansi_color(4);
        let sel_bg = Color::Rgb(sel_r, sel_g, sel_b);
        let (sel_fg_r, sel_fg_g, sel_fg_b) = self.theme.bg_rgb();
        let sel_fg = Color::Rgb(sel_fg_r, sel_fg_g, sel_fg_b);

        // Blank the panel to the theme background first, since it's carved
        // out of the terminal area and won't otherwise be cleared.
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol(" ").set_fg(fg).set_bg(bg);
                }
            }
        }

        let selected = self.state.selected_index();
        let offset = self.state.scroll_offset();
        let width = area.width as usize;

        for (row, entry) in self
            .state
            .entries()
            .iter()
            .enumerate()
            .skip(offset)
            .take(area.height as usize)
        {
            let y = area.y + (row - offset) as u16;
            let marker = if entry.is_dir {
                if self.state.is_expanded(&entry.path) {
                    "▼ "
                } else {
                    "▶ "
                }
            } else {
                "  "
            };
            let name = entry
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?");
            let indent = "  ".repeat(entry.depth);
            let mut label = format!("{indent}{marker}{name}");
            if label.chars().count() > width {
                label = label.chars().take(width).collect();
            }

            let is_selected = row == selected;
            let (row_bg, row_fg) = if is_selected {
                (sel_bg, sel_fg)
            } else {
                (bg, fg)
            };
            let modifier = if is_selected {
                Modifier::BOLD
            } else {
                Modifier::empty()
            };

            for x in area.x..area.x + area.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol(" ")
                        .set_fg(row_fg)
                        .set_bg(row_bg)
                        .set_style(Style::default().add_modifier(modifier));
                }
            }
            for (col, ch) in label.chars().enumerate() {
                let x = area.x + col as u16;
                if x >= area.x + area.width {
                    break;
                }
                let mut buf_ch = [0u8; 4];
                let sym = ch.encode_utf8(&mut buf_ch);
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol(sym)
                        .set_fg(row_fg)
                        .set_bg(row_bg)
                        .set_style(Style::default().add_modifier(modifier));
                }
            }
        }
    }
}
