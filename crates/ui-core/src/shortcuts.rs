//! Keyboard scope and logical shortcut contracts.

use crate::{Key, Modifiers, WidgetId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChord {
    pub key: Key,
    pub modifiers: Modifiers,
}

impl KeyChord {
    pub fn new(key: Key, modifiers: Modifiers) -> Self {
        let key = match key {
            Key::Character(value) => Key::Character(value.to_lowercase()),
            key => key,
        };
        Self { key, modifiers }
    }
    pub(crate) fn matches(&self, key: &Key, modifiers: Modifiers) -> bool {
        Self::new(self.key.clone(), self.modifiers) == Self::new(key.clone(), modifiers)
    }
}

/// Scopes compose along the focus route. The nearest scope has priority.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FocusScope {
    /// Tab wraps within this subtree. Explicit Focus mutations may leave it.
    pub cycle_tab: bool,
    /// Unmatched shortcuts stop here instead of reaching outer registrations.
    pub isolate_shortcuts: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutEvent {
    pub owner: WidgetId,
    pub focused: Option<WidgetId>,
    pub chord: KeyChord,
    pub repeat: bool,
}
