//! TUI mode using ratatui + crossterm with full PTY-backed terminal emulation

use std::collections::HashMap;
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
        LeaveAlternateScreen, SetTitle,
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

use crate::app::keybindings::{KeymapResolver, Resolved};
use crate::app::multiplexer::{Layout, PaneId, SplitDirection, Window};
use crate::app::pty::{Pty, PtyConfig};
use crate::app::terminal::{TermColor, Terminal as TerminalEmulator, TerminalSize};
use crate::app::vte_processor::VteProcessor;
use crate::config::{Config, StatusBarPosition, ThemePalette};
use crate::support::error::{CastermError, Result};

/// Messages from a pane's PTY reader thread
enum PtyMsg {
    Data(Vec<u8>),
    Exit,
}

/// The live state backing a single pane: its PTY, background reader
/// channel, and its own terminal emulator/VTE parser. Each pane runs an
/// independent shell — splitting a window multiplies this, it doesn't
/// share one PTY across panes.
struct PaneRuntime {
    pty: Pty,
    pty_rx: mpsc::Receiver<PtyMsg>,
    emulator: TerminalEmulator,
    vte: VteProcessor,
}

/// Spawn a shell PTY plus its background reader thread and terminal
/// emulator for one pane.
fn spawn_pane_runtime(config: &Config, size: TerminalSize) -> Result<PaneRuntime> {
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

    let mut pty_config = PtyConfig {
        shell,
        rows: size.rows,
        cols: size.cols,
        ..Default::default()
    };
    // Advertise true-color support so shells and editors use it. Prefer
    // casterm's own embedded terminfo entry (extracted to a per-user
    // cache dir, never installed system-wide); fall back to the
    // universally-available xterm-256color identity if it's missing.
    match crate::support::terminfo::install() {
        Some(terminfo_dir) => {
            pty_config.env.push((
                "TERM".to_string(),
                crate::support::terminfo::TERM_NAME.to_string(),
            ));
            pty_config
                .env
                .push(("TERMINFO".to_string(), terminfo_dir.display().to_string()));
        }
        None => {
            pty_config
                .env
                .push(("TERM".to_string(), "xterm-256color".to_string()));
        }
    }
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

    let emulator = TerminalEmulator::new(size);
    let vte = VteProcessor::new();

    Ok(PaneRuntime {
        pty,
        pty_rx,
        emulator,
        vte,
    })
}

/// Full TUI application state
struct TuiApp {
    window: Window,
    panes: HashMap<PaneId, PaneRuntime>,
    config: Config,
    theme: ThemePalette,
    session_name: String,
    keymap: KeymapResolver,
    hostname: String,
    should_quit: bool,
}

impl TuiApp {
    fn new(config: Config, theme: ThemePalette, session_name: String) -> Result<Self> {
        let (cols, rows) = terminal_size().map_err(|e| CastermError::Tui(e.to_string()))?;
        // Reserve one row for the status bar when enabled
        let status_rows: u16 = if config.status_bar.enabled { 1 } else { 0 };
        let term_rows = rows.saturating_sub(status_rows);
        let size = TerminalSize {
            cols,
            rows: term_rows,
        };

        let mut window = Window::new("main");
        window.set_name(session_name.clone());
        let pane_id = window.create_pane();
        let runtime = spawn_pane_runtime(&config, size)?;
        let mut panes = HashMap::new();
        panes.insert(pane_id, runtime);

        let hostname = get_hostname();
        let keymap = KeymapResolver::new(&config.keybindings);

        Ok(Self {
            window,
            panes,
            config,
            theme,
            session_name,
            keymap,
            hostname,
            should_quit: false,
        })
    }

    fn active_pane_id(&self) -> Option<PaneId> {
        self.window.active_pane()
    }

    fn active_pane_mut(&mut self) -> Option<&mut PaneRuntime> {
        let id = self.window.active_pane()?;
        self.panes.get_mut(&id)
    }

    fn write_to_pty(&mut self, data: &[u8]) -> Result<()> {
        if let Some(pane) = self.active_pane_mut() {
            pane.pty.write(data).map(|_| ())?;
        }
        Ok(())
    }

