//! Shared terminal-cell-to-screen-representation model
//!
//! Both `ui::tui` (ratatui buffer cells) and `ui::gui` (wgpu textured
//! quads) need to turn `app::terminal::Terminal`'s `Grid`/`Cell` data into
//! concrete foreground/background colors, attribute flags, and key-press
//! byte sequences. This module does that conversion once so cursor/cell/
//! color/key-encoding logic isn't duplicated between the two UI backends.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::terminal::{CursorStyle, TermColor, Terminal as TerminalEmulator};
use crate::config::ThemePalette;

/// 24-bit RGB color, already resolved from a `TermColor` + theme palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// One terminal cell's fully resolved on-screen representation: the
/// character to draw plus final (post reverse-video/cursor-swap) colors and
/// attribute flags. Renderer-agnostic — `ui::tui` maps this to a ratatui
/// `Style`, `ui::gui` maps it to vertex colors for a textured quad.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedCell {
    pub ch: char,
    pub fg: Rgb,
    pub bg: Rgb,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub blink: bool,
    pub hidden: bool,
    pub strikethrough: bool,
    /// True for the single cell currently under the (visible) cursor.
    /// `ui::gui` uses this to shape the cursor quad per `CursorStyle`
    /// instead of always filling the whole cell.
    pub is_cursor: bool,
}

/// Resolve a `TermColor` to a concrete `Rgb`, using the theme palette for
/// ANSI indices 0-15 and falling back to the theme's default fg/bg for
/// `TermColor::Default`.
pub fn resolve_color(
    color: TermColor,
    is_fg: bool,
    theme: &ThemePalette,
    default_fg: Rgb,
    default_bg: Rgb,
) -> Rgb {
    match color {
        TermColor::Default => {
            if is_fg {
                default_fg
            } else {
                default_bg
            }
        }
        TermColor::Indexed(n) if n < 16 => {
            let (r, g, b) = theme.ansi_color(n);
            Rgb(r, g, b)
        }
        TermColor::Indexed(n) => {
            let (r, g, b) = xterm_256_rgb(n);
            Rgb(r, g, b)
        }
        TermColor::Rgb(r, g, b) => Rgb(r, g, b),
    }
}

/// Standard xterm 256-color palette lookup for indices 16-255 (0-15 are
/// theme-driven via `ThemePalette::ansi_color` instead, so callers only
/// reach this branch for the 6x6x6 color cube and grayscale ramp).
fn xterm_256_rgb(index: u8) -> (u8, u8, u8) {
    if index >= 232 {
        let level = 8 + (index - 232) * 10;
        return (level, level, level);
    }
    let idx = index - 16;
    let levels = [0u8, 95, 135, 175, 215, 255];
    let r = levels[(idx / 36) as usize];
    let g = levels[((idx / 6) % 6) as usize];
    let b = levels[(idx % 6) as usize];
    (r, g, b)
}

/// Read back the emulator's current cursor shape (block/underline/bar), as
/// set at runtime via DECSCUSR (CSI `q`). GUI-only consumer: `ui::gui`
/// varies the cursor quad's geometry per style; `ui::tui` has no partial-
/// cell drawing primitive and always renders a full-cell swap.
pub fn cursor_style(term: &TerminalEmulator) -> CursorStyle {
    term.cursor_style()
}

