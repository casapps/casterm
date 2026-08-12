//! TUI mode using ratatui + crossterm with full PTY-backed terminal emulation

use std::collections::HashMap;
use std::io::{stdout, Read};
use std::path::{Path, PathBuf};
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
use crate::app::serial::{SerialConfig, SerialConnection};
use crate::app::serial_transport::SerialMsg;
use crate::app::session::{Session, SessionId};
use crate::app::ssh::{SshConfig, SshConnection};
use crate::app::ssh_transport::SshMsg;
use crate::app::terminal::{TermColor, Terminal as TerminalEmulator, TerminalSize};
use crate::app::vte_processor::VteProcessor;
use crate::app::App;
use crate::config::{Config, StatusBarPosition, ThemePalette};
use crate::state::{PaneState, WindowState};
use crate::support::error::{CastermError, Result};

/// Messages from a pane's PTY reader thread
enum PtyMsg {
    Data(Vec<u8>),
    Exit,
}

/// A pane's live I/O backend: a local PTY-backed shell, a remote SSH
/// session, or a serial device. All three feed the same `PtyMsg` shape into
/// the pane's terminal emulator/VTE parser, so the render/drain/write/
/// resize paths don't need to know which backend a given pane is running.
enum PaneBackend {
    Local {
        pty: Pty,
        rx: mpsc::Receiver<PtyMsg>,
    },
    Ssh {
        conn: Box<SshConnection>,
    },
    Serial {
        conn: Box<SerialConnection>,
    },
}

impl PaneBackend {
    fn try_recv(&self) -> Option<PtyMsg> {
        match self {
            PaneBackend::Local { rx, .. } => rx.try_recv().ok(),
            PaneBackend::Ssh { conn } => conn.try_recv().map(|msg| match msg {
                SshMsg::Data(data) => PtyMsg::Data(data),
                SshMsg::Exit => PtyMsg::Exit,
            }),
            PaneBackend::Serial { conn } => conn.try_recv().map(|msg| match msg {
                SerialMsg::Data(data) => PtyMsg::Data(data),
                SerialMsg::Disconnected => PtyMsg::Exit,
            }),
        }
    }

    fn write(&mut self, data: &[u8]) -> Result<()> {
        match self {
            PaneBackend::Local { pty, .. } => pty.write(data).map(|_| ()),
            PaneBackend::Ssh { conn } => conn.write(data),
            PaneBackend::Serial { conn } => conn.write(data),
        }
    }

    fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        match self {
            PaneBackend::Local { pty, .. } => pty.resize(rows, cols),
            PaneBackend::Ssh { conn } => conn.resize(cols, rows),
            // Serial devices have no concept of a terminal size — resize
            // is a PTY/SSH-only notion.
            PaneBackend::Serial { .. } => Ok(()),
        }
    }

    /// Gracefully tear down an SSH or serial session before the pane is
    /// dropped (local PTYs don't need this — their child process is killed
    /// by `Pty`'s own `Drop` impl).
    fn disconnect(&mut self) {
        match self {
            PaneBackend::Ssh { conn } => conn.disconnect(),
            PaneBackend::Serial { conn } => conn.disconnect(),
            PaneBackend::Local { .. } => {}
        }
    }
}

/// The live state backing a single pane: its I/O backend and its own
/// terminal emulator/VTE parser. Each pane runs an independent shell or SSH
/// session — splitting a window multiplies this, it doesn't share one
/// backend across panes.
struct PaneRuntime {
    backend: PaneBackend,
    emulator: TerminalEmulator,
    vte: VteProcessor,
    /// The working directory this pane's shell was spawned in — tracked so
    /// session save/restore (`state::PaneState.cwd`) can re-open a restored
    /// pane in the same place. Defaults to the process's own cwd when the
    /// caller doesn't request a specific one. SSH panes report the local
    /// process's own cwd here since they have no local shell directory of
    /// their own.
    cwd: PathBuf,
}

