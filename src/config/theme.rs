//! Theme configuration and built-in theme catalog

use serde::{Deserialize, Serialize};

/// Theme configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    /// Mode: dark, light, auto
    pub mode: ThemeMode,
    /// Theme name (built-in or "custom")
    pub name: String,
    /// Custom theme file path (empty for built-in)
    pub file: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Dark,
            name: "dracula".to_string(),
            file: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    Auto,
}

/// Color palette for a theme
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemePalette {
    // Primary colors
    pub background: String,
    pub foreground: String,

    // Cursor
    pub cursor: String,
    pub cursor_text: String,

    // Selection
    pub selection_background: String,
    pub selection_foreground: String,

    // ANSI colors (0-7: normal, 8-15: bright)
    pub black: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub magenta: String,
    pub cyan: String,
    pub white: String,

    pub bright_black: String,
    pub bright_red: String,
    pub bright_green: String,
    pub bright_yellow: String,
    pub bright_blue: String,
    pub bright_magenta: String,
    pub bright_cyan: String,
    pub bright_white: String,
}

impl Default for ThemePalette {
    fn default() -> Self {
        // Dracula theme as default
        Self {
            background: "#282a36".to_string(),
            foreground: "#f8f8f2".to_string(),
            cursor: "#f8f8f2".to_string(),
            cursor_text: "#282a36".to_string(),
            selection_background: "#44475a".to_string(),
            selection_foreground: "#f8f8f2".to_string(),
            black: "#21222c".to_string(),
            red: "#ff5555".to_string(),
            green: "#50fa7b".to_string(),
            yellow: "#f1fa8c".to_string(),
            blue: "#bd93f9".to_string(),
            magenta: "#ff79c6".to_string(),
            cyan: "#8be9fd".to_string(),
            white: "#f8f8f2".to_string(),
            bright_black: "#6272a4".to_string(),
            bright_red: "#ff6e6e".to_string(),
            bright_green: "#69ff94".to_string(),
            bright_yellow: "#ffffa5".to_string(),
            bright_blue: "#d6acff".to_string(),
            bright_magenta: "#ff92df".to_string(),
            bright_cyan: "#a4ffff".to_string(),
            bright_white: "#ffffff".to_string(),
        }
    }
}

/// Built-in theme catalog
pub struct ThemeCatalog;

impl ThemeCatalog {
    /// Dual-mode themes (have both _dark and _light variants)
    pub const DUAL_MODE: &'static [&'static str] = &[
        "ashes",
        "ayu",
        "enfocado",
        "everforest",
        "github",
        "gruvbox",
        "kimbie",
        "one",
        "papercolor",
        "pencil",
        "selenized",
        "solarized",
    ];

    /// Themes with a light variant available
    pub const HAS_LIGHT_VARIANT: &'static [&'static str] = &["nord", "tokyo_night"];

    /// Single-mode dark themes
    pub const SINGLE_DARK: &'static [&'static str] = &[
        "afterglow",
        "argonaut",
        "baitong",
        "bluish",
        "catppuccin",
        "challenger_deep",
        "citylights",
        "Cobalt2",
        "cyber_punk_neon",
        "dark_pastels",
        "dark_pride",
        "deep_space",
        "doom_one",
        "dracula",
        "falcon",
        "flat_remix",
        "flexoki",
        "ganbaru",
        "gnome_terminal",
        "google",
        "gotham",
        "gruvbox_material",
        "hardhacker",
        "hatsunemiku",
        "hyper",
        "inferno",
        "iris",
        "iterm",
        "kitty",
        "konsole_linux",
        "linux",
        "Mariana",
        "material_theme",
        "meliora",
        "midnight_haze",
        "monokai",
        "nordic",
        "oceanic_next",
        "omni",
        "oxocarbon",
        "palenight",
        "panda",
        "rainbow",
        "rigel",
        "rose_pine",
        "seashells",
        "smoooooth",
        "snazzy",
        "spacegray",
        "taerminal",
        "tender",
        "terminal_app",
        "thelovelace",
        "tomorrow_night",
        "ubuntu",
        "vesper",
        "wombat",
        "xterm",
        "zenburn",
    ];

    /// Single-mode light themes
    pub const SINGLE_LIGHT: &'static [&'static str] = &[
        "acme",
        "alabaster",
        "modus_operandi",
        "noctis_lux",
        "papertheme",
        "tomorrow",
    ];

    /// Resolve theme name based on mode
    ///
    /// For dual-mode themes, appends _dark or _light based on mode.
    /// For single-mode themes, returns the name as-is.
    pub fn resolve_theme_name(name: &str, mode: ThemeMode) -> String {
        let effective_mode = match mode {
            ThemeMode::Auto => detect_system_theme(),
            m => m,
        };

        // Check if dual-mode
        if Self::DUAL_MODE.contains(&name) {
            match effective_mode {
                ThemeMode::Dark => format!("{}_dark", name),
                ThemeMode::Light => format!("{}_light", name),
                ThemeMode::Auto => format!("{}_dark", name),
            }
        } else if Self::HAS_LIGHT_VARIANT.contains(&name) && effective_mode == ThemeMode::Light {
            format!("{}_light", name)
        } else {
            // Single-mode: use as-is
            name.to_string()
        }
    }

    /// Check if a theme name is valid
    pub fn is_valid_theme(name: &str) -> bool {
        Self::DUAL_MODE.contains(&name)
            || Self::HAS_LIGHT_VARIANT.contains(&name)
            || Self::SINGLE_DARK.contains(&name)
            || Self::SINGLE_LIGHT.contains(&name)
    }

    /// Get all available theme names
    pub fn all_themes() -> Vec<&'static str> {
        let mut themes = Vec::new();
        themes.extend_from_slice(Self::DUAL_MODE);
        themes.extend_from_slice(Self::HAS_LIGHT_VARIANT);
        themes.extend_from_slice(Self::SINGLE_DARK);
        themes.extend_from_slice(Self::SINGLE_LIGHT);
        themes.sort();
        themes
    }
}

