//! Platform-specific integrations

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

use std::path::Path;
use std::process::Command;

use crate::support::error::{CastermError, Result};

/// Platform abstraction layer
pub struct Platform;

impl Platform {
    /// Check if running in a GUI-capable environment
    pub fn has_display() -> bool {
        #[cfg(target_os = "linux")]
        {
            std::env::var("WAYLAND_DISPLAY").is_ok() || std::env::var("DISPLAY").is_ok()
        }
        #[cfg(target_os = "macos")]
        {
            true
        }
        #[cfg(target_os = "windows")]
        {
            true
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            false
        }
    }

    /// Check if running over SSH/remote session
    pub fn is_remote_session() -> bool {
        std::env::var("SSH_CONNECTION").is_ok()
            || std::env::var("SSH_CLIENT").is_ok()
            || std::env::var("SSH_TTY").is_ok()
            || std::env::var("MOSH_IP").is_ok()
            || std::env::var("MOSH_KEY").is_ok()
    }

    /// Hand a file off to the operating system's default application for
    /// it, as a detached, non-blocking process — used by the local file
    /// browser for any file that isn't opened by casterm's own built-in
    /// text editor/image viewer (PDF, video, archives, office documents,
    /// …). Requires a display: returns a clear `CastermError` (never a
    /// silent no-op) when `has_display()` is false, since there is nothing
    /// to hand the file off to in a headless/SSH-only session.
    pub fn open_with_default_app(path: &Path) -> Result<()> {
        if !Self::has_display() {
            return Err(CastermError::NoDisplay);
        }

        #[cfg(target_os = "linux")]
        let result = Command::new("xdg-open").arg(path).spawn();

        #[cfg(target_os = "macos")]
        let result = Command::new("open").arg(path).spawn();

        #[cfg(target_os = "windows")]
        let result = Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(path)
            .spawn();

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        let result: std::io::Result<std::process::Child> = Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "opening files in the OS default application is not supported on this platform",
        ));

        result.map(|_child| ()).map_err(|e| {
            CastermError::Other(anyhow::anyhow!(
                "failed to open {} in the OS default application: {e}",
                path.display()
            ))
        })
    }

    /// Get default font families for the current platform
    pub fn default_fonts() -> Vec<&'static str> {
        #[cfg(target_os = "macos")]
        {
            vec!["Menlo", "SF Mono", "Monaco"]
        }
        #[cfg(target_os = "windows")]
        {
            vec!["Cascadia Code", "Consolas", "Courier New"]
        }
        #[cfg(target_os = "linux")]
        {
            vec![
                "JetBrains Mono",
                "Fira Code",
                "DejaVu Sans Mono",
                "monospace",
            ]
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            vec!["monospace"]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn open_with_default_app_errors_clearly_when_headless() {
        // Fully unit-testable without a real desktop: simulate the
        // no-display case by clearing the env vars `has_display()` checks.
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("DISPLAY");
        let result = Platform::open_with_default_app(Path::new("/tmp/does-not-matter"));
        assert!(result.is_err());
        assert!(!result.unwrap_err().to_string().is_empty());
    }
}