/// Spawn a shell PTY plus its background reader thread and terminal
/// emulator for one pane, starting the shell in `cwd` (falling back to the
/// process's own working directory when `None`).
fn spawn_pane_runtime(
    config: &Config,
    size: TerminalSize,
    cwd: Option<PathBuf>,
) -> Result<PaneRuntime> {
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

    let resolved_cwd = cwd
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let mut pty_config = PtyConfig {
        shell,
        rows: size.rows,
        cols: size.cols,
        cwd,
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
        backend: PaneBackend::Local { pty, rx: pty_rx },
        emulator,
        vte,
        cwd: resolved_cwd,
    })
}

/// Connect an SSH-backed pane and open an interactive remote shell,
/// blocking until the connection either succeeds or fails (same contract as
/// `spawn_pane_runtime` for a local shell).
fn spawn_ssh_pane_runtime(ssh_config: &SshConfig, size: TerminalSize) -> Result<PaneRuntime> {
    let mut conn = SshConnection::new(ssh_config.clone());
    conn.connect(size.cols, size.rows)?;

    let emulator = TerminalEmulator::new(size);
    let vte = VteProcessor::new();

    Ok(PaneRuntime {
        backend: PaneBackend::Ssh {
            conn: Box::new(conn),
        },
        emulator,
        vte,
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    })
}

/// Connect a serial-backed pane and open the device, blocking only for the
/// local open syscall (same contract as `spawn_pane_runtime`/
/// `spawn_ssh_pane_runtime`, but there's no remote handshake to await).
fn spawn_serial_pane_runtime(
    serial_config: &SerialConfig,
    size: TerminalSize,
) -> Result<PaneRuntime> {
    let mut conn = SerialConnection::new(serial_config.clone());
    conn.connect()?;

    let emulator = TerminalEmulator::new(size);
    let vte = VteProcessor::new();

    Ok(PaneRuntime {
        backend: PaneBackend::Serial {
            conn: Box::new(conn),
        },
        emulator,
        vte,
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    })
}

/// Convert a live window + its running panes into a serializable
/// `WindowState`, pairing each pane with the working directory its
/// `PaneRuntime` was spawned in.
fn window_to_state(window: &Window, panes: &HashMap<PaneId, PaneRuntime>) -> WindowState {
    let ordered = window.pane_ids_sorted();
    let index_of: HashMap<PaneId, usize> =
        ordered.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let layout = window
        .layout()
        .map(|l| l.encode(&index_of))
        .unwrap_or_default();
    let panes = ordered
        .iter()
        .enumerate()
        .map(|(index, id)| PaneState {
            index,
            cwd: panes.get(id).map(|p| p.cwd.clone()),
            command: None,
        })
        .collect();
    WindowState {
        name: window.name().to_string(),
        index: 0,
        panes,
        layout,
    }
}

/// Reconstruct a blank `Window` (panes present, no live `PaneRuntime` yet)
/// plus each restored pane's saved `cwd`, from a saved `WindowState`. Falls
/// back to a single-pane layout if the saved layout string is malformed
/// rather than producing a window with no layout at all.
fn window_from_state(state: &WindowState) -> (Window, Vec<Option<PathBuf>>) {
    let mut window = Window::new(state.name.clone());
    let ids = window.restore_panes(state.panes.len());
    let active = ids.first().copied();
    match Layout::decode(&state.layout, &ids) {
        Some(layout) => window.set_layout(layout, active),
        None => {
            if let Some(id) = active {
                window.set_layout(Layout::Single(id), Some(id));
            }
        }
    }
    let cwds = state.panes.iter().map(|p| p.cwd.clone()).collect();
    (window, cwds)
}

/// Bundles the fresh-session startup knobs `TuiApp::new`/`run_app` need, so
/// they take one parameter instead of five separate ones (directory/restore/
/// ssh/serial all only matter for a *fresh* session — see `TuiApp::new`'s
/// doc comment for the full precedence rules).
struct StartupOptions<'a> {
    session_name: String,
    directory: Option<&'a Path>,
    restore: bool,
    ssh: Option<SshConfig>,
    serial: Option<SerialConfig>,
}