    /// Resize every pane to the rect the current layout tree assigns it.
    fn resize_panes(&mut self, term_area: Rect) {
        for (id, rect) in layout_rects(self.window.layout(), term_area) {
            let size = pane_inner_size(rect, self.window.pane_count());
            if let Some(pane) = self.panes.get_mut(&id) {
                if pane.emulator.size() != size && size.cols > 0 && size.rows > 0 {
                    pane.emulator.resize(size);
                    let _ = pane.pty.resize(size.rows, size.cols);
                }
            }
        }
    }

    /// Drain all pending PTY data into each pane's emulator; panes whose
    /// shell exited are removed from the window.
    fn drain_pty(&mut self) {
        let mut exited = Vec::new();
        for (id, pane) in self.panes.iter_mut() {
            loop {
                match pane.pty_rx.try_recv() {
                    Ok(PtyMsg::Data(data)) => {
                        pane.vte.process(&mut pane.emulator, &data);
                    }
                    Ok(PtyMsg::Exit) => {
                        exited.push(*id);
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        exited.push(*id);
                        break;
                    }
                }
            }
        }
        for id in exited {
            self.close_pane(id);
        }

        // Mirror each pane's OSC-reported terminal title into the window's
        // own pane model, so future window-list/status UI can read it back
        // from `Window` without reaching into TUI-runtime internals.
        for (id, pane) in self.panes.iter() {
            let title = pane.emulator.title();
            let needs_update = self
                .window
                .get_pane(*id)
                .is_some_and(|p| p.title() != title);
            if needs_update {
                if let Some(model_pane) = self.window.get_pane_mut(*id) {
                    model_pane.set_title(title);
                }
            }
        }
    }

    fn close_pane(&mut self, id: PaneId) {
        self.panes.remove(&id);
        self.window.remove_pane(id);
        if self.window.pane_count() == 0 {
            self.should_quit = true;
        }
    }

    fn split(&mut self, direction: SplitDirection) -> Result<()> {
        let Some(active) = self.active_pane_id() else {
            return Ok(());
        };
        let size = self
            .panes
            .get(&active)
            .map(|p| p.emulator.size())
            .unwrap_or(TerminalSize { cols: 80, rows: 24 });
        if let Some(new_id) = self.window.split_pane(active, direction) {
            let runtime = spawn_pane_runtime(&self.config, size)?;
            self.panes.insert(new_id, runtime);
            self.window.set_active_pane(new_id);
        }
        Ok(())
    }

    fn focus_next_pane(&mut self) {
        let mut ids: Vec<PaneId> = self.window.pane_ids().collect();
        if ids.len() < 2 {
            return;
        }
        ids.sort_by_key(|id| id.to_string());
        let Some(current) = self.active_pane_id() else {
            return;
        };
        let idx = ids.iter().position(|&id| id == current).unwrap_or(0);
        let next = ids[(idx + 1) % ids.len()];
        self.window.set_active_pane(next);
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        // A sequence already in progress (or locked mode) means an
        // unmatched key must be swallowed rather than sent to the PTY —
        // mirrors tmux's "prefix eats the next key" behavior.
        let mid_sequence = self.keymap.awaiting_input();
        match self.keymap.resolve(key.modifiers, key.code) {
            Resolved::Action(action) => self.dispatch_action(&action),
            Resolved::Pending => Ok(()),
            Resolved::NoMatch if mid_sequence => Ok(()),
            Resolved::NoMatch => {
                let bytes = encode_key(key);
                if !bytes.is_empty() {
                    self.write_to_pty(&bytes)?;
                }
                Ok(())
            }
        }
    }

    /// Interpret a resolved keybinding action name. Actions not backed by a
    /// subsystem yet (copy mode, multiple windows, ...) are accepted but
    /// currently no-ops — see PART 3+/6 of the stub-subsystem plan.
    fn dispatch_action(&mut self, action: &str) -> Result<()> {
        match action {
            "split-horizontal" => self.split(SplitDirection::Horizontal),
            "split-vertical" => self.split(SplitDirection::Vertical),
            "close-pane" => {
                if let Some(id) = self.active_pane_id() {
                    self.close_pane(id);
                }
                Ok(())
            }
            "focus-next-pane" => {
                self.focus_next_pane();
                Ok(())
            }
            "detach" | "quit" => {
                self.should_quit = true;
                Ok(())
            }
            "send-literal-prefix" => self.write_to_pty(&[0x00]),
            _ => Ok(()),
        }
    }
}

