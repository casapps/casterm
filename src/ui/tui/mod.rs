//! TUI mode using ratatui + crossterm with full PTY-backed terminal emulation

use std::io::{stdout, Read};
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, size as terminal_size, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    prelude::*,
    style::{Color, Modifier, Style},
    widgets::Widget,
    Terminal,
};

use crate::app::pty::{Pty, PtyConfig};
use crate::app::terminal::{TermColor, Terminal as TerminalEmulator, TerminalSize};
use crate::app::vte_processor::VteProcessor;
use crate::config::{Config, StatusBarPosition, ThemePalette};
use crate::support::error::{CastermError, Result};

/// Messages from the PTY reader thread
enum PtyMsg {
    Data(Vec<u8>),
    Exit,
}

/// Current input mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputMode {
    /// All keys go directly to the PTY
    Terminal,
    /// Next key after prefix triggers a multiplexer command
    Prefix,
}

/// Full TUI application state
struct TuiApp {
    emulator: TerminalEmulator,
    vte: VteProcessor,
    pty: Pty,
    pty_rx: mpsc::Receiver<PtyMsg>,
    config: Config,
    theme: ThemePalette,
    session_name: String,
    window_index: usize,
    pane_index: usize,
    mode: InputMode,
    hostname: String,
    should_quit: bool,
}

impl TuiApp {
    fn new(config: Config, theme: ThemePalette, session_name: String) -> Result<Self> {
        let (cols, rows) = terminal_size().map_err(|e| CastermError::Tui(e.to_string()))?;
        // Reserve one row for the status bar when enabled
        let status_rows: u16 = if config.status_bar.enabled { 1 } else { 0 };
        let term_rows = rows.saturating_sub(status_rows);

        let emulator = TerminalEmulator::new(TerminalSize {
            cols,
            rows: term_rows,
        });
        let vte = VteProcessor::new();

        let shell = config
            .shell
            .path
            .clone()
            .or_else(crate::config::detect_shell)
            .unwrap_or_else(|| {
                #[cfg(windows)]
                {
                    std::path::PathBuf::from("cmd.exe")
                }
                #[cfg(not(windows))]
                {
                    std::path::PathBuf::from("/bin/sh")
                }
            });

        let mut pty_config = PtyConfig::default();
        pty_config.shell = shell;
        pty_config.rows = term_rows;
        pty_config.cols = cols;
        // Advertise true-color support so shells and editors use it
        pty_config
            .env
            .push(("TERM".to_string(), "xterm-256color".to_string()));
        pty_config
            .env
            .push(("COLORTERM".to_string(), "truecolor".to_string()));

        let mut pty = Pty::spawn(pty_config)?;

        // Move reader into a background thread; send bytes back via channel
        let (tx, pty_rx) = mpsc::channel::<PtyMsg>();
        let mut reader = pty
            .take_reader()
            .ok_or_else(|| CastermError::Pty("PTY reader not available".to_string()))?;

        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        let _ = tx.send(PtyMsg::Exit);
                        break;
                    }
                    Ok(n) => {
                        if tx.send(PtyMsg::Data(buf[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        let _ = tx.send(PtyMsg::Exit);
                        break;
                    }
                }
            }
        });

        let hostname = get_hostname();

        Ok(Self {
            emulator,
            vte,
            pty,
            pty_rx,
            config,
            theme,
            session_name,
            window_index: 0,
            pane_index: 0,
            mode: InputMode::Terminal,
            hostname,
            should_quit: false,
        })
    }

    fn write_to_pty(&mut self, data: &[u8]) -> Result<()> {
        self.pty.write(data).map(|_| ())
    }

    fn resize(&mut self, cols: u16, full_rows: u16) {
        let status_rows: u16 = if self.config.status_bar.enabled { 1 } else { 0 };
        let term_rows = full_rows.saturating_sub(status_rows);
        self.emulator.resize(TerminalSize {
            cols,
            rows: term_rows,
        });
        let _ = self.pty.resize(term_rows, cols);
    }

    /// Drain all pending PTY data into the emulator
    fn drain_pty(&mut self) {
        loop {
            match self.pty_rx.try_recv() {
                Ok(PtyMsg::Data(data)) => {
                    self.vte.process(&mut self.emulator, &data);
                }
                Ok(PtyMsg::Exit) => {
                    self.should_quit = true;
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.should_quit = true;
                    break;
                }
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        match self.mode {
            InputMode::Prefix => self.handle_prefix_key(key),
            InputMode::Terminal => self.handle_terminal_key(key),
        }
    }

    fn handle_terminal_key(&mut self, key: KeyEvent) -> Result<()> {
        // Ctrl+Space activates prefix mode
        if key.code == KeyCode::Char(' ') && key.modifiers == KeyModifiers::CONTROL {
            self.mode = InputMode::Prefix;
            return Ok(());
        }
        let bytes = encode_key(key);
        if !bytes.is_empty() {
            self.write_to_pty(&bytes)?;
        }
        Ok(())
    }

    fn handle_prefix_key(&mut self, key: KeyEvent) -> Result<()> {
        // Return to normal mode after any prefix key
        self.mode = InputMode::Terminal;
        match key.code {
            KeyCode::Char('n') => {
                self.window_index = self.window_index.wrapping_add(1);
            }
            KeyCode::Char('p') => {
                self.window_index = self.window_index.saturating_sub(1);
            }
            KeyCode::Char('d') | KeyCode::Char('q') => {
                self.should_quit = true;
            }
            // Send literal Ctrl+Space when prefix is pressed twice
            KeyCode::Char(' ') => {
                self.write_to_pty(&[0x00])?;
            }
            _ => {}
        }
        Ok(())
    }
}

/// Encode a crossterm `KeyEvent` to the byte sequence sent over the PTY
fn encode_key(key: KeyEvent) -> Vec<u8> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    let mut bytes: Vec<u8> = match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                let byte = (c as u8).to_ascii_lowercase();
                if (b'a'..=b'z').contains(&byte) {
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

    // Alt prefix: prepend ESC
    if alt && !bytes.is_empty() {
        let mut alt_bytes = vec![0x1B];
        alt_bytes.extend_from_slice(&bytes);
        bytes = alt_bytes;
    }

    bytes
}

/// Widget that renders the terminal emulator grid into a ratatui Buffer
struct TerminalGrid<'a> {
    emulator: &'a TerminalEmulator,
    theme: &'a ThemePalette,
}

