//! Platform-specific integrations

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

use std::path::PathBuf;

/// Platform abstraction layer
pub struct Platform;

impl Platform {
    /// Get the user's home directory
    pub fn home_dir() -> Option<PathBuf> {
        dirs::home_dir()
    }

    /// Get the config directory for casterm
    pub fn config_dir() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("casterm"))
    }

    /// Get the data directory for casterm
    pub fn data_dir() -> Option<PathBuf> {
        dirs::data_dir().map(|p| p.join("casterm"))
    }

    /// Get the cache directory for casterm
    pub fn cache_dir() -> Option<PathBuf> {
        dirs::cache_dir().map(|p| p.join("casterm"))
    }

    /// Get the runtime directory (for sockets, etc.)
    pub fn runtime_dir() -> Option<PathBuf> {
        #[cfg(unix)]
        {
            dirs::runtime_dir()
                .or_else(|| std::env::var("XDG_RUNTIME_DIR").ok().map(PathBuf::from))
                .map(|p| p.join("casterm"))
        }
        #[cfg(windows)]
        {
            dirs::data_local_dir().map(|p| p.join("casterm").join("run"))
        }
    }

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
    }

    /// Get the session socket path
    pub fn session_socket_path(name: &str) -> Option<PathBuf> {
        Self::runtime_dir().map(|p| p.join(format!("{}.sock", name)))
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
            vec!["JetBrains Mono", "Fira Code", "DejaVu Sans Mono", "monospace"]
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            vec!["monospace"]
        }
    }
}
