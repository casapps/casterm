//! winit keyboard/modifier translation into the crossterm-shaped key
//! representation `render_model::encode_key` and `KeymapResolver` both
//! expect, so the GUI and TUI event loops feed the exact same codepath.

use crossterm::event::{KeyCode, KeyModifiers};
use winit::keyboard::{Key, ModifiersState, NamedKey};

/// Translate a winit logical key into a crossterm `KeyCode`. Returns `None`
/// for keys with no crossterm equivalent (media keys, dead keys, etc.) —
/// callers should simply ignore those.
pub fn to_key_code(key: &Key) -> Option<KeyCode> {
    match key {
        Key::Character(s) => s.chars().next().map(KeyCode::Char),
        Key::Named(NamedKey::Space) => Some(KeyCode::Char(' ')),
        Key::Named(named) => named_to_key_code(*named),
        _ => None,
    }
}

fn named_to_key_code(named: NamedKey) -> Option<KeyCode> {
    Some(match named {
        NamedKey::Enter => KeyCode::Enter,
        NamedKey::Backspace => KeyCode::Backspace,
        NamedKey::Delete => KeyCode::Delete,
        NamedKey::Escape => KeyCode::Esc,
        NamedKey::Tab => KeyCode::Tab,
        NamedKey::ArrowUp => KeyCode::Up,
        NamedKey::ArrowDown => KeyCode::Down,
        NamedKey::ArrowLeft => KeyCode::Left,
        NamedKey::ArrowRight => KeyCode::Right,
        NamedKey::Home => KeyCode::Home,
        NamedKey::End => KeyCode::End,
        NamedKey::PageUp => KeyCode::PageUp,
        NamedKey::PageDown => KeyCode::PageDown,
        NamedKey::F1 => KeyCode::F(1),
        NamedKey::F2 => KeyCode::F(2),
        NamedKey::F3 => KeyCode::F(3),
        NamedKey::F4 => KeyCode::F(4),
        NamedKey::F5 => KeyCode::F(5),
        NamedKey::F6 => KeyCode::F(6),
        NamedKey::F7 => KeyCode::F(7),
        NamedKey::F8 => KeyCode::F(8),
        NamedKey::F9 => KeyCode::F(9),
        NamedKey::F10 => KeyCode::F(10),
        NamedKey::F11 => KeyCode::F(11),
        NamedKey::F12 => KeyCode::F(12),
        _ => return None,
    })
}

/// Translate winit's modifier state into crossterm's bitflags.
pub fn to_key_modifiers(state: ModifiersState) -> KeyModifiers {
    let mut mods = KeyModifiers::NONE;
    if state.control_key() {
        mods |= KeyModifiers::CONTROL;
    }
    if state.alt_key() {
        mods |= KeyModifiers::ALT;
    }
    if state.shift_key() {
        mods |= KeyModifiers::SHIFT;
    }
    mods
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::SmolStr;

    #[test]
    fn character_key_maps_to_char() {
        let key = Key::Character(SmolStr::new("a"));
        assert_eq!(to_key_code(&key), Some(KeyCode::Char('a')));
    }

    #[test]
    fn named_keys_map_to_expected_codes() {
        assert_eq!(
            to_key_code(&Key::Named(NamedKey::Enter)),
            Some(KeyCode::Enter)
        );
        assert_eq!(
            to_key_code(&Key::Named(NamedKey::Escape)),
            Some(KeyCode::Esc)
        );
        assert_eq!(
            to_key_code(&Key::Named(NamedKey::ArrowUp)),
            Some(KeyCode::Up)
        );
        assert_eq!(to_key_code(&Key::Named(NamedKey::F5)), Some(KeyCode::F(5)));
    }

    #[test]
    fn unmapped_named_key_returns_none() {
        assert_eq!(to_key_code(&Key::Named(NamedKey::MediaPlay)), None);
    }

    #[test]
    fn modifiers_translate_all_three_flags() {
        let state = ModifiersState::CONTROL | ModifiersState::ALT | ModifiersState::SHIFT;
        let mods = to_key_modifiers(state);
        assert!(mods.contains(KeyModifiers::CONTROL));
        assert!(mods.contains(KeyModifiers::ALT));
        assert!(mods.contains(KeyModifiers::SHIFT));
    }

    #[test]
    fn no_modifiers_translates_to_none() {
        assert_eq!(
            to_key_modifiers(ModifiersState::empty()),
            KeyModifiers::NONE
        );
    }
}
