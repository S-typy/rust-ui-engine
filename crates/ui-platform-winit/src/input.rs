use rust_desktop_ui_core::{Key, Modifiers, Point, PointerButton};
use winit::{
    event::{MouseButton, MouseScrollDelta},
    keyboard::{Key as NativeKey, KeyCode, NamedKey, PhysicalKey},
};

pub(crate) fn key(key: &NativeKey) -> Key {
    match key {
        NativeKey::Named(named) => match named {
            NamedKey::Backspace => Key::Backspace,
            NamedKey::Alt => Key::Alt,
            NamedKey::F10 => Key::F10,
            NamedKey::F2 => Key::F2,
            NamedKey::PageUp => Key::PageUp,
            NamedKey::PageDown => Key::PageDown,
            NamedKey::Insert => Key::Insert,
            NamedKey::Tab => Key::Tab,
            NamedKey::Enter => Key::Enter,
            NamedKey::Space => Key::Space,
            NamedKey::Escape => Key::Escape,
            NamedKey::Delete => Key::Delete,
            NamedKey::ArrowUp => Key::ArrowUp,
            NamedKey::ArrowDown => Key::ArrowDown,
            NamedKey::ArrowLeft => Key::ArrowLeft,
            NamedKey::ArrowRight => Key::ArrowRight,
            NamedKey::Home => Key::Home,
            NamedKey::End => Key::End,
            _ => Key::Other,
        },
        NativeKey::Character(s) => Key::Character(s.to_string()),
        _ => Key::Other,
    }
}

/// Preserve layout-aware Latin shortcuts, with a physical Latin fallback when
/// a non-Latin layout is active. AltGr and ordinary text never use the fallback.
pub(crate) fn command_key(logical: &NativeKey, physical: PhysicalKey, modifiers: Modifiers) -> Key {
    if (modifiers.control || modifiers.super_key)
        && !modifiers.alt
        && matches!(logical, NativeKey::Character(text) if !text.is_ascii())
        && let PhysicalKey::Code(code) = physical
    {
        use KeyCode::*;
        let letters = [
            KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN,
            KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
        ];
        if let Some(index) = letters.iter().position(|&letter| letter == code) {
            return Key::Character(char::from(b'a' + index as u8).to_string());
        }
    }
    key(logical)
}

pub(crate) fn button(button: MouseButton) -> PointerButton {
    match button {
        MouseButton::Left => PointerButton::Primary,
        MouseButton::Right => PointerButton::Secondary,
        MouseButton::Middle => PointerButton::Middle,
        MouseButton::Back => PointerButton::Other(4),
        MouseButton::Forward => PointerButton::Other(5),
        MouseButton::Other(value) => PointerButton::Other(value),
    }
}

pub(crate) fn wheel(delta: MouseScrollDelta, scale: f64) -> Point {
    if !scale.is_finite() || scale <= 0.0 {
        return Point::ZERO;
    }
    let (x, y) = match delta {
        MouseScrollDelta::LineDelta(x, y) => (-f64::from(x) * 84.0, -f64::from(y) * 84.0),
        MouseScrollDelta::PixelDelta(p) => (-p.x / scale, -p.y / scale),
    };
    let point = Point::new(x as f32, y as f32);
    if point.is_finite() {
        point
    } else {
        Point::ZERO
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn non_latin_shortcuts_preserve_text_altgr_and_latin_layouts() {
        let control = Modifiers {
            control: true,
            ..Default::default()
        };
        let physical = PhysicalKey::Code(KeyCode::KeyA);
        let russian = NativeKey::Character("ф".into());
        assert_eq!(
            command_key(&russian, physical, control),
            Key::Character("a".into())
        );
        assert_eq!(
            command_key(&russian, physical, Modifiers::default()),
            Key::Character("ф".into())
        );
        assert_eq!(
            command_key(
                &russian,
                physical,
                Modifiers {
                    alt: true,
                    ..control
                }
            ),
            Key::Character("ф".into())
        );
        assert_eq!(
            command_key(&NativeKey::Character("q".into()), physical, control),
            Key::Character("q".into())
        );
        assert_eq!(
            command_key(&NativeKey::Named(NamedKey::F2), physical, control),
            Key::F2
        );
    }
    #[test]
    fn pixel_wheel_uses_logical_coordinates_and_keeps_both_axes() {
        let p = winit::dpi::PhysicalPosition::new(30.0, -60.0);
        assert_eq!(
            wheel(MouseScrollDelta::PixelDelta(p), 1.5),
            Point::new(-20.0, 40.0)
        );
        assert_eq!(wheel(MouseScrollDelta::PixelDelta(p), 0.0), Point::ZERO);
        assert_eq!(
            wheel(
                MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(f64::MAX, 0.0)),
                1.0
            ),
            Point::ZERO
        );
    }
    #[test]
    fn edit_and_access_keys_are_not_committed_text() {
        assert_eq!(key(&NativeKey::Named(NamedKey::F2)), Key::F2);
        assert_eq!(key(&NativeKey::Named(NamedKey::Backspace)), Key::Backspace);
        assert_eq!(
            key(&NativeKey::Character("Ж".into())),
            Key::Character("Ж".into())
        );
    }
}
