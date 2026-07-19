//! Configuration loading and defaults

mod defaults;
mod theme;

pub use defaults::*;
pub use theme::*;

use figment::{
    providers::{Env, Format, Serialized, Yaml},
    Figment,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::support::error::Result;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    // Shell
    pub shell: ShellConfig,

    // Appearance
    pub theme: ThemeConfig,
    pub font: FontConfig,
    pub cursor: CursorConfig,

    // Behavior
    pub behavior: BehaviorConfig,

    // Multiplexer
    pub multiplexer: MultiplexerConfig,

    // Status bar
    pub status_bar: StatusBarConfig,

    // Key bindings
    pub keybindings: KeyBindingsConfig,

    // Connections
    pub connections: ConnectionsConfig,

    // Logging
    pub logging: LoggingConfig,

    // Updates
    pub updates: UpdatesConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shell: ShellConfig::default(),
            theme: ThemeConfig::default(),
            font: FontConfig::default(),
            cursor: CursorConfig::default(),
            behavior: BehaviorConfig::default(),
            multiplexer: MultiplexerConfig::default(),
            status_bar: StatusBarConfig::default(),
            keybindings: KeyBindingsConfig::default(),
            connections: ConnectionsConfig::default(),
            logging: LoggingConfig::default(),
            updates: UpdatesConfig::default(),
        }
    }
}

impl Config {
    /// Load configuration from file, environment, and defaults
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let mut figment = Figment::new().merge(Serialized::defaults(Config::default()));

        // User config locations
        let config_paths = Self::config_paths();

        // Load from default locations
        for p in &config_paths {
            if p.exists() {
                figment = figment.merge(Yaml::file(p));
            }
        }

        // Load from explicit path
        if let Some(p) = path {
            figment = figment.merge(Yaml::file(p));
        }

        // Environment overrides (CASTERM_ prefix)
        figment = figment.merge(Env::prefixed("CASTERM_").split("_"));