/// Parse a hex color string `"#rrggbb"` (or `"rrggbb"`) to `(r, g, b)`.
/// Returns `None` for any malformed input.
pub fn parse_hex_color(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim().strip_prefix('#').unwrap_or(s.trim());
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some((r, g, b))
}

impl ThemePalette {
    /// Background color as `(r, g, b)`.
    pub fn bg_rgb(&self) -> (u8, u8, u8) {
        parse_hex_color(&self.background).unwrap_or((0, 0, 0))
    }

    /// Foreground color as `(r, g, b)`.
    pub fn fg_rgb(&self) -> (u8, u8, u8) {
        parse_hex_color(&self.foreground).unwrap_or((200, 200, 200))
    }

    /// ANSI palette entry 0-15 as `(r, g, b)`. Indices 0-7 are normal, 8-15 are bright.
    pub fn ansi_color(&self, index: u8) -> (u8, u8, u8) {
        let hex = match index {
            0 => &self.black,
            1 => &self.red,
            2 => &self.green,
            3 => &self.yellow,
            4 => &self.blue,
            5 => &self.magenta,
            6 => &self.cyan,
            7 => &self.white,
            8 => &self.bright_black,
            9 => &self.bright_red,
            10 => &self.bright_green,
            11 => &self.bright_yellow,
            12 => &self.bright_blue,
            13 => &self.bright_magenta,
            14 => &self.bright_cyan,
            15 => &self.bright_white,
            _ => &self.foreground,
        };
        parse_hex_color(hex).unwrap_or((128, 128, 128))
    }

    /// Cursor color as `(r, g, b)`.
    pub fn cursor_rgb(&self) -> (u8, u8, u8) {
        parse_hex_color(&self.cursor).unwrap_or((200, 200, 200))
    }
}

/// Detect system theme preference
pub fn detect_system_theme() -> ThemeMode {
    match dark_light::detect() {
        Ok(dark_light::Mode::Dark) => ThemeMode::Dark,
        Ok(dark_light::Mode::Light) => ThemeMode::Light,
        Ok(dark_light::Mode::Unspecified) => ThemeMode::Dark,
        Err(_) => ThemeMode::Dark,
    }
}

/// Detect terminal background when running nested
pub fn detect_terminal_background() -> Option<ThemeMode> {
    // Check TERMINAL_BACKGROUND override
    if let Ok(val) = std::env::var("TERMINAL_BACKGROUND") {
        match val.to_lowercase().as_str() {
            "dark" => return Some(ThemeMode::Dark),
            "light" => return Some(ThemeMode::Light),
            _ => {}
        }
    }

    // Check COLORFGBG (format: "fg;bg" where bg < 7 = dark, > 8 = light)
    if let Ok(val) = std::env::var("COLORFGBG") {
        if let Some((_, bg)) = val.split_once(';') {
            if let Ok(bg_num) = bg.parse::<u8>() {
                if bg_num < 7 {
                    return Some(ThemeMode::Dark);
                } else if bg_num > 8 {
                    return Some(ThemeMode::Light);
                }
            }
        }
    }

    None
}
