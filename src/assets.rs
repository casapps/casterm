//! Embedded assets (themes, icons, default config)

use rust_embed::Embed;

use crate::config::{ThemeCatalog, ThemePalette};
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
    pub fn list_prefix(
        prefix: &str,
    ) -> impl Iterator<Item = std::borrow::Cow<'static, str>> + use<'_> {
        Self::iter().filter(move |name| name.starts_with(prefix))
    }
}

/// Load a theme from embedded assets
pub fn load_theme(name: &str) -> Result<ThemePalette> {
    let path = format!("themes/{}.toml", name);

    let content = Assets::get_string(&path).ok_or_else(|| {
        CastermError::Theme(format!(
            "Theme '{name}' not found (valid themes: {})",
            ThemeCatalog::all_themes().join(", ")
        ))
    })?;

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Backs `--list-themes` (see `main.rs`): every embedded `themes/*.toml`
    /// file must show up as a bare theme name, and every one of them must
    /// actually load through `load_theme()`.
    #[test]
    fn list_themes_returns_loadable_theme_names() {
        let names = list_themes();

        assert!(!names.is_empty());
        assert!(names.contains(&"dracula".to_string()));
        for name in &names {
            assert!(load_theme(name).is_ok(), "theme '{name}' failed to load");
        }
    }

    #[test]
    fn load_theme_reports_valid_names_in_error_for_unknown_theme() {
        let err = load_theme("not-a-real-theme").unwrap_err();
        assert!(err.to_string().contains("dracula"));
    }
}