/// Full TUI application state
struct TuiApp {
    app: App,
    session_id: SessionId,
    panes: HashMap<PaneId, PaneRuntime>,
    config: Config,
    theme: ThemePalette,
    session_name: String,
    keymap: KeymapResolver,
    hostname: String,
    should_quit: bool,
}

impl TuiApp {
    /// Construct the TUI application, either starting a fresh session or —
    /// when `restore` is true and `config.multiplexer.persist_sessions` is
    /// on — resuming a saved one (`session_name` by exact match, else the
    /// most-recently-attached saved session), re-spawning each restored
    /// pane's shell in its saved `cwd`. `directory` seeds the first pane's
    /// cwd for a fresh session (this is what the `-d`/`--directory` CLI flag
    /// controls). When `ssh` is set and the session is fresh, the starting
    /// pane connects to that remote host instead of spawning a local shell
    /// (this is what the `--ssh` CLI flag controls), or — when `serial` is
    /// set — opens that device instead (the `--serial` CLI flag); restored
    /// sessions always re-spawn local shells since SSH/serial panes aren't
    /// part of session persistence yet.
    fn new(config: Config, theme: ThemePalette, opts: StartupOptions) -> Result<Self> {
        let StartupOptions {
            session_name,
            directory,
            restore,
            ssh,
            serial,
        } = opts;
        let (cols, rows) = terminal_size().map_err(|e| CastermError::Tui(e.to_string()))?;
        // Reserve one row for the status bar when enabled
        let status_rows: u16 = if config.status_bar.enabled { 1 } else { 0 };
        let term_rows = rows.saturating_sub(status_rows);
        let size = TerminalSize {
            cols,
            rows: term_rows,
        };

        let mut app = App::new(config.clone())?;

        let saved_state = if restore && app.config().multiplexer.persist_sessions {
            app.state()
                .load_session(&session_name)
                .cloned()
                .or_else(|| {
                    app.state()
                        .list_sessions()
                        .max_by_key(|s| s.last_attached)
                        .cloned()
                })
        } else {
            None
        };

        let (mut window, pane_cwds, restored_name) =
            match saved_state.as_ref().and_then(|s| s.windows.first()) {
                Some(window_state) => {
                    let (window, cwds) = window_from_state(window_state);
                    (window, cwds, saved_state.as_ref().map(|s| s.name.clone()))
                }
                None => (Window::new(session_name.clone()), Vec::new(), None),
            };

        let effective_name = restored_name.unwrap_or_else(|| session_name.clone());
        window.set_name(effective_name.clone());

        let mut panes = HashMap::new();
        if window.pane_count() == 0 {
            // Fresh session: create the one starting pane, honoring
            // `-d`/`--directory` for its initial working directory, or
            // `--ssh` to connect it to a remote host instead of a local
            // shell.
            let pane_id = window.create_pane();
            let runtime = match (&ssh, &serial) {
                (Some(ssh_config), _) => spawn_ssh_pane_runtime(ssh_config, size)?,
                (None, Some(serial_config)) => spawn_serial_pane_runtime(serial_config, size)?,
                (None, None) => {
                    let cwd = directory.map(Path::to_path_buf);
                    spawn_pane_runtime(&config, size, cwd)?
                }
            };
            panes.insert(pane_id, runtime);
        } else {
            // Restored session: re-spawn a shell for every saved pane in
            // its saved cwd (falling back to `-d`/`--directory`, then the
            // process's own cwd).
            let ordered = window.pane_ids_sorted();
            for (idx, id) in ordered.iter().enumerate() {
                let cwd = pane_cwds
                    .get(idx)
                    .cloned()
                    .flatten()
                    .or_else(|| directory.map(Path::to_path_buf));
                let runtime = spawn_pane_runtime(&config, size, cwd)?;
                panes.insert(*id, runtime);
            }
        }

        let session = Session::with_window(effective_name.clone(), window);
        let session_id = app.sessions_mut().insert(session);

        let hostname = get_hostname();
        let keymap = KeymapResolver::new(&config.keybindings);

        Ok(Self {
            app,
            session_id,
            panes,
            config,
            theme,
            session_name: effective_name,
            keymap,
            hostname,
            should_quit: false,
        })
    }

