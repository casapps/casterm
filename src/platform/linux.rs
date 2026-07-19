//! Linux-specific platform code

use std::path::PathBuf;

/// Linux-specific platform utilities
pub struct Linux;

impl Linux {
    /// Detect the display server in use
    pub fn display_server() -> DisplayServer {
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            DisplayServer::Wayland
        } else if std::env::var("DISPLAY").is_ok() {
            DisplayServer::X11
        } else {
            DisplayServer::None
        }
    }

    /// Check if systemd is available
    pub fn has_systemd() -> bool {
        std::path::Path::new("/run/systemd/system").exists()
    }

    /// Get the XDG config home
    pub fn xdg_config_home() -> PathBuf {
        std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .map(|h| h.join(".config"))
                    .unwrap_or_else(|| std::env::temp_dir())
            })
    }

    /// Get the XDG data home
    pub fn xdg_data_home() -> PathBuf {
        std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .map(|h| h.join(".local/share"))
                    .unwrap_or_else(|| std::env::temp_dir())
            })
    }

    /// Get the XDG runtime dir
    pub fn xdg_runtime_dir() -> Option<PathBuf> {
        std::env::var("XDG_RUNTIME_DIR").ok().map(PathBuf::from)
    }
}

/// Display server type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayServer {
    Wayland,
    X11,
    None,
}