        let config: Config = figment.extract()?;
        Ok(config)
    }

    /// Get list of config file paths to check
    fn config_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // XDG config
        if let Some(config_dir) = dirs::config_dir() {
            paths.push(config_dir.join("casapps/casterm/config.yml"));
            paths.push(config_dir.join("casapps/casterm/config.yaml"));
        }

        // Home directory
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".config/casapps/casterm/config.yml"));
            paths.push(home.join(".castermrc"));
        }

        // Current directory (project-local)
        paths.push(PathBuf::from(".casterm.yml"));
        paths.push(PathBuf::from("casterm.yml"));

        paths
    }

    /// Get the config directory path
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("casapps/casterm")
    }

    /// Get the data directory path
    pub fn data_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("casapps/casterm")
    }

    /// Get the cache directory path
    pub fn cache_dir() -> PathBuf {
        dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("casapps/casterm")
    }

    /// Get the log directory path
    pub fn log_dir() -> PathBuf {
        dirs::state_dir()
            .or_else(dirs::data_local_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("casapps/casterm/logs")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ShellConfig {
    /// Full path to shell binary
    pub path: Option<PathBuf>,
    /// Shell arguments
    pub args: Vec<String>,
    /// Login shell
    pub login: bool,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            path: None,
            args: Vec::new(),
            login: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FontConfig {
    /// Font family name
    pub family: String,
    /// Font size in points
    pub size: f32,
    /// Line height multiplier
    pub line_height: f32,
    /// Enable ligatures
    pub ligatures: bool,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: "Source Code Pro".to_string(),
            size: 12.0,
            line_height: 1.2,
            ligatures: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CursorConfig {
    /// Cursor shape: block, ibeam, underline
    pub shape: CursorShape,
    /// Cursor blink
    pub blink: bool,
    /// Blink interval in milliseconds
    pub blink_interval: u32,
    /// Cursor color (hex or theme reference)
    pub color: String,
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            shape: CursorShape::IBeam,
            blink: true,
            blink_interval: 530,
            color: "#008080".to_string(), // Teal
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CursorShape {
    Block,
    #[default]
    IBeam,
    Underline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BehaviorConfig {
    /// Scrollback buffer lines
    pub scrollback_lines: u32,
    /// Enable audible bell
    pub audible_bell: bool,
    /// Enable visual bell
    pub visual_bell: bool,
    /// Confirm before paste of dangerous content
    pub confirm_paste: bool,
    /// Close window on shell exit
    pub close_on_exit: CloseOnExit,
    /// Enable URL detection
    pub url_detection: bool,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            scrollback_lines: 10000,
            audible_bell: false,
            visual_bell: true,
            confirm_paste: true,
            close_on_exit: CloseOnExit::OnCleanExit,
            url_detection: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CloseOnExit {
    Always,
    Never,
    #[default]
    OnCleanExit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MultiplexerConfig {
    /// Prefix key
    pub prefix_key: String,
    /// Mouse support
    pub mouse: bool,
    /// Default layout for new windows
    pub default_layout: String,
    /// Auto-rename windows
    pub auto_rename: bool,
    /// Session persistence
    pub persist_sessions: bool,
}

impl Default for MultiplexerConfig {
    fn default() -> Self {
        Self {
            prefix_key: "Ctrl+Space".to_string(),
            mouse: true,
            default_layout: "even".to_string(),
            auto_rename: true,
            persist_sessions: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StatusBarConfig {
    /// Enable status bar
    pub enabled: bool,
    /// Position: top or bottom
    pub position: StatusBarPosition,
    /// Left segments
    pub left: Vec<String>,
    /// Center segments
    pub center: Vec<String>,
    /// Right segments
    pub right: Vec<String>,
}

impl Default for StatusBarConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            position: StatusBarPosition::Bottom,
            left: vec![
                "mode".to_string(),
                "session".to_string(),
                "window".to_string(),
            ],
            center: vec!["pane_title".to_string()],
            right: vec![
                "git_branch".to_string(),
                "hostname".to_string(),
                "time".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StatusBarPosition {
    Top,
    #[default]
    Bottom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyBindingsConfig {
    /// Copy mode style: vi or emacs
    pub copy_mode_style: CopyModeStyle,
    /// Custom key bindings
    pub bindings: Vec<KeyBinding>,
}

impl Default for KeyBindingsConfig {
    fn default() -> Self {
        Self {
            copy_mode_style: CopyModeStyle::Vi,
            bindings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CopyModeStyle {
    #[default]
    Vi,
    Emacs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBinding {
    pub key: String,
    pub action: String,
    #[serde(default)]
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConnectionsConfig {
    /// Default SSH port
    pub default_ssh_port: u16,
    /// SSH keepalive interval (seconds)
    pub ssh_keepalive: u32,
    /// Auto-reconnect on disconnect
    pub auto_reconnect: bool,
}

impl Default for ConnectionsConfig {
    fn default() -> Self {
        Self {
            default_ssh_port: 22,
            ssh_keepalive: 60,
            auto_reconnect: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    /// Enable session logging
    pub session_logging: bool,
    /// Log directory
    pub log_directory: PathBuf,
    /// Log filename pattern
    pub log_filename_pattern: String,
    /// Add timestamps to log
    pub log_timestamps: bool,
    /// Strip ANSI from logs
    pub log_strip_ansi: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            session_logging: false,
            log_directory: Config::log_dir(),
            log_filename_pattern: "%Y%m%d-%H%M%S-{session}.log".to_string(),
            log_timestamps: true,
            log_strip_ansi: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdatesConfig {
    /// Update channel: stable, beta, daily
    pub channel: UpdateChannel,
    /// Check for updates on startup
    pub check_on_startup: bool,
    /// Auto-update (never without confirmation)
    pub auto_update: bool,
}

impl Default for UpdatesConfig {
    fn default() -> Self {
        Self {
            channel: UpdateChannel::Stable,
            check_on_startup: true,
            auto_update: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    #[default]
    Stable,
    Beta,
    Daily,
}

