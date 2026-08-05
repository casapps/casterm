//! Embedded `casterm` terminfo entry
//!
//! casterm ships a minimal terminfo entry (`assets/terminfo/casterm.terminfo`)
//! that inherits every capability from `xterm-256color` and adds the `Tc`
//! extension to declare genuine 24-bit truecolor support (implemented in
//! `app::vte_processor` via the `38;2;r;g;b` / `48;2;r;g;b` SGR sequences).
//!
//! It is compiled at build time with `tic` (see `build.rs`) and embedded
//! into the binary, so no host-side `tic`/ncurses toolchain or terminfo
//! database is required to run casterm. At startup the compiled entry is
//! extracted into a per-user cache directory and referenced via the
//! `TERMINFO` environment variable when spawning the PTY child — never
//! installed into any system-wide location, and never requires elevated
//! privileges.
//!
//! If the embedded entry is missing (build environment lacked `tic`) or
//! extraction fails for any reason, callers fall back to the standard
//! `TERM=xterm-256color` identity, which casterm fully implements anyway.

use std::fs;
use std::path::PathBuf;

/// Compiled terminfo entry, embedded at build time. Empty if `tic` was
/// unavailable when the project was built.
static COMPILED_ENTRY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/casterm.terminfo.bin"));

/// The `TERM` value casterm advertises when its own terminfo entry is
/// available.
pub const TERM_NAME: &str = "casterm";

/// Extract the embedded `casterm` terminfo entry into a per-user cache
/// directory and return the directory to set as `TERMINFO` for spawned
/// child processes. Returns `None` if the entry was not embedded (no `tic`
/// at build time) or extraction failed — callers should fall back to
/// `TERM=xterm-256color` in that case.
///
/// Unix-only: terminfo is not a concept on Windows.
#[cfg(unix)]
pub fn install() -> Option<PathBuf> {
    if COMPILED_ENTRY.is_empty() {
        return None;
    }

    let root = crate::config::Config::cache_dir().join("terminfo");
    let entry_dir = root.join("c");
    let entry_path = entry_dir.join("casterm");

    fs::create_dir_all(&entry_dir).ok()?;
    fs::write(&entry_path, COMPILED_ENTRY).ok()?;

    Some(root)
}

#[cfg(not(unix))]
pub fn install() -> Option<PathBuf> {
    None
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn embedded_entry_is_compiled_in() {
        assert!(
            !COMPILED_ENTRY.is_empty(),
            "casterm.terminfo was not compiled by build.rs (tic missing in the build image?)"
        );
    }

    #[test]
    fn install_extracts_a_readable_entry() {
        let dir = install().expect("install() should succeed when COMPILED_ENTRY is non-empty");
        let entry_path = dir.join("c").join("casterm");
        let written = fs::read(&entry_path).expect("extracted entry should be readable");
        assert_eq!(written, COMPILED_ENTRY);
    }
}