impl<'a> Widget for TerminalGrid<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let grid = self.emulator.grid();
        let cursor = self.emulator.cursor();
        let size = self.emulator.size();

        let (dfr, dfg, dfb) = self.theme.fg_rgb();
        let (dbr, dbg, dbb) = self.theme.bg_rgb();
        let default_fg = Color::Rgb(dfr, dfg, dfb);
        let default_bg = Color::Rgb(dbr, dbg, dbb);

        for row in 0..area.height {
            for col in 0..area.width {
                let x = area.x + col;
                let y = area.y + row;

                // Out-of-bounds terminal cells → render as blank with theme background
                if row >= size.rows || col >= size.cols {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_symbol(" ")
                            .set_fg(default_fg)
                            .set_bg(default_bg);
                    }
                    continue;
                }

                let cell = grid.get(row, col).cloned().unwrap_or_default();

                let is_cursor = cursor.row == row
                    && cursor.col == col
                    && self.emulator.cursor_visible();

                let mut fg =
                    resolve_color(cell.attrs.fg, true, self.theme, default_fg, default_bg);
                let mut bg =
                    resolve_color(cell.attrs.bg, false, self.theme, default_fg, default_bg);

                // Reverse video attribute swaps fg/bg
                if cell.attrs.reverse {
                    std::mem::swap(&mut fg, &mut bg);
                }

                // Cursor: invert the cell colors
                if is_cursor {
                    std::mem::swap(&mut fg, &mut bg);
                }

                let mut modifier = Modifier::empty();
                if cell.attrs.bold {
                    modifier |= Modifier::BOLD;
                }
                if cell.attrs.italic {
                    modifier |= Modifier::ITALIC;
                }
                if cell.attrs.underline {
                    modifier |= Modifier::UNDERLINED;
                }
                if cell.attrs.blink {
                    modifier |= Modifier::SLOW_BLINK;
                }
                if cell.attrs.hidden {
                    modifier |= Modifier::HIDDEN;
                }
                if cell.attrs.strikethrough {
                    modifier |= Modifier::CROSSED_OUT;
                }

                let display_char = if cell.char == '\0' { ' ' } else { cell.char };
                let mut sym_buf = [0u8; 4];
                let sym = display_char.encode_utf8(&mut sym_buf);

                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol(sym)
                        .set_fg(fg)
                        .set_bg(bg)
                        .set_style(Style::default().add_modifier(modifier));
                }
            }
        }
    }
}

