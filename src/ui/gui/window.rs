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
use crate::config::{Config, FileBrowserPosition, ThemePalette};
use crate::support::error::{CastermError, Result};
use crate::ui::render_model::{cursor_style, encode_key, resolve_grid, Rgb};

use super::input::{to_key_code, to_key_modifiers};
use super::renderer::{EditorPanelView, FileBrowserPanelView, ImagePanelView, Renderer, Selection};

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
    /// `Some` while the local file-browser tree panel is open. See
    /// `.claude/plans/inherited-painting-lark.md`.
    file_browser: Option<crate::app::file_browser::FileBrowserState>,
    file_browser_show_hidden: bool,
    file_browser_width: u16,
    file_browser_position: FileBrowserPosition,
    /// Which panel content is showing while `file_browser` is open — the
    /// tree, or the built-in editor for a selected `FileKind::Text` entry.
    /// See `.claude/plans/inherited-painting-lark.md` phase 5.
    viewer: crate::app::file_browser::ViewerContent,
    /// Transient status message shown in the editor's hint-bar row (e.g.
    /// "Saved" or a save error) in place of the default key hints.
    editor_status: Option<String>,
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
                if action == "toggle-file-browser" {
                    self.file_browser = match self.file_browser.take() {
                        Some(_) => None,
                        None => {
                            let root =
                                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                            Some(crate::app::file_browser::FileBrowserState::new(
                                root,
                                self.file_browser_show_hidden,
                            ))
                        }
                    };
                    if !self.viewer.is_tree() {
                        // Leaving an open editor/image view behind: free the
                        // GPU image texture if one was loaded (no-op
                        // otherwise) before resetting to the tree.
                        self.renderer.set_image(None);
                    }
                    self.viewer = crate::app::file_browser::ViewerContent::Tree;
                    self.editor_status = None;
                    // Toggling doesn't fire a `WindowEvent::Resized`, so the
                    // terminal's cols/rows (which shrink to make room for
                    // the panel) need an explicit recompute here.
                    self.recompute_terminal_grid_from_window();
                }
                Ok(false)
            }
            Resolved::Pending => Ok(false),
            // While the panel is open, any key that didn't resolve to a
            // bound action (notably the toggle key itself, handled above)
            // is swallowed by the panel instead of falling through to PTY
            // passthrough — same "swallow while active" precedent as the
            // TUI's `handle_file_browser_key`/`handle_editor_key`.
            Resolved::NoMatch if self.file_browser.is_some() => {
                match &self.viewer {
                    crate::app::file_browser::ViewerContent::Tree => {
                        self.handle_file_browser_key(code)?;
                    }
                    crate::app::file_browser::ViewerContent::Editor(_) => {
                        self.handle_editor_key(code)?;
                    }
                    crate::app::file_browser::ViewerContent::Image(_) => {
                        self.handle_image_key(code);
                    }
                }
                Ok(false)
            }
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

    /// Handle a key event while the file-browser panel has focus: move the
    /// selection, expand/collapse a directory, or open a file. Mirrors
    /// `ui::tui::mod::TuiApp::handle_file_browser_key` exactly, including
    /// the OS-handoff-for-everything-non-directory behavior of this phase
    /// (the built-in editor lands in Phase 4/5). `Esc` closes the panel.
    fn handle_file_browser_key(&mut self, code: crossterm::event::KeyCode) -> Result<()> {
        use crossterm::event::KeyCode;
        match code {
            KeyCode::Esc => {
                self.file_browser = None;
                self.recompute_terminal_grid_from_window();
                Ok(())
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(browser) = self.file_browser.as_mut() {
                    browser.move_selection(-1);
                }
                Ok(())
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(browser) = self.file_browser.as_mut() {
                    browser.move_selection(1);
                }
                Ok(())
            }
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                self.open_selected_file_browser_entry()
            }
            KeyCode::Char('r') => {
                if let Some(browser) = self.file_browser.as_mut() {
                    browser.refresh();
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Act on the currently-selected file-browser entry: expand/collapse a
    /// directory, switch to the built-in editor for `FileKind::Text`
    /// (Phase 5), switch to the built-in image viewer for `FileKind::Image`
    /// (Phase 6, GUI-only — the TUI stays on OS handoff for images
    /// permanently), or hand everything else off to the OS default
    /// application. Diverges from
    /// `ui::tui::mod::TuiApp::open_selected_file_browser_entry` only in the
    /// `Image` arm, per the plan's deliberate TUI/GUI asymmetry.
    fn open_selected_file_browser_entry(&mut self) -> Result<()> {
        let Some(browser) = self.file_browser.as_mut() else {
            return Ok(());
        };
        let Some(entry) = browser.selected_entry() else {
            return Ok(());
        };
        if entry.is_dir {
            browser.toggle_selected();
            return Ok(());
        }
        let path = entry.path.clone();
        match crate::app::file_browser::classify_path(&path) {
            crate::app::file_browser::FileKind::Directory => Ok(()),
            crate::app::file_browser::FileKind::Text => {
                let editor = crate::app::file_browser::open_for_edit(&path)?;
                self.viewer = crate::app::file_browser::ViewerContent::Editor(editor);
                self.editor_status = None;
                // The editor takes over the full window instead of the
                // narrow tree-panel strip, so the terminal grid's reserved
                // pixel width (and thus its cols/rows) changes here too.
                self.recompute_terminal_grid_from_window();
                Ok(())
            }
            crate::app::file_browser::FileKind::Image => {
                let image = crate::app::file_browser::open_for_view(&path)?;
                self.renderer.set_image(Some(&image));
                self.viewer = crate::app::file_browser::ViewerContent::Image(image);
                // The image viewer takes over the full window, same as the
                // editor, so the terminal grid's reserved pixel width
                // changes here too.
                self.recompute_terminal_grid_from_window();
                Ok(())
            }
            crate::app::file_browser::FileKind::Other => {
                crate::platform::Platform::open_with_default_app(&path)
            }
        }
    }

    /// Handle a key event while the built-in editor has focus. Delegates to
    /// the shared `app::editor::dispatch_editor_key` (same dispatch table
    /// as the TUI's `handle_editor_key`, since both front ends' key events
    /// are `crossterm::event::KeyCode`). The panel's own toggle key closes
    /// the whole panel (handled earlier in `dispatch_key`, before this is
    /// reached).
    fn handle_editor_key(&mut self, code: crossterm::event::KeyCode) -> Result<()> {
        let crate::app::file_browser::ViewerContent::Editor(editor) = &mut self.viewer else {
            return Ok(());
        };
        let mods = to_key_modifiers(self.modifiers);
        let ctrl = mods.contains(crossterm::event::KeyModifiers::CONTROL);
        match crate::app::editor::dispatch_editor_key(editor, code, ctrl) {
            crate::app::editor::EditorKeyOutcome::Handled => self.editor_status = None,
            crate::app::editor::EditorKeyOutcome::Saved(Ok(())) => {
                self.editor_status = Some("Saved".to_string());
            }
            crate::app::editor::EditorKeyOutcome::Saved(Err(e)) => {
                self.editor_status = Some(format!("Save failed: {e}"));
            }
            crate::app::editor::EditorKeyOutcome::Exit => {
                self.viewer = crate::app::file_browser::ViewerContent::Tree;
                self.editor_status = None;
                self.recompute_terminal_grid_from_window();
            }
        }
        Ok(())
    }

    /// Handle a key event while the built-in image viewer has focus. No
    /// zoom/pan/scroll in MVP scope (per the plan), so the only bound key
    /// is `Esc`, returning to the tree view — the panel's own toggle key
    /// closes the whole panel (handled earlier in `dispatch_key`). Every
    /// other key is swallowed, same "swallow while active" precedent as
    /// the tree panel and editor.
    fn handle_image_key(&mut self, code: crossterm::event::KeyCode) {
        if code == crossterm::event::KeyCode::Esc {
            self.renderer.set_image(None);
            self.viewer = crate::app::file_browser::ViewerContent::Tree;
            self.recompute_terminal_grid_from_window();
        }
    }

    /// Pixel width currently reserved for the file-browser tree panel —
    /// zero when it's closed, and also zero while the built-in editor or
    /// image viewer is showing (Phase 5/6) since both take over the full
    /// window instead of sharing the narrow tree-panel strip.
    fn file_browser_panel_px(&self) -> f32 {
        if self.file_browser.is_some() && self.viewer.is_tree() {
            self.file_browser_width as f32 * self.renderer.cell_size().0
        } else {
            0.0
        }
    }

    /// Recompute the terminal's cols/rows from the current window size,
    /// accounting for the panel's pixel width when open. Shared by
    /// `handle_resize` (a real `WindowEvent::Resized`) and by the panel
    /// toggle/close paths above (which don't fire one).
    fn recompute_terminal_grid_from_window(&mut self) {
        let size = self.window.inner_size();
        self.recompute_terminal_grid(size.width, size.height);
    }

    fn recompute_terminal_grid(&mut self, width: u32, height: u32) {
        let (cell_w, cell_h) = self.renderer.cell_size();
        let usable_width = (width as f32 - self.file_browser_panel_px()).max(cell_w);
        let cols = ((usable_width / cell_w).floor().max(1.0)) as u16;
        let rows = ((height as f32 / cell_h).floor().max(1.0)) as u16;
        let current = self.pane.emulator.size();
        if cols != current.cols || rows != current.rows {
            self.pane.emulator.resize(TerminalSize { cols, rows });
            let _ = self.pane.backend.resize(rows, cols);
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
        let (cr, cg, cb) = self.theme.cursor_rgb();
        let style = cursor_style(&self.pane.emulator);

        let (_, cell_h) = self.renderer.cell_size();
        let panel_px = self.file_browser_panel_px();
        let win_width = self.window.inner_size().width as f32;

        // Keep the selected row scrolled into view before rendering — same
        // clamp algorithm as the TUI's `run_app` panel carve-out.
        if let Some(browser) = self.file_browser.as_mut() {
            let visible_rows = (self.window.inner_size().height as f32 / cell_h)
                .floor()
                .max(1.0) as usize;
            let selected = browser.selected_index();
            let offset = browser.scroll_offset();
            let new_offset = if selected < offset {
                selected
            } else if selected >= offset + visible_rows {
                selected + 1 - visible_rows
            } else {
                offset
            };
            browser.set_scroll_offset(new_offset);
        }

        let (term_x_offset, panel_view) =
            if let Some(browser) = self.file_browser.as_ref().filter(|_| self.viewer.is_tree()) {
                let (fr, fgg, fb) = self.theme.fg_rgb();
                let (sel_r, sel_g, sel_b) = self.theme.ansi_color(4);
                let (sel_fg_r, sel_fg_g, sel_fg_b) = self.theme.bg_rgb();
                let x = match self.file_browser_position {
                    FileBrowserPosition::Left => 0.0,
                    FileBrowserPosition::Right => win_width - panel_px,
                };
                let term_offset = match self.file_browser_position {
                    FileBrowserPosition::Left => panel_px,
                    FileBrowserPosition::Right => 0.0,
                };
                (
                    term_offset,
                    Some(FileBrowserPanelView {
                        state: browser,
                        x,
                        width: panel_px,
                        fg: Rgb(fr, fgg, fb),
                        selected_bg: Rgb(sel_r, sel_g, sel_b),
                        selected_fg: Rgb(sel_fg_r, sel_fg_g, sel_fg_b),
                    }),
                )
            } else {
                (0.0, None)
            };

        let editor_view =
            if let crate::app::file_browser::ViewerContent::Editor(editor) = &self.viewer {
                let (fr, fgg, fb) = self.theme.fg_rgb();
                let (bar_r, bar_g, bar_b) = self.theme.ansi_color(4);
                let (bar_fg_r, bar_fg_g, bar_fg_b) = self.theme.bg_rgb();
                Some(EditorPanelView {
                    state: editor,
                    status: self.editor_status.as_deref(),
                    fg: Rgb(fr, fgg, fb),
                    bar_bg: Rgb(bar_r, bar_g, bar_b),
                    bar_fg: Rgb(bar_fg_r, bar_fg_g, bar_fg_b),
                })
            } else {
                None
            };

        let image_view = if let crate::app::file_browser::ViewerContent::Image(image) = &self.viewer
        {
            Some(ImagePanelView { state: image })
        } else {
            None
        };

        self.renderer.render(
            &cells,
            size.cols,
            size.rows,
            Rgb(bgr, bgg, bgb),
            self.selection,
            Rgb(sbr, sbg, sbb),
            Rgb(sfr, sfg, sfb),
            style,
            Rgb(cr, cg, cb),
            term_x_offset,
            panel_view,
            editor_view,
            image_view,
        )
    }

    fn handle_resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(width, height);
        self.recompute_terminal_grid(width, height);
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

        let keymap = KeymapResolver::new(
            &self.config.keybindings,
            &self.config.file_browser.keybinding,
        );

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
            file_browser: None,
            file_browser_show_hidden: self.config.file_browser.show_hidden,
            file_browser_width: self.config.file_browser.width,
            file_browser_position: self.config.file_browser.position,
            viewer: crate::app::file_browser::ViewerContent::Tree,
            editor_status: None,
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