    /// The live window backing this TUI session. Phase 2 scoped the
    /// multiplexer to one window per session, and `TuiApp::new` always
    /// leaves its just-inserted session as `self.app`'s active one, so
    /// `active()`/`active_mut()` always resolve to it.
    fn window(&self) -> &Window {
        self.app
            .sessions()
            .active()
            .expect("TuiApp's own session is always the active one")
            .window()
    }

    fn window_mut(&mut self) -> &mut Window {
        self.app
            .sessions_mut()
            .active_mut()
            .expect("TuiApp's own session is always the active one")
            .window_mut()
    }

    /// Serialize the live window/pane tree and persist it, when
    /// `config.multiplexer.persist_sessions` is on. Called on clean
    /// shutdown so the next launch can restore it. Also marks the in-memory
    /// session `Detached` (a clean save, distinct from `Dead` when every
    /// pane already exited on its own) and removes it from the app's
    /// session table, since this `TuiApp`'s process is about to exit.
    fn save_session(&mut self) -> Result<()> {
        let already_dead = self
            .app
            .sessions()
            .active()
            .map(|s| s.state() == crate::app::session::SessionState::Dead)
            .unwrap_or(false);

        if !already_dead {
            if let Some(session) = self.app.sessions_mut().active_mut() {
                session.set_state(crate::app::session::SessionState::Detached);
            }
        }

        // A `Dead` session (every pane already exited on its own) has
        // nothing left worth resuming; only persist a clean/`Detached` one,
        // and drop any stale on-disk copy from an earlier save this run.
        let result = if already_dead {
            let _ = self.app.state_mut().remove_session(&self.session_name);
            Ok(())
        } else if self.config.multiplexer.persist_sessions {
            self.persist_session_state()
        } else {
            Ok(())
        };

        self.app.sessions_mut().remove(self.session_id);
        result
    }