/// Recursively resolve a pane layout tree into `(PaneId, Rect)` leaves.
/// `SplitDirection::Horizontal` arranges panes side by side (splitting
/// width); `Vertical` stacks them top/bottom (splitting height) — matching
/// tmux's `split-window -h`/`-v` convention.
fn layout_rects(layout: Option<&Layout>, area: Rect) -> Vec<(PaneId, Rect)> {
    let mut out = Vec::new();
    if let Some(layout) = layout {
        collect_layout_rects(layout, area, &mut out);
    }
    out
}

fn collect_layout_rects(layout: &Layout, area: Rect, out: &mut Vec<(PaneId, Rect)>) {
    match layout {
        Layout::Single(id) => out.push((*id, area)),
        Layout::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let (first_area, second_area) = split_rect(area, *direction, *ratio);
            collect_layout_rects(first, first_area, out);
            collect_layout_rects(second, second_area, out);
        }
    }
}

fn split_rect(area: Rect, direction: SplitDirection, ratio: f32) -> (Rect, Rect) {
    match direction {
        SplitDirection::Horizontal => {
            let first_w = ((area.width as f32) * ratio).round() as u16;
            let first = Rect::new(area.x, area.y, first_w, area.height);
            let second = Rect::new(
                area.x + first_w,
                area.y,
                area.width.saturating_sub(first_w),
                area.height,
            );
            (first, second)
        }
        SplitDirection::Vertical => {
            let first_h = ((area.height as f32) * ratio).round() as u16;
            let first = Rect::new(area.x, area.y, area.width, first_h);
            let second = Rect::new(
                area.x,
                area.y + first_h,
                area.width,
                area.height.saturating_sub(first_h),
            );
            (first, second)
        }
    }
}

/// The terminal size a pane should render at, given its screen rect. With
/// more than one pane a 1-cell border is drawn around each, so the usable
/// interior is 2 cells smaller on each axis.
fn pane_inner_size(rect: Rect, pane_count: usize) -> TerminalSize {
    if pane_count > 1 {
        TerminalSize {
            cols: rect.width.saturating_sub(2),
            rows: rect.height.saturating_sub(2),
        }
    } else {
        TerminalSize {
            cols: rect.width,
            rows: rect.height,
        }
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
                        cell.set_symbol(" ").set_fg(default_fg).set_bg(default_bg);
                    }
                    continue;
                }

                let cell = grid.get(row, col).cloned().unwrap_or_default();

                let is_cursor =
                    cursor.row == row && cursor.col == col && self.emulator.cursor_visible();

                let mut fg = resolve_color(cell.attrs.fg, true, self.theme, default_fg, default_bg);
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

/// Status-bar mode badge — derived each frame from the keymap resolver's
/// state rather than owned by `TuiApp` directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusMode {
    /// Keys go straight to the PTY.
    Terminal,
    /// A multi-chord keybinding sequence is in progress.
    Waiting,
    /// Locked mode: all keys except the unlock sequence are swallowed.
    Locked,
}

/// Responsive status bar widget with 7 breakpoints
struct StatusBar<'a> {
    theme: &'a ThemePalette,
    session_name: &'a str,
    window_index: usize,
    pane_index: usize,
    hostname: &'a str,
    mode: StatusMode,
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
            StatusMode::Terminal => {
                let (r, g, b) = self.theme.ansi_color(4);
                Color::Rgb(r, g, b)
            }
            StatusMode::Waiting => {
                let (r, g, b) = self.theme.ansi_color(3);
                Color::Rgb(r, g, b)
            }
            StatusMode::Locked => {
                let (r, g, b) = self.theme.ansi_color(1);
                Color::Rgb(r, g, b)
            }
        };
        let (mfr, mfg, mfb) = self.theme.bg_rgb();
        let mode_fg = Color::Rgb(mfr, mfg, mfb);

        let mode_str = match self.mode {
            StatusMode::Terminal => "TERM",
            StatusMode::Waiting => "WAIT",
            StatusMode::Locked => "LOCK",
        };

        let width = area.width as usize;
        let (left, right) = self.build_segments(width, mode_str);

        // Fill row with bar background
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut((x, area.y)) {
                cell.set_symbol(" ").set_fg(bar_fg).set_bg(bar_bg);
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
                cell.set_symbol(" ").set_fg(bar_fg).set_bg(bar_bg);
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
            for (rx, c) in (right_start..).zip(right_display.chars()) {
                if rx >= area.x + area.width || rx < x {
                    break;
                }
                let mut s = [0u8; 4];
                if let Some(cell) = buf.cell_mut((rx, area.y)) {
                    cell.set_symbol(c.encode_utf8(&mut s))
                        .set_fg(bar_fg)
                        .set_bg(bar_bg);
                }
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
                (
                    truncate(self.session_name, avail).to_string(),
                    String::new(),
                )
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
        .or_else(|_| std::fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string()))
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

    let theme_name =
        crate::config::ThemeCatalog::resolve_theme_name(&config.theme.name, config.theme.mode);
    let theme = crate::assets::load_theme(&theme_name)
        .unwrap_or_else(|_| crate::config::ThemePalette::default());

    enable_raw_mode().map_err(|e| CastermError::Tui(e.to_string()))?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen).map_err(|e| CastermError::Tui(e.to_string()))?;
    let backend = CrosstermBackend::new(out);
    let mut ratatui_term = Terminal::new(backend).map_err(|e| CastermError::Tui(e.to_string()))?;
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