/// Resolve a `TermColor` to a ratatui `Color`, using the theme palette for ANSI indices 0-15
fn resolve_color(
    color: TermColor,
    is_fg: bool,
    theme: &ThemePalette,
    default_fg: Color,
    default_bg: Color,
) -> Color {
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
            Color::Rgb(r, g, b)
        }
        TermColor::Indexed(n) => Color::Indexed(n),
        TermColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// Responsive status bar widget with 7 breakpoints
struct StatusBar<'a> {
    theme: &'a ThemePalette,
    session_name: &'a str,
    window_index: usize,
    pane_index: usize,
    hostname: &'a str,
    mode: InputMode,
    pane_title: &'a str,
    git_branch: Option<&'a str>,
}

impl<'a> Widget for StatusBar<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (bg_r, bg_g, bg_b) = self.theme.ansi_color(8); // bright_black as bar background
        let (fg_r, fg_g, fg_b) = self.theme.fg_rgb();
        let bar_bg = Color::Rgb(bg_r, bg_g, bg_b);
        let bar_fg = Color::Rgb(fg_r, fg_g, fg_b);

        // Mode badge uses a distinctive accent color
        let mode_bg = match self.mode {
            InputMode::Terminal => {
                let (r, g, b) = self.theme.ansi_color(4);
                Color::Rgb(r, g, b)
            }
            InputMode::Prefix => {
                let (r, g, b) = self.theme.ansi_color(3);
                Color::Rgb(r, g, b)
            }
        };
        let (mfr, mfg, mfb) = self.theme.bg_rgb();
        let mode_fg = Color::Rgb(mfr, mfg, mfb);

        let mode_str = match self.mode {
            InputMode::Terminal => "TERM",
            InputMode::Prefix => "WAIT",
        };

        let width = area.width as usize;
        let (left, right) = self.build_segments(width, mode_str);

        // Fill row with bar background
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut((x, area.y)) {
                cell.set_symbol(" ")
                    .set_fg(bar_fg)
                    .set_bg(bar_bg);
            }
        }

        // Render mode badge on the left
        let mode_display = format!(" {} ", mode_str);
        let mut x = area.x;
        for c in mode_display.chars() {
            if x >= area.x + area.width {
                break;
            }
            let mut s = [0u8; 4];
            if let Some(cell) = buf.cell_mut((x, area.y)) {
                cell.set_symbol(c.encode_utf8(&mut s))
                    .set_fg(mode_fg)
                    .set_bg(mode_bg)
                    .set_style(Style::default().add_modifier(Modifier::BOLD));
            }
            x += 1;
        }

        // Separator space after badge
        if x < area.x + area.width && !left.is_empty() {
            if let Some(cell) = buf.cell_mut((x, area.y)) {
                cell.set_symbol(" ")
                    .set_fg(bar_fg)
                    .set_bg(bar_bg);
            }
            x += 1;
        }

        // Left content
        for c in left.chars() {
            if x >= area.x + area.width {
                break;
            }
            let mut s = [0u8; 4];
            if let Some(cell) = buf.cell_mut((x, area.y)) {
                cell.set_symbol(c.encode_utf8(&mut s))
                    .set_fg(bar_fg)
                    .set_bg(bar_bg);
            }
            x += 1;
        }

        // Right content (right-aligned)
        if !right.is_empty() {
            let right_display = format!(" {} ", right);
            let right_len = right_display.chars().count() as u16;
            let right_start = (area.x + area.width).saturating_sub(right_len);
            let mut rx = right_start;
            for c in right_display.chars() {
                if rx >= area.x + area.width || rx < x {
                    break;
                }
                let mut s = [0u8; 4];
                if let Some(cell) = buf.cell_mut((rx, area.y)) {
                    cell.set_symbol(c.encode_utf8(&mut s))
                        .set_fg(bar_fg)
                        .set_bg(bar_bg);
                }
                rx += 1;
            }
        }
    }
}

