//! Keybinding dispatcher
//!
//! Compiles `config::KeyBindingsConfig` into a chord-sequence lookup table
//! and resolves incoming key events against it, including multi-chord
//! sequences (e.g. the built-in `C-Space n` "next window" binding) and a
//! locked mode that swallows every key except a configured unlock sequence
//! (IDEA.md's "locked mode" requirement).
//!
//! This module only resolves keys to abstract action strings — it does not
//! interpret them. Callers (currently `ui::tui`) own the `match` on action
//! names and decide what each one does, so new actions (multiplexer splits,
//! copy mode, etc.) can be added by callers without changing this module.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyModifiers};

use crate::config::{CopyModeStyle, KeyBindingsConfig};

/// A single key chord: modifiers + key code, normalized so printable
/// characters don't double-count `Shift` (crossterm already encodes shift
/// via the uppercase/alternate `char`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub modifiers: KeyModifiers,
    pub code: KeyCode,
}

impl KeyChord {
    pub fn from_event(modifiers: KeyModifiers, code: KeyCode) -> Self {
        let modifiers = match code {
            KeyCode::Char(_) => modifiers & !KeyModifiers::SHIFT,
            _ => modifiers,
        };
        Self { modifiers, code }
    }

    /// Parse a single chord token, e.g. `"C-b"`, `"M-Enter"`, `"Space"`, `"q"`.
    fn parse(token: &str) -> Option<Self> {
        let mut modifiers = KeyModifiers::NONE;
        let mut rest = token;
        loop {
            if let Some(stripped) = rest.strip_prefix("C-") {
                modifiers |= KeyModifiers::CONTROL;
                rest = stripped;
            } else if let Some(stripped) =
                rest.strip_prefix("M-").or_else(|| rest.strip_prefix("A-"))
            {
                modifiers |= KeyModifiers::ALT;
                rest = stripped;
            } else if let Some(stripped) = rest.strip_prefix("S-") {
                modifiers |= KeyModifiers::SHIFT;
                rest = stripped;
            } else {
                break;
            }
        }

        let code = match rest {
            "Space" => KeyCode::Char(' '),
            "Enter" | "Return" => KeyCode::Enter,
            "Escape" | "Esc" => KeyCode::Esc,
            "Tab" => KeyCode::Tab,
            "Backspace" => KeyCode::Backspace,
            "Delete" | "Del" => KeyCode::Delete,
            "Up" => KeyCode::Up,
            "Down" => KeyCode::Down,
            "Left" => KeyCode::Left,
            "Right" => KeyCode::Right,
            "Home" => KeyCode::Home,
            "End" => KeyCode::End,
            "PageUp" => KeyCode::PageUp,
            "PageDown" => KeyCode::PageDown,
            s if s.len() == 1 => KeyCode::Char(s.chars().next()?),
            s if s.starts_with('F') => KeyCode::F(s[1..].parse().ok()?),
            _ => return None,
        };
        Some(KeyChord { modifiers, code })
    }

    /// Parse a full binding key string (e.g. `"C-Space n"`) into a chord
    /// sequence. Returns `None` if any chord token fails to parse.
    fn parse_sequence(spec: &str) -> Option<Vec<KeyChord>> {
        spec.split_whitespace().map(KeyChord::parse).collect()
    }
}

/// Outcome of feeding one key event into the resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// No binding matches and no sequence is in progress.
    NoMatch,
    /// A prefix of one or more bindings matched; waiting for the next chord.
    Pending,
    /// A full binding matched; the action name is returned for the caller
    /// to interpret.
    Action(String),
}

/// Compiles `KeyBindingsConfig` into a lookup table and tracks in-progress
/// multi-chord sequences.
pub struct KeymapResolver {
    bindings: HashMap<Vec<KeyChord>, String>,
    pending: Vec<KeyChord>,
    locked: bool,
    unlock_sequence: Vec<KeyChord>,
}

