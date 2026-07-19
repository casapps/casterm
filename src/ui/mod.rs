//! UI layer: GUI, TUI, and CLI modes

pub mod cli;
pub mod gui;
pub mod tui;

use crate::config::Config;

/// UI mode enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMode {
    Gui,
    Tui,
    Cli,
}

/// Runtime capabilities for UI detection
struct Capabilities {
    gui_supported: bool,
    tui_supported: bool,
    stdin_tty: bool,
    stdout_tty: bool,
    interactive_launch: bool,
    tui_requires_formatting: bool,
    machine_output_requested: bool,
    local_windows_session: bool,
    local_macos_session: bool,
}

impl Capabilities {
    fn detect() -> Self {
        use std::io::IsTerminal;

        Self {
            gui_supported: cfg!(feature = "gui"),
            tui_supported: cfg!(feature = "tui"),
            stdin_tty: std::io::stdin().is_terminal(),
            stdout_tty: std::io::stdout().is_terminal(),
            interactive_launch: true, // Could check for specific launch contexts
            tui_requires_formatting: true,
            machine_output_requested: false,
            #[cfg(windows)]
            local_windows_session: true,
            #[cfg(not(windows))]
            local_windows_session: false,
            #[cfg(target_os = "macos")]
            local_macos_session: std::env::var("__CFBundleIdentifier").is_ok()
                || std::env::var("TERM_PROGRAM").is_ok(),
            #[cfg(not(target_os = "macos"))]
            local_macos_session: false,
        }
    }
}

/// Environment variable checker
struct Env;

impl Env {
    fn has(name: &str) -> bool {
        std::env::var(name).is_ok()
    }

    fn has_any(names: &[&str]) -> bool {
        names.iter().any(|n| Self::has(n))
    }

    fn is(name: &str, value: &str) -> bool {
        std::env::var(name).map(|v| v == value).unwrap_or(false)
    }

    fn truthy(name: &str) -> bool {
        std::env::var(name)
            .map(|v| !v.is_empty() && v != "0" && v.to_lowercase() != "false")
            .unwrap_or(false)
    }
}

/// Detect the appropriate UI mode based on environment
pub fn detect_ui_mode(_config: &Config) -> UiMode {
    let caps = Capabilities::detect();

    // Check for remote shell indicators
    let remote_shell = Env::has_any(&[
        "SSH_CONNECTION",
        "SSH_CLIENT",
        "SSH_TTY",
        "MOSH_IP",
        "MOSH_KEY",
    ]);

    // Check for display availability
    let display_available = Env::has("WAYLAND_DISPLAY")
        || Env::has("DISPLAY")
        || caps.local_windows_session
        || caps.local_macos_session;

    // GUI mode: supported, not remote, display available, interactive
    if caps.gui_supported && !remote_shell && display_available && caps.interactive_launch {
        return UiMode::Gui;
    }

    // Check for plain/non-interactive indicators
    let plain_or_noninteractive = !caps.stdin_tty
        || !caps.stdout_tty
        || Env::is("TERM", "dumb")
        || Env::truthy("CI")
        || (Env::truthy("NO_COLOR") && caps.tui_requires_formatting)
        || caps.machine_output_requested;

    // TUI mode: supported, interactive terminal
    if caps.tui_supported && !plain_or_noninteractive {
        return UiMode::Tui;
    }

    // CLI mode: fallback
    UiMode::Cli
}
