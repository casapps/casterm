//! GUI mode using winit + wgpu

use std::path::Path;

use crate::config::Config;
use crate::support::error::{CastermError, Result};

/// Run the GUI terminal
pub fn run(config: &Config, command: &Option<String>, directory: Option<&Path>) -> Result<()> {
    tracing::info!("Starting GUI mode");

    // Check for display
    let wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
    let x11 = std::env::var("DISPLAY").is_ok();

    if !wayland && !x11 && !cfg!(windows) && !cfg!(target_os = "macos") {
        return Err(CastermError::NoDisplay);
    }

    // GUI windowing lands in Phase 6; until then, fall back to TUI with
    // default session naming/auto-restore (no `--session`/`--no-restore`
    // equivalents exist at the GUI entry point yet).
    tracing::warn!("GUI not yet implemented, falling back to TUI");
    super::tui::run(config, command, directory, None, true)
}