impl KeymapResolver {
    /// `file_browser_toggle` is the configured chord spec for the
    /// "toggle-file-browser" action (`config::FileBrowserConfig::keybinding`,
    /// `"C-t"` by default) — passed separately from `KeyBindingsConfig`
    /// since the file-browser panel has its own config section, mirroring
    /// how `copy_mode_style` already selects a variant default binding.
    pub fn new(config: &KeyBindingsConfig, file_browser_toggle: &str) -> Self {
        let mut bindings = HashMap::new();
        for (spec, action) in default_bindings(config.copy_mode_style, file_browser_toggle) {
            if let Some(seq) = KeyChord::parse_sequence(&spec) {
                bindings.insert(seq, action.to_string());
            }
        }
        // User-configured bindings override/extend the defaults.
        for binding in &config.bindings {
            if let Some(seq) = KeyChord::parse_sequence(&binding.key) {
                bindings.insert(seq, binding.action.clone());
            }
        }
        let unlock_sequence = KeyChord::parse_sequence("C-Space u").unwrap_or_default();
        Self {
            bindings,
            pending: Vec::new(),
            locked: false,
            unlock_sequence,
        }
    }

    /// True while a multi-chord sequence is in progress or the resolver is
    /// locked — callers use this to decide whether an unmatched key should
    /// be swallowed (mid-sequence / locked) or sent through as normal input
    /// (fresh key, nothing pending).
    pub fn awaiting_input(&self) -> bool {
        self.locked || !self.pending.is_empty()
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }

    fn lock(&mut self) {
        self.locked = true;
        self.pending.clear();
    }

    fn unlock(&mut self) {
        self.locked = false;
        self.pending.clear();
    }

    /// Feed one key event through the resolver.
    pub fn resolve(&mut self, modifiers: KeyModifiers, code: KeyCode) -> Resolved {
        let chord = KeyChord::from_event(modifiers, code);

        if self.locked {
            let mut candidate = self.pending.clone();
            candidate.push(chord);
            if candidate == self.unlock_sequence {
                self.unlock();
                return Resolved::Action("unlock".to_string());
            }
            if self.unlock_sequence.starts_with(candidate.as_slice()) {
                self.pending = candidate;
                return Resolved::Pending;
            }
            self.pending.clear();
            return Resolved::NoMatch;
        }

        let mut candidate = self.pending.clone();
        candidate.push(chord);

        if let Some(action) = self.bindings.get(&candidate) {
            self.pending.clear();
            let action = action.clone();
            if action == "lock" {
                self.lock();
            }
            return Resolved::Action(action);
        }

        let has_prefix_match = self
            .bindings
            .keys()
            .any(|seq| seq.len() > candidate.len() && seq.starts_with(candidate.as_slice()));
        if has_prefix_match {
            self.pending = candidate;
            return Resolved::Pending;
        }

        self.pending.clear();
        Resolved::NoMatch
    }
}

