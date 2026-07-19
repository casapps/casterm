//! Error types for casterm

use thiserror::Error;

/// Result type alias using CastermError
pub type Result<T> = std::result::Result<T, CastermError>;

/// Main error type for casterm
#[derive(Error, Debug)]
pub enum CastermError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("PTY error: {0}")]
    Pty(String),

    #[error("Terminal error: {0}")]
    Terminal(String),

    #[error("SSH error: {0}")]
    Ssh(String),

    #[error("Serial port error: {0}")]
    Serial(String),

    #[error("TUI error: {0}")]
    Tui(String),

    #[error("GUI error: {0}")]
    Gui(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("Theme error: {0}")]
    Theme(String),

    #[error("No display available")]
    NoDisplay,

    #[error("Unsupported platform: {0}")]
    UnsupportedPlatform(String),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

impl From<figment::Error> for CastermError {
    fn from(e: figment::Error) -> Self {
        CastermError::Config(e.to_string())
    }
}

impl From<serde_json::Error> for CastermError {
    fn from(e: serde_json::Error) -> Self {
        CastermError::Config(e.to_string())
    }
}

impl From<toml::de::Error> for CastermError {
    fn from(e: toml::de::Error) -> Self {
        CastermError::Theme(e.to_string())
    }
}
