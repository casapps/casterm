//! VTE escape sequence parser/processor

use vte::{Params, Parser, Perform};

use super::terminal::{Cell, CellAttrs, CursorStyle, TermColor, Terminal};

/// VTE-based terminal processor
pub struct VteProcessor {
    parser: Parser,
    pending: Vec<u8>,
}

impl VteProcessor {
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
            pending: Vec::new(),
        }
    }

    /// Feed bytes to the parser and apply actions to the terminal
    pub fn process(&mut self, terminal: &mut Terminal, data: &[u8]) {
        let mut performer = Performer { terminal };
        self.parser.advance(&mut performer, data);
    }
}

/// Implements the VTE Perform trait to apply escape sequences to the terminal
struct Performer<'a> {
    terminal: &'a mut Terminal,
}

impl<'a> Perform for Performer<'a> {
    fn print(&mut self, c: char) {
        self.terminal.write_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            // BEL
            0x07 => {}
            // Backspace
            0x08 => self.terminal.write_char('\x08'),
            // Tab
            0x09 => self.terminal.write_char('\t'),
            // LF/VT/FF
            0x0A | 0x0B | 0x0C => self.terminal.write_char('\n'),
            // CR
            0x0D => self.terminal.write_char('\r'),
            _ => {}
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}

    fn put(&mut self, _byte: u8) {}

