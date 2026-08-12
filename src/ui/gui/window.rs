//! winit `ApplicationHandler` driving one GUI terminal window: PTY I/O,
//! keyboard/mouse input, and per-frame rendering via `super::renderer`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowId};

use crate::app::keybindings::{KeymapResolver, Resolved};
use crate::app::pane_runtime::{spawn_pane_runtime, PaneRuntime, PtyMsg};
use crate::app::terminal::TerminalSize;
use crate::config::{Config, ThemePalette};
use crate::support::error::{CastermError, Result};
use crate::ui::render_model::{encode_key, resolve_grid, Rgb};

use super::input::{to_key_code, to_key_modifiers};
use super::renderer::{Renderer, Selection};

/// Redraw/PTY-poll cadence — matches the TUI event loop's own 16ms poll
/// interval (roughly 60Hz) so PTY output feels equally responsive in both
/// front ends.
const POLL_INTERVAL: Duration = Duration::from_millis(16);

/// Converts a font point size (as configured) to a rasterization pixel size
/// assuming a 96 DPI baseline — the same assumption ratatui-less GUI
/// toolkits commonly make when no per-monitor DPI query is wired up yet
/// (see `TODO.AI.md` for HiDPI scale-factor follow-up).
fn font_px(config: &Config) -> f32 {
    (config.font.size * 96.0 / 72.0).max(6.0)
}

struct WindowState {
    window: Arc<Window>,
    renderer: Renderer,
    pane: PaneRuntime,
    keymap: KeymapResolver,
    theme: ThemePalette,
    modifiers: ModifiersState,
    cursor_pos: PhysicalPosition<f64>,
    mouse_down_cell: Option<(u16, u16)>,
    selection: Option<Selection>,
    last_title: String,
}

impl WindowState {
    fn pixel_to_cell(&self, pos: PhysicalPosition<f64>) -> (u16, u16) {
        let (cell_w, cell_h) = self.renderer.cell_size();
        let size = self.pane.emulator.size();
        let col = ((pos.x as f32 / cell_w).floor().max(0.0)) as u16;
        let row = ((pos.y as f32 / cell_h).floor().max(0.0)) as u16;
        (
            row.min(size.rows.saturating_sub(1)),
            col.min(size.cols.saturating_sub(1)),
        )
    }

    fn selection_text(&self, sel: Selection) -> String {
        let (start, end) =
            if sel.start.0 < sel.end.0 || (sel.start.0 == sel.end.0 && sel.start.1 <= sel.end.1) {
                (sel.start, sel.end)
            } else {
                (sel.end, sel.start)
            };
        let grid = self.pane.emulator.grid();
        let size = self.pane.emulator.size();
        let mut lines = Vec::new();
        for row in start.0..=end.0 {
            let col_start = if row == start.0 { start.1 } else { 0 };
            let col_end = if row == end.0 {
                end.1
            } else {
                size.cols.saturating_sub(1)
            };
            let mut line = String::new();
            for col in col_start..=col_end {
                let ch = grid
                    .get(row, col)
                    .map(|c| if c.char == '\0' { ' ' } else { c.char })
                    .unwrap_or(' ');
                line.push(ch);
            }
            lines.push(line.trim_end().to_string());
        }
        lines.join("\n")
    }

    /// Resolve one key press through the shared keymap. Returns `Ok(true)`
    /// when the resolved action means "close this window" — the GUI's MVP
    /// is single-pane/single-window (see `TODO.AI.md` for multi-pane
    /// splits) so `quit`/`detach` are the only actions meaningful here;
    /// anything else resolvable by the shared keymap is accepted but a
    /// no-op.
    fn dispatch_key(&mut self, code: crossterm::event::KeyCode) -> Result<bool> {
        let mid_sequence = self.keymap.awaiting_input();
        let mods = to_key_modifiers(self.modifiers);
        match self.keymap.resolve(mods, code) {
            Resolved::Action(action) => {
                if action == "quit" || action == "detach" {
                    self.selection = None;
                    self.mouse_down_cell = None;
                    return Ok(true);
                }
                Ok(false)
            }
            Resolved::Pending => Ok(false),
            Resolved::NoMatch if mid_sequence => Ok(false),
            Resolved::NoMatch => {
                let bytes = encode_key(crossterm::event::KeyEvent::new(code, mods));
                if !bytes.is_empty() {
                    self.pane.backend.write(&bytes)?;
                }
                Ok(false)
            }
        }
    }

    fn drain_pty(&mut self) -> bool {
        while let Some(msg) = self.pane.backend.try_recv() {
            match msg {
                PtyMsg::Data(data) => {
                    self.pane.vte.process(&mut self.pane.emulator, &data);
                }
                PtyMsg::Exit => return true,
            }
        }
        false
    }