impl<'a> StatusBar<'a> {
    fn build_segments(&self, width: usize, _mode_str: &str) -> (String, String) {
        match width {
            // nano (<60): only mode badge visible, no text segments
            w if w < 60 => (String::new(), String::new()),

            // tiny (60-79): session name truncated, no right segment
            w if w < 80 => {
                let avail = w.saturating_sub(10);
                (truncate(self.session_name, avail).to_string(), String::new())
            }

            // small (80-119): session:win, HH:MM
            w if w < 120 => {
                let left = format!("{}:{}", self.session_name, self.window_index);
                (left, current_time_hhmm())
            }

            // medium (120-159): session win:pane, HH:MM
            w if w < 160 => {
                let left = format!(
                    "{}  {}:{}",
                    self.session_name, self.window_index, self.pane_index
                );
                (left, current_time_hhmm())
            }

            // large (160-199): + pane title, HH:MM:SS
            w if w < 200 => {
                let mut left = format!(
                    "{}  {}:{}",
                    self.session_name, self.window_index, self.pane_index
                );
                if !self.pane_title.is_empty() {
                    left.push_str(&format!("  {}", self.pane_title));
                }
                (left, current_time_hhmmss())
            }

            // xlarge (200-239): + hostname
            w if w < 240 => {
                let mut left = format!(
                    "{}  {}:{}",
                    self.session_name, self.window_index, self.pane_index
                );
                if !self.pane_title.is_empty() {
                    left.push_str(&format!("  {}", self.pane_title));
                }
                let right = format!("{}  {}", self.hostname, current_time_hhmmss());
                (left, right)
            }

            // xxlarge (≥240): + git branch
            _ => {
                let mut left = format!(
                    "{}  {}:{}",
                    self.session_name, self.window_index, self.pane_index
                );
                if !self.pane_title.is_empty() {
                    left.push_str(&format!("  {}", self.pane_title));
                }
                if let Some(branch) = self.git_branch {
                    left.push_str(&format!("   {}", branch));
                }
                let right = format!("{}  {}", self.hostname, current_time_hhmmss());
                (left, right)
            }
        }
    }
}

fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        None => s,
        Some((idx, _)) => &s[..idx],
    }
}

fn get_hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| {
            std::fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string())
        })
        .unwrap_or_else(|_| "localhost".to_string())
}

fn current_time_hhmm() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let tod = secs % 86400;
    format!("{:02}:{:02}", tod / 3600, (tod % 3600) / 60)
}

fn current_time_hhmmss() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let tod = secs % 86400;
    format!("{:02}:{:02}:{:02}", tod / 3600, (tod % 3600) / 60, tod % 60)
}

/// Read the current git branch from `.git/HEAD` in the given directory tree
fn detect_git_branch(dir: &Path) -> Option<String> {
    // Walk up to find a .git directory
    let mut current = dir.to_path_buf();
    loop {
        let head = current.join(".git").join("HEAD");
        if let Ok(content) = std::fs::read_to_string(&head) {
            let content = content.trim();
            return if let Some(branch) = content.strip_prefix("ref: refs/heads/") {
                Some(format!(" {}", branch.trim()))
            } else if content.len() >= 7 {
                Some(format!(" {}", &content[..7]))
            } else {
                None
            };
        }
        if !current.pop() {
            break;
        }
    }
    None
}