/// Resolve the full visible grid for one frame into row-major
/// `ResolvedCell`s (`size.rows * size.cols` entries, matching
/// `emulator.size()`).
pub fn resolve_grid(emulator: &TerminalEmulator, theme: &ThemePalette) -> Vec<ResolvedCell> {
    let grid = emulator.grid();
    let cursor = emulator.cursor();
    let size = emulator.size();

    let (dfr, dfg, dfb) = theme.fg_rgb();
    let (dbr, dbg, dbb) = theme.bg_rgb();
    let default_fg = Rgb(dfr, dfg, dfb);
    let default_bg = Rgb(dbr, dbg, dbb);

    let mut out = Vec::with_capacity(size.rows as usize * size.cols as usize);
    for row in 0..size.rows {
        for col in 0..size.cols {
            let cell = grid.get(row, col).cloned().unwrap_or_default();
            let is_cursor = cursor.row == row && cursor.col == col && emulator.cursor_visible();

            let mut fg = resolve_color(cell.attrs.fg, true, theme, default_fg, default_bg);
            let mut bg = resolve_color(cell.attrs.bg, false, theme, default_fg, default_bg);

            // Reverse video attribute swaps fg/bg.
            if cell.attrs.reverse {
                std::mem::swap(&mut fg, &mut bg);
            }
            // Cursor: invert the cell colors.
            if is_cursor {
                std::mem::swap(&mut fg, &mut bg);
            }

            let display_char = if cell.char == '\0' { ' ' } else { cell.char };

            out.push(ResolvedCell {
                ch: display_char,
                fg,
                bg,
                bold: cell.attrs.bold,
                italic: cell.attrs.italic,
                underline: cell.attrs.underline,
                blink: cell.attrs.blink,
                hidden: cell.attrs.hidden,
                strikethrough: cell.attrs.strikethrough,
                is_cursor,
            });
        }
    }
    out
}