    fn redraw(&mut self) -> Result<()> {
        let title = self.pane.emulator.title();
        if title != self.last_title {
            self.last_title = title.to_string();
            self.window
                .set_title(&format!("casterm — {}", self.last_title));
        }

        let cells = resolve_grid(&self.pane.emulator, &self.theme);
        let size = self.pane.emulator.size();
        let (bgr, bgg, bgb) = self.theme.bg_rgb();
        let (sbr, sbg, sbb) = self.theme.selection_bg_rgb();
        let (sfr, sfg, sfb) = self.theme.selection_fg_rgb();
        self.renderer.render(
            &cells,
            size.cols,
            size.rows,
            Rgb(bgr, bgg, bgb),
            self.selection,
            Rgb(sbr, sbg, sbb),
            Rgb(sfr, sfg, sfb),
        )
    }

    fn handle_resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(width, height);
        let (cell_w, cell_h) = self.renderer.cell_size();
        let cols = ((width as f32 / cell_w).floor().max(1.0)) as u16;
        let rows = ((height as f32 / cell_h).floor().max(1.0)) as u16;
        let current = self.pane.emulator.size();
        if cols != current.cols || rows != current.rows {
            self.pane.emulator.resize(TerminalSize { cols, rows });
            let _ = self.pane.backend.resize(rows, cols);
        }
    }
}

/// Top-level `ApplicationHandler`: owns the config/theme needed to (re)open
/// the window on `resumed` and carries any fatal setup error back out to
/// `gui::run` once the event loop exits.
pub struct GuiApp {
    config: Config,
    directory: Option<PathBuf>,
    theme: ThemePalette,
    state: Option<WindowState>,
    error: Option<CastermError>,
}

impl GuiApp {
    pub fn new(config: Config, theme: ThemePalette, directory: Option<PathBuf>) -> Self {
        Self {
            config,
            directory,
            theme,
            state: None,
            error: None,
        }
    }

    /// Consume the app after the event loop returns, surfacing any fatal
    /// setup/runtime error it recorded.
    pub fn into_result(self) -> Result<()> {
        match self.error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, err: CastermError) {
        tracing::error!("GUI error: {err}");
        self.error = Some(err);
        event_loop.exit();
    }
}

impl ApplicationHandler for GuiApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let attrs = Window::default_attributes()
            .with_title("casterm")
            .with_inner_size(winit::dpi::LogicalSize::new(1000.0, 650.0));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => return self.fail(event_loop, CastermError::Gui(e.to_string())),
        };

        let Some(font) = super::font::load_font() else {
            return self.fail(
                event_loop,
                CastermError::Gui(
                    "no monospace font found on this system; set CASTERM_GUI_FONT_PATH".to_string(),
                ),
            );
        };

        let px = font_px(&self.config);
        let renderer = match Renderer::new(window.clone(), font, px) {
            Ok(r) => r,
            Err(e) => return self.fail(event_loop, e),
        };

        let (cell_w, cell_h) = renderer.cell_size();
        let size = window.inner_size();
        let cols = ((size.width as f32 / cell_w).floor().max(1.0)) as u16;
        let rows = ((size.height as f32 / cell_h).floor().max(1.0)) as u16;

        let pane = match spawn_pane_runtime(
            &self.config,
            TerminalSize { cols, rows },
            self.directory.clone(),
        ) {
            Ok(p) => p,
            Err(e) => return self.fail(event_loop, e),
        };

        let keymap = KeymapResolver::new(&self.config.keybindings);

        self.state = Some(WindowState {
            window,
            renderer,
            pane,
            keymap,
            theme: self.theme.clone(),
            modifiers: ModifiersState::empty(),
            cursor_pos: PhysicalPosition::new(0.0, 0.0),
            mouse_down_cell: None,
            selection: None,
            last_title: String::new(),
        });

        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + POLL_INTERVAL));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        if state.window.id() != window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => state.handle_resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                if let Err(e) = state.redraw() {
                    self.fail(event_loop, e);
                }
            }
            WindowEvent::ModifiersChanged(mods) => {
                state.modifiers = mods.state();
            }
            WindowEvent::KeyboardInput { event: key, .. } => {
                if key.state == ElementState::Pressed {
                    if let Some(code) = to_key_code(&key.logical_key) {
                        match state.dispatch_key(code) {
                            Ok(true) => event_loop.exit(),
                            Ok(false) => {}
                            Err(e) => self.fail(event_loop, e),
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                state.cursor_pos = position;
                if let Some(start) = state.mouse_down_cell {
                    let cell = state.pixel_to_cell(position);
                    state.selection = Some(Selection { start, end: cell });
                }
            }
            WindowEvent::MouseInput {
                state: btn_state,
                button: MouseButton::Left,
                ..
            } => match btn_state {
                ElementState::Pressed => {
                    let cell = state.pixel_to_cell(state.cursor_pos);
                    state.mouse_down_cell = Some(cell);
                    state.selection = Some(Selection {
                        start: cell,
                        end: cell,
                    });
                }
                ElementState::Released => {
                    state.mouse_down_cell = None;
                    if let Some(sel) = state.selection {
                        if sel.start != sel.end {
                            let text = state.selection_text(sel);
                            if !text.is_empty() {
                                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                    let _ = clipboard.set_text(text);
                                }
                            }
                        }
                    }
                }
            },
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        if state.drain_pty() {
            event_loop.exit();
            return;
        }
        state.window.request_redraw();
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + POLL_INTERVAL));
    }
}
