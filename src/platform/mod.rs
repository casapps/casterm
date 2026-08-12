//! Platform-specific integrations

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

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