    fn unhook(&mut self) {}

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        // OSC sequences: title setting, etc.
        if params.len() >= 2 {
            match params[0] {
                // OSC 0 or 2: set title
                b"0" | b"2" => {
                    if let Ok(title) = std::str::from_utf8(params[1]) {
                        self.terminal.set_title(title);
                    }
                }
                _ => {}
            }
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        let ps: Vec<u16> = params.iter().flat_map(|s| s.iter()).copied().collect();

        let p0 = ps.first().copied().unwrap_or(0);

        match action {
            // CUU: cursor up
            'A' => {
                let n = p0.max(1) as u16;
                let cursor = self.terminal.cursor();
                self.terminal
                    .set_cursor(cursor.row.saturating_sub(n), cursor.col);
            }
            // CUD: cursor down
            'B' => {
                let n = p0.max(1) as u16;
                let cursor = self.terminal.cursor();
                let max_row = self.terminal.size().rows.saturating_sub(1);
                self.terminal
                    .set_cursor((cursor.row + n).min(max_row), cursor.col);
            }
            // CUF: cursor forward
            'C' => {
                let n = p0.max(1) as u16;
                let cursor = self.terminal.cursor();
                let max_col = self.terminal.size().cols.saturating_sub(1);
                self.terminal
                    .set_cursor(cursor.row, (cursor.col + n).min(max_col));
            }
            // CUB: cursor backward
            'D' => {
                let n = p0.max(1) as u16;
                let cursor = self.terminal.cursor();
                self.terminal
                    .set_cursor(cursor.row, cursor.col.saturating_sub(n));
            }
            // CUP/HVP: cursor position (1-indexed)
            'H' | 'f' => {
                let row = p0.saturating_sub(1) as u16;
                let col = ps.get(1).copied().unwrap_or(1).saturating_sub(1) as u16;
                self.terminal.set_cursor(row, col);
            }
            // ED: erase in display
            'J' => match p0 {
                0 => {
                    // Erase from cursor to end of screen
                    let cursor = self.terminal.cursor();
                    let size = self.terminal.size();
                    for row in cursor.row..size.rows {
                        let start_col = if row == cursor.row { cursor.col } else { 0 };
                        for col in start_col..size.cols {
                            self.terminal
                                .grid_mut()
                                .set(row, col, Cell::default());
                        }
                    }
                }
                1 => {
                    // Erase from start to cursor
                    let cursor = self.terminal.cursor();
                    for row in 0..=cursor.row {
                        let end_col = if row == cursor.row { cursor.col } else { self.terminal.size().cols };
                        for col in 0..end_col {
                            self.terminal
                                .grid_mut()
                                .set(row, col, Cell::default());
                        }
                    }
                }
                2 | 3 => {
                    // Erase entire screen
                    self.terminal.clear();
                }
                _ => {}
            },
            // EL: erase in line
            'K' => {
                let cursor = self.terminal.cursor();
                let size = self.terminal.size();
                match p0 {
                    0 => {
                        for col in cursor.col..size.cols {
                            self.terminal.grid_mut().set(cursor.row, col, Cell::default());
                        }
                    }
                    1 => {
                        for col in 0..=cursor.col {
                            self.terminal.grid_mut().set(cursor.row, col, Cell::default());
                        }
                    }
                    2 => {
                        for col in 0..size.cols {
                            self.terminal.grid_mut().set(cursor.row, col, Cell::default());
                        }
                    }
                    _ => {}
                }
            }
            // SGR: select graphic rendition
            'm' => self.apply_sgr(&ps),
            // SM/RM: set/reset mode (cursor visibility, etc.)
            'h' | 'l' => {
                let set = action == 'h';
                for &p in &ps {
                    match p {
                        // DECTCEM: cursor visibility
                        25 => self.terminal.set_cursor_visible(set),
                        // Alternate screen
                        47 | 1047 | 1049 => {
                            if set {
                                self.terminal.enter_alt_screen();
                            } else {
                                self.terminal.leave_alt_screen();
                            }
                        }
                        _ => {}
                    }
                }
            }
            // DECSCUSR: cursor style
            'q' => match p0 {
                0 | 1 | 2 => self.terminal.set_cursor_style(CursorStyle::Block),
                3 | 4 => self.terminal.set_cursor_style(CursorStyle::Underline),
                5 | 6 => self.terminal.set_cursor_style(CursorStyle::Bar),
                _ => {}
            },
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}

impl<'a> Performer<'a> {
    fn apply_sgr(&mut self, params: &[u16]) {
        if params.is_empty() || params == [0] {
            self.terminal.reset_attrs();
            return;
        }

        let mut attrs = *self.terminal.current_attrs();
        let mut i = 0;

        while i < params.len() {
            match params[i] {
                0 => attrs = CellAttrs::default(),
                1 => attrs.bold = true,
                3 => attrs.italic = true,
                4 => attrs.underline = true,
                5 => attrs.blink = true,
                7 => attrs.reverse = true,
                8 => attrs.hidden = true,
                9 => attrs.strikethrough = true,
                22 => attrs.bold = false,
                23 => attrs.italic = false,
                24 => attrs.underline = false,
                25 => attrs.blink = false,
                27 => attrs.reverse = false,
                28 => attrs.hidden = false,
                29 => attrs.strikethrough = false,
                // Standard foreground colors (30-37)
                n @ 30..=37 => attrs.fg = TermColor::Indexed((n - 30) as u8),
                // Default foreground
                39 => attrs.fg = TermColor::Default,
                // Standard background colors (40-47)
                n @ 40..=47 => attrs.bg = TermColor::Indexed((n - 40) as u8),
                // Default background
                49 => attrs.bg = TermColor::Default,
                // Bright foreground colors (90-97)
                n @ 90..=97 => attrs.fg = TermColor::Indexed((n - 90 + 8) as u8),
                // Bright background colors (100-107)
                n @ 100..=107 => attrs.bg = TermColor::Indexed((n - 100 + 8) as u8),
                // 256-color foreground: 38;5;n
                38 if i + 2 < params.len() && params[i + 1] == 5 => {
                    attrs.fg = TermColor::Indexed(params[i + 2] as u8);
                    i += 2;
                }
                // 256-color background: 48;5;n
                48 if i + 2 < params.len() && params[i + 1] == 5 => {
                    attrs.bg = TermColor::Indexed(params[i + 2] as u8);
                    i += 2;
                }
                // 24-bit RGB foreground: 38;2;r;g;b
                38 if i + 4 < params.len() && params[i + 1] == 2 => {
                    attrs.fg = TermColor::Rgb(
                        params[i + 2] as u8,
                        params[i + 3] as u8,
                        params[i + 4] as u8,
                    );
                    i += 4;
                }
                // 24-bit RGB background: 48;2;r;g;b
                48 if i + 4 < params.len() && params[i + 1] == 2 => {
                    attrs.bg = TermColor::Rgb(
                        params[i + 2] as u8,
                        params[i + 3] as u8,
                        params[i + 4] as u8,
                    );
                    i += 4;
                }
                _ => {}
            }
            i += 1;
        }

        self.terminal.set_attrs(attrs);
    }
}
