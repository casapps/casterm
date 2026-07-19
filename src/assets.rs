//! Embedded assets (themes, icons, default config)

use rust_embed::Embed;

use crate::config::ThemePalette;
use crate::support::error::{CastermError, Result};

/// Embedded asset files
#[derive(Embed)]
#[folder = "assets/"]
#[prefix = ""]
pub struct Assets;

impl Assets {
    /// Get a file's contents as bytes
    pub fn get_bytes(path: &str) -> Option<std::borrow::Cow<'static, [u8]>> {
        Self::get(path).map(|f| f.data)
    }

    /// Get a file's contents as a string
    pub fn get_string(path: &str) -> Option<String> {
        Self::get_bytes(path).and_then(|b| String::from_utf8(b.to_vec()).ok())
    }

    /// List all files matching a prefix
    pub fn list_prefix(prefix: &str) -> impl Iterator<Item = std::borrow::Cow<'static, str>> + use<'_> {
        Self::iter().filter(move |name| name.starts_with(prefix))
    }
}

/// Load a theme from embedded assets
pub fn load_theme(name: &str) -> Result<ThemePalette> {
    let path = format!("themes/{}.toml", name);

    let content = Assets::get_string(&path)
        .ok_or_else(|| CastermError::Theme(format!("Theme '{}' not found", name)))?;

    toml::from_str(&content)
        .map_err(|e| CastermError::Theme(format!("Failed to parse theme '{}': {}", name, e)))
}

/// List all available themes
pub fn list_themes() -> Vec<String> {
    Assets::list_prefix("themes/")
        .filter_map(|name| {
            name.strip_prefix("themes/")
                .and_then(|n| n.strip_suffix(".toml"))
                .map(String::from)
        })
        .collect()
}

/// Get the default configuration file contents
pub fn default_config() -> Option<String> {
    Assets::get_string("config/default.yaml")
}

/// Get an icon by name
pub fn get_icon(name: &str) -> Option<std::borrow::Cow<'static, [u8]>> {
    let path = format!("icons/{}.png", name);
    Assets::get_bytes(&path)
}