/// Run the TUI terminal
pub fn run(config: &Config, _command: &Option<String>, directory: Option<&Path>) -> Result<()> {
    tracing::info!("Starting TUI mode");

    let theme_name = crate::config::ThemeCatalog::resolve_theme_name(
        &config.theme.name,
        config.theme.mode,
    );
    let theme = crate::assets::load_theme(&theme_name)
        .unwrap_or_else(|_| crate::config::ThemePalette::default());

    enable_raw_mode().map_err(|e| CastermError::Tui(e.to_string()))?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen).map_err(|e| CastermError::Tui(e.to_string()))?;
    let backend = CrosstermBackend::new(out);
    let mut ratatui_term =
        Terminal::new(backend).map_err(|e| CastermError::Tui(e.to_string()))?;
    ratatui_term
        .hide_cursor()
        .map_err(|e| CastermError::Tui(e.to_string()))?;

    let result = run_app(&mut ratatui_term, config.clone(), theme, directory);

    disable_raw_mode().map_err(|e| CastermError::Tui(e.to_string()))?;
    execute!(ratatui_term.backend_mut(), LeaveAlternateScreen)
        .map_err(|e| CastermError::Tui(e.to_string()))?;
    ratatui_term
        .show_cursor()
        .map_err(|e| CastermError::Tui(e.to_string()))?;

    result
}

fn run_app<B: Backend>(
    ratatui_term: &mut Terminal<B>,
    config: Config,
    theme: ThemePalette,
    directory: Option<&Path>,
) -> Result<()> {
    let mut app = TuiApp::new(config, theme, "main".to_string())?;

    let cwd = directory
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let git_branch = detect_git_branch(&cwd);

    loop {
        app.drain_pty();

        if app.should_quit {
            break;
        }

        let full_size = ratatui_term
            .size()
            .map_err(|e| CastermError::Tui(e.to_string()))?;
        let full_area = Rect::new(0, 0, full_size.width, full_size.height);

        let (term_area, status_area) = if app.config.status_bar.enabled {
            match app.config.status_bar.position {
                StatusBarPosition::Bottom => {
                    let ta = Rect::new(
                        full_area.x,
                        full_area.y,
                        full_area.width,
                        full_area.height.saturating_sub(1),
                    );
                    let sa = Rect::new(
                        full_area.x,
                        full_area.y + ta.height,
                        full_area.width,
                        1,
                    );
                    (ta, Some(sa))
                }
                StatusBarPosition::Top => {
                    let sa = Rect::new(full_area.x, full_area.y, full_area.width, 1);
                    let ta = Rect::new(
                        full_area.x,
                        full_area.y + 1,
                        full_area.width,
                        full_area.height.saturating_sub(1),
                    );
                    (ta, Some(sa))
                }
            }
        } else {
            (full_area, None)
        };

        // Keep emulator size in sync with the terminal area
        let desired = TerminalSize {
            cols: term_area.width,
            rows: term_area.height,
        };
        if app.emulator.size() != desired {
            app.resize(full_area.width, full_area.height);
        }

        let pane_title = app.emulator.title().to_string();
        let win_idx = app.window_index;
        let pane_idx = app.pane_index;
        let mode = app.mode;
        let branch = git_branch.as_deref();

        ratatui_term
            .draw(|frame| {
                frame.render_widget(
                    TerminalGrid {
                        emulator: &app.emulator,
                        theme: &app.theme,
                    },
                    term_area,
                );

                if let Some(sa) = status_area {
                    frame.render_widget(
                        StatusBar {
                            theme: &app.theme,
                            session_name: &app.session_name,
                            window_index: win_idx,
                            pane_index: pane_idx,
                            hostname: &app.hostname,
                            mode,
                            pane_title: &pane_title,
                            git_branch: branch,
                        },
                        sa,
                    );
                }
            })
            .map_err(|e| CastermError::Tui(e.to_string()))?;

        // 16 ms poll ≈ 60 fps; draining the channel keeps latency low
        if event::poll(Duration::from_millis(16))
            .map_err(|e| CastermError::Tui(e.to_string()))?
        {
            match event::read().map_err(|e| CastermError::Tui(e.to_string()))? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    app.handle_key(key)?;
                }
                Event::Resize(cols, rows) => {
                    app.resize(cols, rows);
                }
                _ => {}
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