fn run_app<B: Backend + std::io::Write>(
    ratatui_term: &mut Terminal<B>,
    config: Config,
    theme: ThemePalette,
    directory: Option<&Path>,
) -> Result<()> {
    let mut app = TuiApp::new(config, theme, "main".to_string())?;

    // Propagate the window's own name (and id, for multi-window
    // disambiguation once Phase 3 wires session resurrection) into the
    // host terminal emulator's title bar.
    let _ = execute!(
        ratatui_term.backend_mut(),
        SetTitle(format!(
            "casterm — {} [{}]",
            app.window.name(),
            app.window.id()
        ))
    );

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
                    let sa = Rect::new(full_area.x, full_area.y + ta.height, full_area.width, 1);
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

        // Resize every pane to whatever rect the current split layout
        // assigns it; only the panes whose rect actually changed size
        // touch their PTY/emulator (checked inside resize_panes).
        app.resize_panes(term_area);

        let rects = layout_rects(app.window.layout(), term_area);
        let active_id = app.active_pane_id();
        let pane_title = active_id
            .and_then(|id| app.panes.get(&id))
            .map(|p| p.emulator.title().to_string())
            .unwrap_or_default();
        let mut sorted_ids: Vec<PaneId> = app.window.pane_ids().collect();
        sorted_ids.sort_by_key(|id| id.to_string());
        let pane_idx = active_id
            .and_then(|id| sorted_ids.iter().position(|&p| p == id))
            .unwrap_or(0);
        let mode = if app.keymap.is_locked() {
            StatusMode::Locked
        } else if app.keymap.awaiting_input() {
            StatusMode::Waiting
        } else {
            StatusMode::Terminal
        };
        let branch = git_branch.as_deref();
        let multi_pane = rects.len() > 1;

        ratatui_term
            .draw(|frame| {
                for (id, rect) in &rects {
                    let Some(pane) = app.panes.get(id) else {
                        continue;
                    };
                    let is_active = Some(*id) == active_id;
                    let inner = if multi_pane {
                        let border_color = if is_active {
                            let (r, g, b) = app.theme.ansi_color(4);
                            Color::Rgb(r, g, b)
                        } else {
                            let (r, g, b) = app.theme.ansi_color(8);
                            Color::Rgb(r, g, b)
                        };
                        let block = ratatui::widgets::Block::bordered()
                            .border_style(Style::default().fg(border_color));
                        let inner = block.inner(*rect);
                        frame.render_widget(block, *rect);
                        inner
                    } else {
                        *rect
                    };
                    frame.render_widget(
                        TerminalGrid {
                            emulator: &pane.emulator,
                            theme: &app.theme,
                        },
                        inner,
                    );
                }

                if let Some(sa) = status_area {
                    frame.render_widget(
                        StatusBar {
                            theme: &app.theme,
                            session_name: &app.session_name,
                            window_index: 0,
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
        if event::poll(Duration::from_millis(16)).map_err(|e| CastermError::Tui(e.to_string()))? {
            match event::read().map_err(|e| CastermError::Tui(e.to_string()))? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    app.handle_key(key)?;
                }
                // Resize is picked up next iteration via ratatui_term.size()
                // + resize_panes(), no separate handling needed here.
                _ => {}
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
