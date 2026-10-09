use rust_desktop_ui_core::WidgetId;

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