/// Built-in default bindings. `C-Space` is the prefix key (matching the
/// terminal's historical Ctrl+Space-enters-command-mode behavior); the
/// copy-mode entry chord depends on the configured vi/emacs style. The
/// file-browser toggle is deliberately global/unprefixed (no `C-Space`
/// needed) so it behaves like a normal application shortcut.
fn default_bindings(
    style: CopyModeStyle,
    file_browser_toggle: &str,
) -> Vec<(String, &'static str)> {
    let mut bindings: Vec<(String, &'static str)> = vec![
        ("C-Space n".to_string(), "next-window"),
        ("C-Space p".to_string(), "prev-window"),
        ("C-Space d".to_string(), "detach"),
        ("C-Space q".to_string(), "quit"),
        ("C-Space C-Space".to_string(), "send-literal-prefix"),
        ("C-Space Space".to_string(), "send-literal-prefix"),
        ("C-Space l".to_string(), "lock"),
        ("C-Space \"".to_string(), "split-horizontal"),
        ("C-Space %".to_string(), "split-vertical"),
        ("C-Space x".to_string(), "close-pane"),
        ("C-Space o".to_string(), "focus-next-pane"),
    ];
    match style {
        CopyModeStyle::Vi => bindings.push(("C-Space [".to_string(), "copy-mode")),
        CopyModeStyle::Emacs => bindings.push(("C-Space Escape".to_string(), "copy-mode")),
    }
    if !file_browser_toggle.is_empty() {
        bindings.push((file_browser_toggle.to_string(), "toggle-file-browser"));
    }
    bindings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{KeyBinding, KeyBindingsConfig};

    fn ctrl_space() -> (KeyModifiers, KeyCode) {
        (KeyModifiers::CONTROL, KeyCode::Char(' '))
    }

    #[test]
    fn resolves_default_prefix_sequence() {
        let mut resolver = KeymapResolver::new(&KeyBindingsConfig::default(), "C-t");
        let (m, c) = ctrl_space();
        assert_eq!(resolver.resolve(m, c), Resolved::Pending);
        assert_eq!(
            resolver.resolve(KeyModifiers::NONE, KeyCode::Char('n')),
            Resolved::Action("next-window".to_string())
        );
        assert!(!resolver.awaiting_input());
    }

    #[test]
    fn unmatched_key_after_prefix_is_swallowed_not_passed_through() {
        let mut resolver = KeymapResolver::new(&KeyBindingsConfig::default(), "C-t");
        let (m, c) = ctrl_space();
        resolver.resolve(m, c);
        assert_eq!(
            resolver.resolve(KeyModifiers::NONE, KeyCode::Char('z')),
            Resolved::NoMatch
        );
        // Sequence must have reset, not stayed pending forever.
        assert!(!resolver.awaiting_input());
    }

    #[test]
    fn fresh_unmatched_key_reports_no_match_and_nothing_pending() {
        let mut resolver = KeymapResolver::new(&KeyBindingsConfig::default(), "C-t");
        assert_eq!(
            resolver.resolve(KeyModifiers::NONE, KeyCode::Char('a')),
            Resolved::NoMatch
        );
        assert!(!resolver.awaiting_input());
    }

    #[test]
    fn vi_and_emacs_styles_produce_different_copy_mode_bindings() {
        let vi = KeymapResolver::new(
            &KeyBindingsConfig {
                copy_mode_style: CopyModeStyle::Vi,
                bindings: Vec::new(),
            },
            "C-t",
        );
        assert!(vi
            .bindings
            .contains_key(&KeyChord::parse_sequence("C-Space [").expect("valid sequence")));

        let emacs = KeymapResolver::new(
            &KeyBindingsConfig {
                copy_mode_style: CopyModeStyle::Emacs,
                bindings: Vec::new(),
            },
            "C-t",
        );
        assert!(emacs
            .bindings
            .contains_key(&KeyChord::parse_sequence("C-Space Escape").expect("valid sequence")));
    }

    #[test]
    fn user_binding_overrides_default() {
        let config = KeyBindingsConfig {
            copy_mode_style: CopyModeStyle::Vi,
            bindings: vec![KeyBinding {
                key: "C-Space n".to_string(),
                action: "custom-action".to_string(),
                context: None,
            }],
        };
        let mut resolver = KeymapResolver::new(&config, "C-t");
        let (m, c) = ctrl_space();
        resolver.resolve(m, c);
        assert_eq!(
            resolver.resolve(KeyModifiers::NONE, KeyCode::Char('n')),
            Resolved::Action("custom-action".to_string())
        );
    }

    #[test]
    fn file_browser_toggle_resolves_to_action() {
        let mut resolver = KeymapResolver::new(&KeyBindingsConfig::default(), "C-t");
        assert_eq!(
            resolver.resolve(KeyModifiers::CONTROL, KeyCode::Char('t')),
            Resolved::Action("toggle-file-browser".to_string())
        );
    }

    #[test]
    fn locked_mode_swallows_everything_except_unlock() {
        let mut resolver = KeymapResolver::new(&KeyBindingsConfig::default(), "C-t");
        let (m, c) = ctrl_space();
        resolver.resolve(m, c);
        resolver.resolve(KeyModifiers::NONE, KeyCode::Char('l'));
        assert!(resolver.is_locked());

        assert_eq!(
            resolver.resolve(KeyModifiers::CONTROL, KeyCode::Char('c')),
            Resolved::NoMatch
        );
        assert!(resolver.is_locked());

        let (m, c) = ctrl_space();
        assert_eq!(resolver.resolve(m, c), Resolved::Pending);
        assert_eq!(
            resolver.resolve(KeyModifiers::NONE, KeyCode::Char('u')),
            Resolved::Action("unlock".to_string())
        );
        assert!(!resolver.is_locked());
    }
}
