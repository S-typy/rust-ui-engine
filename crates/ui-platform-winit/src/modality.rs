//! Native enable state mirrors the host's portable modal event routing.

/// Invalidate host input state even when a later buffered native blur will be
/// discarded as redundant. Application composition is untouched until its own
/// focus-loss policy runs; its IME cancellation is delivered afterwards.
pub(crate) fn suspend_input(
    focused: &mut bool,
    modifiers: &mut rust_desktop_ui_core::Modifiers,
    ime: &mut crate::ime::ImeSession,
) -> bool {
    *modifiers = rust_desktop_ui_core::Modifiers::default();
    ime.set_target(None, false);
    std::mem::replace(focused, false)
}

pub(crate) fn set_enabled(window: &winit::window::Window, enabled: bool) {
    #[cfg(target_os = "windows")]
    {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(handle) = window.window_handle()
            && let RawWindowHandle::Win32(handle) = handle.as_raw()
        {
            // Window is retained by the caller; no global focus/input is changed.
            unsafe {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow(
                    handle.hwnd.get() as *mut _,
                    enabled as i32,
                );
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = (window, enabled);
}

pub(crate) fn is_user_input(event: &winit::event::WindowEvent) -> bool {
    use winit::event::WindowEvent;
    matches!(
        event,
        WindowEvent::KeyboardInput { .. }
            | WindowEvent::ModifiersChanged(_)
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::CursorMoved { .. }
            | WindowEvent::Ime(_)
            | WindowEvent::Touch(_)
            | WindowEvent::Focused(true)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_core::{Modifiers, UiTree};

    #[test]
    fn manual_blur_resets_modifiers_and_rejects_stale_ime_until_reactivation() {
        let target = UiTree::new().root();
        let mut focused = true;
        let mut modifiers = Modifiers {
            shift: true,
            control: true,
            alt: true,
            super_key: true,
        };
        let mut ime = crate::ime::ImeSession::default();
        ime.set_target(Some(target), true);
        ime.enabled();
        assert!(ime.preedit(true));
        assert!(suspend_input(&mut focused, &mut modifiers, &mut ime));
        assert!(!focused);
        assert_eq!(modifiers, Modifiers::default());
        assert!(!ime.allowed && !ime.composing);
        ime.enabled(); // Delayed notification from the old native context.
        assert!(!ime.commit() && !ime.preedit(true));
        assert!(!suspend_input(&mut focused, &mut modifiers, &mut ime));
        ime.set_target(Some(target), true);
        assert!(!ime.commit());
        ime.enabled();
        assert!(ime.commit());
    }
}