    fn persist_session_state(&mut self) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let name = self
            .app
            .sessions()
            .active()
            .map(|s| s.name().to_string())
            .unwrap_or_else(|| self.session_name.clone());
        let window_state = window_to_state(self.window(), &self.panes);
        let state = crate::state::SessionState {
            name,
            created_at: now,
            last_attached: now,
            windows: vec![window_state],
        };
        self.app.state_mut().save_session(state)
    }

    fn active_pane_id(&self) -> Option<PaneId> {
        self.window().active_pane()
    }

    fn active_pane_mut(&mut self) -> Option<&mut PaneRuntime> {
        let id = self.window().active_pane()?;
        self.panes.get_mut(&id)
    }

    fn write_to_pty(&mut self, data: &[u8]) -> Result<()> {
        if let Some(pane) = self.active_pane_mut() {
            pane.backend.write(data)?;
        }
        Ok(())
    }

    /// Resize every pane to the rect the current layout tree assigns it.
    fn resize_panes(&mut self, term_area: Rect) {
        let rects = layout_rects(self.window().layout(), term_area);
        let pane_count = self.window().pane_count();
        for (id, rect) in rects {
            let size = pane_inner_size(rect, pane_count);
            if let Some(pane) = self.panes.get_mut(&id) {
                if pane.emulator.size() != size && size.cols > 0 && size.rows > 0 {
                    pane.emulator.resize(size);
                    let _ = pane.backend.resize(size.rows, size.cols);
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
                match pane.backend.try_recv() {
                    Some(PtyMsg::Data(data)) => {
                        pane.vte.process(&mut pane.emulator, &data);
                    }
                    Some(PtyMsg::Exit) => {
                        exited.push(*id);
                        break;
                    }
                    None => break,
                }
            }
        }
        for id in exited {
            self.close_pane(id);
        }

        // Mirror each pane's OSC-reported terminal title into the window's
        // own pane model, so future window-list/status UI can read it back
        // from `Window` without reaching into TUI-runtime internals. Titles
        // are collected first, then applied, so the read pass (shared
        // borrow of `self.panes`) never overlaps the write pass (exclusive
        // borrow of `self.window_mut()`).
        let mut title_updates: Vec<(PaneId, String)> = Vec::new();
        for (id, pane) in self.panes.iter() {
            let mut title = pane.emulator.title().to_string();
            // Serial panes in hex-dump mode get a visible marker in the
            // title bar, since the raw device stream isn't self-describing
            // the way an SSH/shell prompt is.
            if let PaneBackend::Serial { conn } = &pane.backend {
                if conn.hex_mode() {
                    title = format!("[HEX] {title}");
                }
            }
            let needs_update = self
                .window()
                .get_pane(*id)
                .is_some_and(|p| p.title() != title);
            if needs_update {
                title_updates.push((*id, title));
            }
        }
        for (id, title) in title_updates {
            if let Some(model_pane) = self.window_mut().get_pane_mut(id) {
                model_pane.set_title(title);
            }
        }
    }

    fn close_pane(&mut self, id: PaneId) {
        if let Some(pane) = self.panes.get_mut(&id) {
            pane.backend.disconnect();
        }
        self.panes.remove(&id);
        self.window_mut().remove_pane(id);
        if self.window().pane_count() == 0 {
            self.should_quit = true;
            // Every pane exited on its own (as opposed to a clean quit-key
            // shutdown, which `save_session` marks `Detached`) — there is
            // nothing left to resume, so mark the session `Dead` rather
            // than `Detached`.
            if let Some(session) = self.app.sessions_mut().active_mut() {
                session.set_state(crate::app::session::SessionState::Dead);
            }
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
        if let Some(new_id) = self.window_mut().split_pane(active, direction) {
            let runtime = spawn_pane_runtime(&self.config, size, None)?;
            self.panes.insert(new_id, runtime);
            self.window_mut().set_active_pane(new_id);
        }
        Ok(())
    }

    fn focus_next_pane(&mut self) {
        let mut ids: Vec<PaneId> = self.window().pane_ids().collect();
        if ids.len() < 2 {
            return;
        }
        ids.sort_by_key(|id| id.value());
        let Some(current) = self.active_pane_id() else {
            return;
        };
        let idx = ids.iter().position(|&id| id == current).unwrap_or(0);
        let next = ids[(idx + 1) % ids.len()];
        self.window_mut().set_active_pane(next);
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
pub fn run(
    config: &Config,
    _command: &Option<String>,
    directory: Option<&Path>,
    session: Option<&str>,
    restore: bool,
    ssh: Option<SshConfig>,
    serial: Option<SerialConfig>,
) -> Result<()> {
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

    let session_name = session.unwrap_or("main").to_string();
    let result = run_app(
        &mut ratatui_term,
        config.clone(),
        theme,
        StartupOptions {
            session_name,
            directory,
            restore,
            ssh,
            serial,
        },
    );

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
    opts: StartupOptions,
) -> Result<()> {
    let directory = opts.directory;
    let mut app = TuiApp::new(config, theme, opts)?;

    // Propagate the window's own name (and id, for multi-window
    // disambiguation once real multi-window sessions land) into the host
    // terminal emulator's title bar.
    let _ = execute!(
        ratatui_term.backend_mut(),
        SetTitle(format!(
            "casterm — {} [{}]",
            app.window().name(),
            app.window().id()
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

        let rects = layout_rects(app.window().layout(), term_area);
        let active_id = app.active_pane_id();
        let pane_title = active_id
            .and_then(|id| app.panes.get(&id))
            .map(|p| p.emulator.title().to_string())
            .unwrap_or_default();
        let mut sorted_ids: Vec<PaneId> = app.window().pane_ids().collect();
        sorted_ids.sort_by_key(|id| id.value());
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

    app.save_session()?;

    Ok(())
}
