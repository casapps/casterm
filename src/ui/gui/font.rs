//! System monospace font discovery
//!
//! The repo ships no embedded `.ttf`/`.otf` (see `TODO.AI.md` for the
//! deferred "bundle a Nerd Font" stretch goal), so the GUI locates a
//! monospace font already installed on the host. `CASTERM_GUI_FONT_PATH`
//! lets a user override the search entirely; otherwise a short list of
//! well-known monospace font paths is tried first, then a bounded
//! filesystem walk of the platform's font directories looks for anything
//! with "mono" in its file name.

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// Well-known monospace font paths across common Linux distros, macOS, and
/// Windows. Checked before falling back to a directory walk.
const KNOWN_MONOSPACE_PATHS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
    "/usr/share/fonts/liberation-mono/LiberationMono-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/noto-mono/NotoSansMono-Regular.ttf",
    "/System/Library/Fonts/Menlo.ttc",
    "/System/Library/Fonts/Monaco.ttf",
    "/Library/Fonts/Menlo.ttc",
    "C:\\Windows\\Fonts\\consola.ttf",
    "C:\\Windows\\Fonts\\cascadiamono.ttf",
];

/// Platform font directories walked (bounded depth) when none of the
/// well-known paths exist.
const FONT_SEARCH_DIRS: &[&str] = &[
    "/usr/share/fonts",
    "/usr/local/share/fonts",
    "/run/host/fonts",
];

/// Locate a monospace font file to load for glyph rasterization.
///
/// Search order: `CASTERM_GUI_FONT_PATH` env override, then the well-known
/// path list, then a depth-bounded walk of the platform font directories
/// looking for a file name containing "mono" (case-insensitive), then
/// (last resort) the first `.ttf`/`.otf`/`.ttc` file found at all so the GUI
/// can still start with *a* font rather than failing outright.
pub fn find_monospace_font_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("CASTERM_GUI_FONT_PATH") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }

    for candidate in KNOWN_MONOSPACE_PATHS {
        let path = Path::new(candidate);
        if path.is_file() {
            return Some(path.to_path_buf());
        }
    }

    // Before the generic "mono"-named-file walk, look for a file matching
    // one of this platform's preferred font family names (e.g. "JetBrains
    // Mono" on Linux, "Menlo" on macOS) — a closer match to what the OS
    // ships by default than an arbitrary monospace font.
    let preferred_names: Vec<String> = crate::platform::Platform::default_fonts()
        .iter()
        .map(|n| n.to_ascii_lowercase().replace([' ', '-', '_'], ""))
        .collect();
    for dir in FONT_SEARCH_DIRS {
        for entry in WalkDir::new(dir)
            .max_depth(6)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            if !matches!(ext.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|n| n.to_str()) else {
                continue;
            };
            let normalized = stem.to_ascii_lowercase().replace([' ', '-', '_'], "");
            if preferred_names
                .iter()
                .any(|p| normalized.contains(p.as_str()))
            {
                return Some(path.to_path_buf());
            }
        }
    }

    let mut any_font: Option<PathBuf> = None;
    for dir in FONT_SEARCH_DIRS {
        for entry in WalkDir::new(dir)
            .max_depth(6)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            if !matches!(ext.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc") {
                continue;
            }
            let is_mono = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.to_ascii_lowercase().contains("mono"))
                .unwrap_or(false);
            if is_mono {
                return Some(path.to_path_buf());
            }
            if any_font.is_none() {
                any_font = Some(path.to_path_buf());
            }
        }
    }

    any_font
}

/// Load a `fontdue::Font` from `find_monospace_font_path()`'s result.
pub fn load_font() -> Option<fontdue::Font> {
    let path = find_monospace_font_path()?;
    let bytes = std::fs::read(path).ok()?;
    fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).ok()
}
