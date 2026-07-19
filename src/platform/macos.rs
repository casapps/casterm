//! macOS-specific platform code

use std::path::PathBuf;

/// macOS-specific platform utilities
pub struct MacOs;

impl MacOs {
    /// Get the Application Support directory
    pub fn application_support() -> Option<PathBuf> {
        dirs::data_dir()
    }

    /// Check if running as a macOS app bundle
    pub fn is_app_bundle() -> bool {
        std::env::var("__CFBundleIdentifier").is_ok()
    }

    /// Get the bundle identifier if running as app
    pub fn bundle_identifier() -> Option<String> {
        std::env::var("__CFBundleIdentifier").ok()
    }

    /// Check if in a Terminal.app or iTerm session
    pub fn terminal_app() -> Option<&'static str> {
        std::env::var("TERM_PROGRAM")
            .ok()
            .and_then(|p| match p.as_str() {
                "Apple_Terminal" => Some("Terminal.app"),
                "iTerm.app" => Some("iTerm2"),
                "Hyper" => Some("Hyper"),
                "WezTerm" => Some("WezTerm"),
                "Alacritty" => Some("Alacritty"),
                "kitty" => Some("kitty"),
                _ => None,
            })
    }

    /// Get default Homebrew shell path if available
    pub fn homebrew_shell(name: &str) -> Option<PathBuf> {
        let arm_path = PathBuf::from(format!("/opt/homebrew/bin/{}", name));
        let intel_path = PathBuf::from(format!("/usr/local/bin/{}", name));

        if arm_path.exists() {
            Some(arm_path)
        } else if intel_path.exists() {
            Some(intel_path)
        } else {
            None
        }
    }
}
