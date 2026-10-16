use rust_desktop_ui_core::{Rect, WidgetId};

/// Never pass malformed geometry or an unidentified editor into the OS IME.
pub(crate) fn valid_cursor(target: Option<WidgetId>, cursor: Option<Rect>) -> Option<Rect> {
    target?;
    cursor.filter(|r| {
        r.is_valid()
            && r.height > 0.0
            && (r.x + r.width).is_finite()
            && (r.y + r.height).is_finite()
    })
}

/// Native IME events have no editor ID. Disable/re-enable the OS context when
/// focus changes and reject delayed preedit/commit until its new Enabled event.
#[derive(Default)]
pub(crate) struct ImeSession {
    target: Option<WidgetId>,
    pub allowed: bool,
    ready: bool,
    pub composing: bool,
}

impl ImeSession {
    pub fn set_target(&mut self, target: Option<WidgetId>, allowed: bool) -> bool {
        let allowed = allowed && target.is_some();
        if self.target == target && self.allowed == allowed {
            return false;
        }
        self.target = target;
        self.allowed = allowed;
        self.ready = false;
        self.composing = false;
        true
    }
    pub fn enabled(&mut self) {
        self.ready = self.allowed;
    }
    pub fn disabled(&mut self) {
        self.ready = false;
        self.composing = false;
    }
    pub fn cancelled_by_editor(&mut self, active: Option<bool>) -> bool {
        if self.composing && active == Some(false) {
            self.disabled();
            true
        } else {
            false
        }
    }
    pub fn preedit(&mut self, nonempty: bool) -> bool {
        if !self.ready {
            return false;
        }
        self.composing = nonempty;
        true
    }
    pub fn commit(&mut self) -> bool {
        if !self.ready {
            return false;
        }
        self.composing = false;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unidentified_editor_and_invalid_caret_never_enable_native_composition() {
        let id = rust_desktop_ui_core::UiTree::new().root();
        let good = Rect::new(10.0, 20.0, 0.0, 24.0);
        assert_eq!(valid_cursor(Some(id), Some(good)), Some(good));
        assert!(valid_cursor(None, Some(good)).is_none());
        for rect in [
            Rect::new(f32::NAN, 0.0, 1.0, 24.0),
            Rect::new(0.0, 0.0, 1.0, 0.0),
            Rect::new(0.0, 0.0, -1.0, 24.0),
            Rect::new(f32::MAX, 0.0, f32::MAX, 24.0),
        ] {
            assert!(valid_cursor(Some(id), Some(rect)).is_none());
        }
        let mut session = ImeSession::default();
        session.set_target(None, true);
        session.enabled();
        assert!(!session.preedit(true));
        assert!(!session.commit());
        session.set_target(Some(id), true);
        session.enabled();
        assert!(session.preedit(true));
        session.set_target(None, true);
        session.enabled();
        assert!(!session.commit());
    }

    #[test]
    fn independent_windows_retain_their_own_target_epochs() {
        let first = rust_desktop_ui_core::UiTree::new().root();
        let second = rust_desktop_ui_core::UiTree::new().root();
        let mut a = ImeSession::default();
        let mut b = ImeSession::default();
        a.set_target(Some(first), true);
        a.enabled();
        b.set_target(Some(second), false);
        assert!(a.preedit(true));
        assert!(!b.preedit(true));
        a.set_target(Some(first), false);
        b.set_target(Some(second), true);
        b.enabled();
        assert!(!a.commit());
        assert!(b.preedit(true));
        assert!(!a.composing);
        assert!(b.commit());
    }
    #[test]
    fn switching_fields_rejects_stale_composition_until_native_reactivation() {
        let a = rust_desktop_ui_core::UiTree::new().root();
        let b = rust_desktop_ui_core::UiTree::new().root();
        let mut session = ImeSession::default();
        assert!(session.set_target(Some(a), true));
        assert!(!session.commit());
        session.enabled();
        assert!(session.preedit(true));
        assert!(session.composing);
        assert!(session.set_target(Some(b), true));
        assert!(!session.composing);
        assert!(!session.preedit(true));
        assert!(!session.commit());
        session.disabled();
        session.enabled();
        assert!(session.commit());
        assert!(session.set_target(Some(b), false));
        session.enabled();
        assert!(!session.commit());
        assert!(!session.set_target(Some(b), false));
    }
    #[test]
    fn moving_the_caret_within_one_field_invalidates_the_native_composition() {
        let field = rust_desktop_ui_core::UiTree::new().root();
        let mut session = ImeSession::default();
        session.set_target(Some(field), true);
        session.enabled();
        session.preedit(true);
        assert!(!session.set_target(Some(field), true));
        assert!(session.cancelled_by_editor(Some(false)));
        assert!(!session.commit());
        assert!(!session.preedit(true));
        session.enabled();
        assert!(session.preedit(true));
        assert!(session.commit());
    }
}
