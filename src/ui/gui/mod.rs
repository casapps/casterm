//! GUI mode using winit + wgpu
//!
//! Opens a single OS window backed by a local shell PTY (reusing
//! `app::pane_runtime` and `ui::render_model`, the same shared plumbing the
//! TUI front end uses) and renders the terminal grid with a wgpu monospace
//! glyph atlas built from a system-discovered font (`font.rs` — no font is
//! embedded in this repo yet, see `TODO.AI.md`). SSH/serial-backed GUI
//! panes and multi-pane splits are out of scope for this MVP; see
//! `TODO.AI.md` for the full deferred-scope list.

mod font;
mod input;
mod renderer;
mod window;

use std::path::Path;

use winit::event_loop::EventLoop;

use crate::config::Config;
use crate::support::error::{CastermError, Result};

use window::GuiApp;

/// Run the GUI terminal
pub fn run(config: &Config, _command: &Option<String>, directory: Option<&Path>) -> Result<()> {
    tracing::info!("Starting GUI mode");

    if !crate::platform::Platform::has_display() {
        return Err(CastermError::NoDisplay);
    }

    let theme_name =
        crate::config::ThemeCatalog::resolve_theme_name(&config.theme.name, config.theme.mode);
    let theme = crate::assets::load_theme(&theme_name)
        .unwrap_or_else(|_| crate::config::ThemePalette::default());

    let event_loop = EventLoop::new()
        .map_err(|e| CastermError::Gui(format!("failed to create event loop: {e}")))?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);

    let mut app = GuiApp::new(config.clone(), theme, directory.map(Path::to_path_buf));
    event_loop
        .run_app(&mut app)
        .map_err(|e| CastermError::Gui(format!("event loop error: {e}")))?;

    app.into_result()
}
