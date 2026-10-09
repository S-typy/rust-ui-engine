use rust_desktop_ui_core::{Key, Point, PointerButton};
use winit::{
    event::{MouseButton, MouseScrollDelta},
    keyboard::{Key as NativeKey, NamedKey},
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