/// Encode a crossterm-shaped `KeyEvent` to the byte sequence sent over the
/// PTY. Shared by `ui::tui`'s crossterm event loop and `ui::gui`'s winit
/// event loop (the latter translates winit key events into this same
/// `KeyEvent` shape before calling here) so the two front ends never
/// diverge on how a keypress becomes PTY bytes.
pub fn encode_key(key: KeyEvent) -> Vec<u8> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    let mut bytes: Vec<u8> = match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                let byte = (c as u8).to_ascii_lowercase();
                if byte.is_ascii_lowercase() {
                    vec![byte - b'a' + 1]
                } else {
                    match c {
                        '@' => vec![0x00],
                        '[' => vec![0x1B],
                        '\\' => vec![0x1C],
                        ']' => vec![0x1D],
                        '^' => vec![0x1E],
                        '_' => vec![0x1F],
                        _ => {
                            let mut buf = [0u8; 4];
                            c.encode_utf8(&mut buf).as_bytes().to_vec()
                        }
                    }
                }
            } else {
                let mut buf = [0u8; 4];
                c.encode_utf8(&mut buf).as_bytes().to_vec()
            }
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7F],
        KeyCode::Delete => vec![0x1B, b'[', b'3', b'~'],
        KeyCode::Esc => vec![0x1B],
        KeyCode::Tab => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                vec![0x1B, b'[', b'Z']
            } else {
                vec![b'\t']
            }
        }
        KeyCode::Up => vec![0x1B, b'[', b'A'],
        KeyCode::Down => vec![0x1B, b'[', b'B'],
        KeyCode::Right => vec![0x1B, b'[', b'C'],
        KeyCode::Left => vec![0x1B, b'[', b'D'],
        KeyCode::Home => vec![0x1B, b'[', b'H'],
        KeyCode::End => vec![0x1B, b'[', b'F'],
        KeyCode::PageUp => vec![0x1B, b'[', b'5', b'~'],
        KeyCode::PageDown => vec![0x1B, b'[', b'6', b'~'],
        KeyCode::F(n) => match n {
            1 => vec![0x1B, b'O', b'P'],
            2 => vec![0x1B, b'O', b'Q'],
            3 => vec![0x1B, b'O', b'R'],
            4 => vec![0x1B, b'O', b'S'],
            5 => vec![0x1B, b'[', b'1', b'5', b'~'],
            6 => vec![0x1B, b'[', b'1', b'7', b'~'],
            7 => vec![0x1B, b'[', b'1', b'8', b'~'],
            8 => vec![0x1B, b'[', b'1', b'9', b'~'],
            9 => vec![0x1B, b'[', b'2', b'0', b'~'],
            10 => vec![0x1B, b'[', b'2', b'1', b'~'],
            11 => vec![0x1B, b'[', b'2', b'3', b'~'],
            12 => vec![0x1B, b'[', b'2', b'4', b'~'],
            _ => vec![],
        },
        _ => vec![],
    };

    // Alt prefix: prepend ESC.
    if alt && !bytes.is_empty() {
        let mut alt_bytes = vec![0x1B];
        alt_bytes.extend_from_slice(&bytes);
        bytes = alt_bytes;
    }

    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::terminal::{CellAttrs, TerminalSize};
    use crate::config::ThemePalette;

    #[test]
    fn resolve_color_default_uses_theme_defaults() {
        let theme = ThemePalette::default();
        let default_fg = Rgb(1, 2, 3);
        let default_bg = Rgb(4, 5, 6);
        assert_eq!(
            resolve_color(TermColor::Default, true, &theme, default_fg, default_bg),
            default_fg
        );
        assert_eq!(
            resolve_color(TermColor::Default, false, &theme, default_fg, default_bg),
            default_bg
        );
    }

    #[test]
    fn resolve_color_rgb_passthrough() {
        let theme = ThemePalette::default();
        let default_fg = Rgb(0, 0, 0);
        let default_bg = Rgb(0, 0, 0);
        assert_eq!(
            resolve_color(
                TermColor::Rgb(10, 20, 30),
                true,
                &theme,
                default_fg,
                default_bg
            ),
            Rgb(10, 20, 30)
        );
    }

    #[test]
    fn xterm_256_grayscale_ramp() {
        assert_eq!(xterm_256_rgb(232), (8, 8, 8));
        assert_eq!(xterm_256_rgb(255), (238, 238, 238));
    }

    #[test]
    fn resolve_grid_marks_visible_cursor() {
        let mut term = TerminalEmulator::new(TerminalSize { cols: 4, rows: 2 });
        term.write_char('a');
        term.write_char('b');
        term.set_cursor(0, 1);
        let theme = ThemePalette::default();
        let cells = resolve_grid(&term, &theme);
        let idx = 1;
        // Cursor colors are swapped relative to a non-cursor cell with the
        // same underlying attrs.
        assert_eq!(cells[idx].fg, cells[0].bg);
        assert_eq!(cells[idx].bg, cells[0].fg);
    }

    #[test]
    fn resolve_grid_null_char_renders_as_space() {
        let term = TerminalEmulator::new(TerminalSize { cols: 2, rows: 1 });
        let theme = ThemePalette::default();
        let cells = resolve_grid(&term, &theme);
        assert_eq!(cells[0].ch, ' ');
    }

    #[test]
    fn resolve_grid_reverse_video_swaps_colors() {
        let mut term = TerminalEmulator::new(TerminalSize { cols: 1, rows: 1 });
        let attrs = CellAttrs {
            reverse: true,
            ..Default::default()
        };
        term.set_attrs(attrs);
        term.write_char('x');
        let theme = ThemePalette::default();
        let cells = resolve_grid(&term, &theme);
        let (dfr, dfg, dfb) = theme.fg_rgb();
        let (dbr, dbg, dbb) = theme.bg_rgb();
        assert_eq!(cells[0].fg, Rgb(dbr, dbg, dbb));
        assert_eq!(cells[0].bg, Rgb(dfr, dfg, dfb));
    }

    #[test]
    fn encode_key_basic_chars_and_control() {
        assert_eq!(
            encode_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
            b"a".to_vec()
        );
        assert_eq!(
            encode_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            vec![0x03]
        );
        assert_eq!(
            encode_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            vec![b'\r']
        );
        assert_eq!(
            encode_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            vec![0x1B]
        );
    }

    #[test]
    fn encode_key_alt_prefixes_escape() {
        assert_eq!(
            encode_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT)),
            vec![0x1B, b'x']
        );
    }

    #[test]
    fn encode_key_arrow_and_function_keys() {
        assert_eq!(
            encode_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
            vec![0x1B, b'[', b'A']
        );
        assert_eq!(
            encode_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)),
            vec![0x1B, b'O', b'P']
        );
    }
}
