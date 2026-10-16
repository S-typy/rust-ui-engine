//! Protected single-line input. Secrets never enter ordinary editor sessions.
use super::*;
use std::time::{Duration, Instant};

/// Hard bounds keep both masking and temporary reveal inexpensive.
pub const MAX_PASSWORD_BYTES: usize = 16_384;
pub const MAX_PASSWORD_GRAPHEMES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasswordBoxSettings {
    pub max_length: usize,
    pub is_read_only: bool,
    pub allow_paste: bool,
    pub allow_reveal: bool,
    pub placeholder: String,
    /// Reveal always ends on blur, disable, detach, input or this timeout.
    pub reveal_duration: Duration,
}
impl Default for PasswordBoxSettings {
    fn default() -> Self {
        Self {
            max_length: 256,
            is_read_only: false,
            allow_paste: true,
            allow_reveal: true,
            placeholder: String::new(),
            reveal_duration: Duration::from_secs(3),
        }
    }
}
impl PasswordBoxSettings {
    pub fn validate(&self) -> Result<(), ControlError> {
        if self.max_length > MAX_PASSWORD_GRAPHEMES
            || self.placeholder.len() > 4096
            || self.reveal_duration.is_zero()
            || self.reveal_duration > Duration::from_secs(10)
        {
            return Err(ControlError::Invalid("Invalid PasswordBox settings".into()));
        }
        Ok(())
    }
    fn limits(&self) -> TextLimits {
        TextLimits {
            max_bytes: MAX_PASSWORD_BYTES,
            max_graphemes: Some(self.max_length),
        }
    }
}

/// Explicit borrowed access. Formatting never reveals the secret.
pub struct PasswordRef<'a>(&'a str);
impl PasswordRef<'_> {
    pub fn expose_secret(&self) -> &str {
        self.0
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
impl fmt::Debug for PasswordRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PasswordRef([REDACTED])")
    }
}
/// Owned, non-cloneable result of taking a password. This type does not claim
/// locked or zeroized memory; the caller controls any explicitly exposed copy.
pub struct PasswordValue(String);
impl PasswordValue {
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
impl fmt::Debug for PasswordValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PasswordValue([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasswordEventKind {
    Changed,
    Submitted,
    Revealed,
    Hidden,
}
/// Contains neither text nor a reconstructable selection or character payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PasswordEvent {
    pub id: WidgetId,
    pub kind: PasswordEventKind,
}

pub(super) struct PasswordState {
    document: TextDocument,
    settings: PasswordBoxSettings,
    reveal_button: Option<WidgetId>,
    revealed_until: Option<Instant>,
    scroll_x: f32,
    display_boundaries: Vec<usize>,
}
impl PasswordState {
    fn new(settings: PasswordBoxSettings) -> Self {
        Self {
            document: TextDocument::default(),
            settings,
            reveal_button: None,
            revealed_until: None,
            scroll_x: 0.0,
            display_boundaries: vec![0],
        }
    }
}

impl Controls {
    /// Create an initially empty protected input with an optional reveal button.
    /// Load a value only through the explicit `set_password_value` API.
    pub fn password_box(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        settings: PasswordBoxSettings,
        mut style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        settings.validate()?;
        style.kind = LayoutKind::Grid;
        style.grid_columns = vec![rust_desktop_ui_core::GridTrack::Fr(1.0)];
        style.grid_rows = vec![rust_desktop_ui_core::GridTrack::Fr(1.0)];
        let id = self.add(
            parent,
            Kind::Password(Box::new(PasswordState::new(settings))),
            label.into(),
            None,
            style,
        )?;
        let button = self.button(
            id,
            "",
            None,
            LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                max_size: Size::new(30.0, f32::INFINITY),
                horizontal_alignment: Some(rust_desktop_ui_core::Align::End),
                ..Default::default()
            },
        )?;
        self.set_button_icon(button, Some(ButtonIcon::new(IconSymbol::Eye)))?;
        self.set_automation_name(button, "Show password temporarily")?;
        self.set_button_chrome(button, ButtonChrome::Flat)?;
        self.password_state_mut(id)?.reveal_button = Some(button);
        self.sync_passwords()?;
        Ok(id)
    }
    fn password_state(&self, id: WidgetId) -> Option<&PasswordState> {
        match &self.controls.get(&id)?.kind {
            Kind::Password(state) => Some(state),
            _ => None,
        }
    }
    fn password_state_mut(&mut self, id: WidgetId) -> Result<&mut PasswordState, ControlError> {
        match &mut self
            .controls
            .get_mut(&id)
            .ok_or_else(|| ControlError::Invalid("Unknown PasswordBox".into()))?
            .kind
        {
            Kind::Password(state) => Ok(state),
            _ => Err(ControlError::Invalid("Control is not a PasswordBox".into())),
        }
    }
    pub fn is_password_box(&self, id: WidgetId) -> bool {
        self.password_state(id).is_some()
    }
    /// Hosts use this before logging native key, text, paste or IME events.
    pub fn password_input_focused(&self) -> bool {
        self.runtime
            .focused()
            .is_some_and(|id| self.is_password_box(id))
    }
    pub fn password_settings(&self, id: WidgetId) -> Option<&PasswordBoxSettings> {
        Some(&self.password_state(id)?.settings)
    }
    pub fn password_composition_active(&self, id: WidgetId) -> Option<bool> {
        Some(self.password_state(id)?.document.composition().is_some())
    }
    pub fn set_password_style(
        &mut self,
        id: WidgetId,
        style: EditBoxStyle,
    ) -> Result<(), ControlError> {
        style.validate()?;
        self.password_state_mut(id)?;
        self.controls.get_mut(&id).unwrap().edit_box_style = style;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn set_password_settings(
        &mut self,
        id: WidgetId,
        settings: PasswordBoxSettings,
    ) -> Result<(), ControlError> {
        settings.validate()?;
        settings
            .limits()
            .validate(
                self.password_state(id)
                    .ok_or_else(|| ControlError::Invalid("Unknown PasswordBox".into()))?
                    .document
                    .text(),
            )
            .map_err(edit_error)?;
        self.hide_password(id)?;
        let state = self.password_state_mut(id)?;
        state.document.cancel_preedit();
        state.settings = settings;
        self.sync_passwords()?;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn password_value(&self, id: WidgetId) -> Option<PasswordRef<'_>> {
        Some(PasswordRef(self.password_state(id)?.document.text()))
    }
    pub fn set_password_value(&mut self, id: WidgetId, value: &str) -> Result<(), ControlError> {
        let state = self
            .password_state(id)
            .ok_or_else(|| ControlError::Invalid("Unknown PasswordBox".into()))?;
        validate_password_input(value)?;
        state
            .settings
            .limits()
            .validate(value)
            .map_err(edit_error)?;
        let changed = state.document.text() != value;
        self.hide_password(id)?;
        let state = self.password_state_mut(id)?;
        state.document = TextDocument::new(value);
        state.scroll_x = 0.0;
        if changed {
            self.password_changed(id);
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub fn take_password_value(&mut self, id: WidgetId) -> Result<PasswordValue, ControlError> {
        let value = self
            .password_state(id)
            .ok_or_else(|| ControlError::Invalid("Unknown PasswordBox".into()))?
            .document
            .text()
            .to_owned();
        self.set_password_value(id, "")?;
        Ok(PasswordValue(value))
    }
    pub fn password_reveal_button(&self, id: WidgetId) -> Option<WidgetId> {
        self.password_state(id)?
            .reveal_button
            .filter(|button| self.runtime.tree().contains(*button))
    }
    pub fn password_is_revealed(&self, id: WidgetId) -> Option<bool> {
        Some(self.password_state(id)?.revealed_until.is_some())
    }
    pub fn reveal_password(&mut self, id: WidgetId) -> Result<bool, ControlError> {
        let state = self
            .password_state(id)
            .ok_or_else(|| ControlError::Invalid("Unknown PasswordBox".into()))?;
        if !state.settings.allow_reveal
            || state.document.text().is_empty()
            || state.document.composition().is_some()
            || !self.is_enabled(id)
            || !self.window_focused
            || self.runtime.focused() != Some(id)
        {
            return Ok(false);
        }
        // A masked secret may contain a grapheme that exceeds shaping budgets.
        // Refuse its visual reveal before changing state; keep the value and
        // masked presentation usable, and never put this preflight in the cache.
        let mut candidate = TextRun::new(
            state.document.text(),
            Rect::new(0.0, 0.0, 16_384.0, 4096.0),
            Color::rgb(0, 0, 0),
            self.controls[&id]
                .edit_box_style
                .resolve(
                    self.theme,
                    EditBoxVisualState::from_flags(true, state.settings.is_read_only, true, false),
                )
                .font_size,
        );
        candidate.sensitive = true;
        let duration = state.settings.reveal_duration;
        let changed = state.revealed_until.is_none();
        if self.engine.validate_run(&candidate, 1.0).is_err() {
            return Ok(false);
        }
        let until = Instant::now() + duration;
        self.password_state_mut(id)?.revealed_until = Some(until);
        if changed {
            self.password_events.push(PasswordEvent {
                id,
                kind: PasswordEventKind::Revealed,
            });
        }
        self.scene_dirty = true;
        Ok(true)
    }
    pub fn hide_password(&mut self, id: WidgetId) -> Result<(), ControlError> {
        if self.password_state_mut(id)?.revealed_until.take().is_some() {
            self.password_events.push(PasswordEvent {
                id,
                kind: PasswordEventKind::Hidden,
            });
            // Previously retained frame commands must no longer expose the value.
            self.scene.clear();
            self.base_scene.clear();
            self.overlay_scene.clear();
            self.text_layouts.remove(&id);
            self.scene_dirty = true;
        }
        Ok(())
    }
    pub fn drain_password_events(&mut self) -> Vec<PasswordEvent> {
        std::mem::take(&mut self.password_events)
    }
    /// Clipboard insertion has a distinct policy from typed text.
    pub fn paste_input(&mut self, text: &str) -> Result<(), ControlError> {
        if let Some(id) = self
            .runtime
            .focused()
            .filter(|id| self.is_password_box(*id))
            && !self
                .password_settings(id)
                .is_some_and(|settings| settings.allow_paste)
        {
            return Ok(());
        }
        self.text_input(text)
    }
    fn password_changed(&mut self, id: WidgetId) {
        self.password_events.push(PasswordEvent {
            id,
            kind: PasswordEventKind::Changed,
        });
        self.caret_visible = true;
        self.scene_dirty = true;
    }
    pub(super) fn password_text_input(
        &mut self,
        text: &str,
        ime_commit: bool,
    ) -> Result<bool, ControlError> {
        let Some(id) = self
            .runtime
            .focused()
            .filter(|id| self.is_password_box(*id))
        else {
            return Ok(false);
        };
        if !self.is_enabled(id) || self.password_state(id).unwrap().settings.is_read_only {
            return Ok(true);
        }
        self.hide_password(id)?;
        let state = self.password_state_mut(id)?;
        if text.is_empty() {
            if ime_commit {
                state.document.cancel_preedit();
            }
            return Ok(true);
        }
        if validate_password_input(text).is_err() {
            if ime_commit {
                state.document.cancel_preedit();
            }
            self.scene_dirty = true;
            return Ok(true);
        }
        let before = state.document.text().to_owned();
        let result = if ime_commit {
            state.document.try_commit(text, state.settings.limits())
        } else {
            state.document.try_insert(text, state.settings.limits())
        };
        if ime_commit {
            state.document.cancel_preedit();
        }
        state.document.clear_history();
        let changed = before != state.document.text();
        user_edit(result)?;
        if changed {
            self.password_changed(id);
        }
        self.scene_dirty = true;
        Ok(true)
    }
    pub(super) fn password_preedit(
        &mut self,
        text: &str,
        cursor: Option<(usize, usize)>,
    ) -> Result<bool, ControlError> {
        let Some(id) = self
            .runtime
            .focused()
            .filter(|id| self.is_password_box(*id))
        else {
            return Ok(false);
        };
        if !self.is_enabled(id) || self.password_state(id).unwrap().settings.is_read_only {
            return Ok(true);
        }
        self.hide_password(id)?;
        let state = self.password_state_mut(id)?;
        if validate_password_input(text).is_err() {
            state.document.cancel_preedit();
            self.scene_dirty = true;
            return Ok(true);
        }
        let result = state
            .document
            .try_set_preedit(text, cursor, state.settings.limits());
        if result.is_err() {
            state.document.cancel_preedit();
        }
        self.scene_dirty = true;
        user_edit(result)?;
        Ok(true)
    }
    pub(super) fn password_cancel_preedit(&mut self) {
        for control in self.controls.values_mut() {
            if let Kind::Password(state) = &mut control.kind {
                state.document.cancel_preedit();
            }
        }
    }
    pub(super) fn password_deadline(&self) -> Option<Instant> {
        self.controls
            .values()
            .filter_map(|control| match &control.kind {
                Kind::Password(state) => state.revealed_until,
                _ => None,
            })
            .min()
    }
    pub(super) fn password_tick(&mut self, now: Instant) -> Result<bool, ControlError> {
        let ids: Vec<_> = self
            .controls
            .iter()
            .filter_map(|(&id, control)| match &control.kind {
                Kind::Password(state) if state.revealed_until.is_some_and(|until| now >= until) => {
                    Some(id)
                }
                _ => None,
            })
            .collect();
        let changed = !ids.is_empty();
        for id in ids {
            self.hide_password(id)?;
        }
        Ok(changed)
    }
    pub(super) fn sync_passwords(&mut self) -> Result<(), ControlError> {
        let ids: Vec<_> = self
            .controls
            .keys()
            .copied()
            .filter(|id| self.is_password_box(*id))
            .collect();
        for id in ids {
            let button = self.password_reveal_button(id);
            let focus_inside = self.runtime.focused() == Some(id)
                || self
                    .runtime
                    .focused()
                    .is_some_and(|focus| Some(focus) == button);
            let enabled = self.is_enabled(id);
            if !enabled || !self.window_focused || !focus_inside {
                self.hide_password(id)?;
                let state = self.password_state_mut(id)?;
                if state.document.composition().is_some() {
                    state.document.cancel_preedit();
                    self.scene_dirty = true;
                }
            }
            if let Some(button) = button {
                let allowed = self.password_state(id).unwrap().settings.allow_reveal;
                let revealed = self.password_is_revealed(id) == Some(true);
                if self.runtime.tree().node(button).is_some_and(|node| {
                    node.props.visible != allowed || node.props.collapsed == allowed
                }) {
                    self.edit_props(button, |props| {
                        props.visible = allowed;
                        props.collapsed = !allowed;
                    })?;
                }
                let label = if revealed {
                    "Hide password"
                } else {
                    "Show password temporarily"
                };
                if self
                    .controls
                    .get(&button)
                    .and_then(|control| control.automation_name.as_deref())
                    != Some(label)
                {
                    self.set_automation_name(button, label)?;
                }
            }
        }
        self.password_events
            .retain(|event| self.runtime.tree().contains(event.id));
        Ok(())
    }
    pub(super) fn activate_password(&mut self, button: WidgetId) -> Result<bool, ControlError> {
        let owner = self
            .controls
            .iter()
            .find_map(|(&id, control)| match &control.kind {
                Kind::Password(state) if state.reveal_button == Some(button) => Some(id),
                _ => None,
            });
        let Some(id) = owner else { return Ok(false) };
        self.runtime.apply(Mutation::Focus(Some(id)))?;
        if self.password_is_revealed(id) == Some(true) {
            self.hide_password(id)?;
        } else {
            self.reveal_password(id)?;
        }
        Ok(true)
    }
    pub(super) fn password_before_input(
        &mut self,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        let focused = self
            .runtime
            .focused()
            .filter(|id| self.is_password_box(*id));
        let hit = match event {
            InputEvent::PointerDown { position, .. } => self.runtime.hit_test(*position),
            _ => None,
        };
        let pointer_owner = hit.filter(|id| self.is_password_box(*id));
        let selecting = self.selecting.filter(|id| self.is_password_box(*id));
        let handled = match event {
            InputEvent::KeyDown { key, .. } => {
                focused.is_some()
                    && matches!(
                        key,
                        Key::Backspace
                            | Key::Delete
                            | Key::Home
                            | Key::End
                            | Key::ArrowLeft
                            | Key::ArrowRight
                            | Key::Enter
                            | Key::Escape
                            | Key::Insert
                            | Key::Character(_)
                    )
            }
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                ..
            } => pointer_owner.is_some(),
            InputEvent::PointerMoved { .. }
            | InputEvent::PointerUp {
                button: PointerButton::Primary,
                ..
            } => selecting.is_some(),
            _ => false,
        };
        if !handled {
            return Ok(false);
        }
        let report = self.runtime.dispatch(event.clone());
        self.prune_removed_controls()?;
        if let Some(error) = report.errors.into_iter().next() {
            return Err(error.into());
        }
        if report.default_prevented {
            if let InputEvent::KeyDown {
                key: Key::Character(text),
                modifiers,
                ..
            } = event
                && !modifiers.control
                && !modifiers.super_key
                && !modifiers.alt
            {
                self.suppressed_key_text = Some(text.clone());
            }
            return Ok(true);
        }
        match event {
            InputEvent::PointerDown {
                position,
                modifiers,
                ..
            } => {
                if let Some(id) = pointer_owner
                    .filter(|id| self.is_enabled(*id) && self.runtime.focused() == Some(*id))
                {
                    self.password_place_caret(id, *position, modifiers.shift)?;
                    self.selecting = Some(id);
                    self.runtime.apply(Mutation::CapturePointer(Some(id)))?;
                }
            }
            InputEvent::PointerMoved { position } => {
                if let Some(id) = selecting {
                    self.password_place_caret(id, *position, true)?;
                }
            }
            InputEvent::PointerUp { .. } => {
                self.selecting = None;
                self.runtime.apply(Mutation::CapturePointer(None))?;
            }
            InputEvent::KeyDown {
                key,
                modifiers,
                repeat,
            } => {
                if let Some(id) = focused.filter(|id| self.is_enabled(*id)) {
                    self.password_key(id, key, *modifiers, *repeat)?;
                }
            }
            _ => {}
        }
        self.synchronize_editor_focus()?;
        self.sync_passwords()?;
        self.scene_dirty = true;
        Ok(true)
    }
    fn password_key(
        &mut self,
        id: WidgetId,
        key: &Key,
        modifiers: rust_desktop_ui_core::Modifiers,
        repeat: bool,
    ) -> Result<(), ControlError> {
        let shortcut = (modifiers.control && !modifiers.alt) || modifiers.super_key;
        let state = self.password_state_mut(id)?;
        if *key == Key::Escape {
            state.document.cancel_preedit();
            self.hide_password(id)?;
            return Ok(());
        }
        if state.document.composition().is_some() {
            return Ok(());
        }
        if *key == Key::Enter && !repeat && !shortcut && !modifiers.alt && !modifiers.shift {
            self.hide_password(id)?;
            self.password_events.push(PasswordEvent {
                id,
                kind: PasswordEventKind::Submitted,
            });
            return Ok(());
        }
        if let Key::Character(character) = key {
            if shortcut {
                match character.to_ascii_lowercase().as_str() {
                    "a" => state.document.select_all(),
                    "v" if state.settings.allow_paste
                        && !state.settings.is_read_only
                        && !repeat =>
                    {
                        self.clipboard.push(ClipboardRequest::Paste)
                    }
                    // Copy, cut and history shortcuts never expose or restore text.
                    _ => {}
                }
            }
            return Ok(());
        }
        if *key == Key::Insert
            && modifiers.shift
            && state.settings.allow_paste
            && !state.settings.is_read_only
            && !repeat
        {
            self.clipboard.push(ClipboardRequest::Paste);
            return Ok(());
        }
        let before = state.document.text().to_owned();
        match key {
            Key::ArrowLeft => {
                if shortcut {
                    state.document.move_word_left(modifiers.shift)
                } else {
                    state.document.move_left(modifiers.shift)
                }
            }
            Key::ArrowRight => {
                if shortcut {
                    state.document.move_word_right(modifiers.shift)
                } else {
                    state.document.move_right(modifiers.shift)
                }
            }
            Key::Home => state.document.move_document_start(modifiers.shift),
            Key::End => state.document.move_document_end(modifiers.shift),
            Key::Backspace if !state.settings.is_read_only => {
                if shortcut {
                    state.document.delete_word_backward();
                } else {
                    state.document.backspace();
                }
            }
            Key::Delete if !state.settings.is_read_only && !modifiers.shift => {
                if shortcut {
                    state.document.delete_word_forward();
                } else {
                    state.document.delete_forward();
                }
            }
            _ => {}
        }
        state.document.clear_history();
        if state.document.text() != before {
            self.hide_password(id)?;
            self.password_changed(id);
        }
        Ok(())
    }
    fn password_place_caret(
        &mut self,
        id: WidgetId,
        point: Point,
        extend: bool,
    ) -> Result<(), ControlError> {
        let Some(layout) = self.text_layouts.get(&id) else {
            return Ok(());
        };
        let displayed = layout.hit_test_caret(point).index;
        let state = self.password_state_mut(id)?;
        if state.document.composition().is_some() {
            state.document.cancel_preedit();
            return Ok(());
        }
        let index = if state.revealed_until.is_some() {
            displayed
        } else {
            *state
                .display_boundaries
                .get(displayed / '•'.len_utf8())
                .unwrap_or(&state.document.text().len())
        };
        let anchor = if extend {
            state.document.selection().anchor
        } else {
            index
        };
        state
            .document
            .set_selection(anchor, index)
            .map_err(edit_error)?;
        Ok(())
    }
    pub(super) fn password_caret_bounds(&self, id: WidgetId) -> Option<Rect> {
        let state = self.password_state(id)?;
        let raw = state.document.display_selection().focus;
        let index = if state.revealed_until.is_some() {
            raw
        } else {
            state
                .display_boundaries
                .partition_point(|index| *index < raw)
                * '•'.len_utf8()
        };
        self.text_layouts
            .get(&id)
            .map(|layout| layout.caret_rect(index))
    }
    pub(super) fn paint_password(
        &mut self,
        id: WidgetId,
        bounds: Rect,
        clip: Rect,
    ) -> Result<bool, ControlError> {
        if !self.is_password_box(id) {
            return Ok(false);
        }
        let enabled = self.is_enabled(id);
        let focused = self.runtime.focused() == Some(id);
        let control = self.controls.get_mut(&id).unwrap();
        let Kind::Password(state) = &mut control.kind else {
            unreachable!()
        };
        let style = control.edit_box_style.resolve(
            self.theme,
            EditBoxVisualState::from_flags(
                enabled,
                state.settings.is_read_only,
                focused,
                self.runtime.hovered() == Some(id),
            ),
        );
        editor_fill(
            &mut self.scene,
            bounds,
            clip,
            style.corner_radius,
            style.background,
        );
        self.scene.rounded_stroke(
            bounds,
            clip,
            style.corner_radius,
            style.border_width,
            style.border,
        );
        if focused {
            fill_clipped(
                &mut self.scene,
                Rect::new(
                    bounds.x + 4.0,
                    bounds.y + bounds.height - 2.0,
                    (bounds.width - 8.0).max(0.0),
                    2.0,
                ),
                clip,
                style.accent,
            );
        }
        let inset = style.padding;
        let content = Rect::new(
            bounds.x + inset.left,
            bounds.y + inset.top,
            (bounds.width
                - inset.horizontal()
                - if state.settings.allow_reveal {
                    30.0
                } else {
                    0.0
                })
            .max(0.0),
            (bounds.height - inset.vertical()).max(0.0),
        );
        let text_clip = content
            .intersection(clip)
            .unwrap_or(Rect::new(content.x, content.y, 0.0, 0.0));
        let display = state.document.display_text();
        state.display_boundaries = TextDocument::new(&display).grapheme_boundaries().to_vec();
        let revealed = state.revealed_until.is_some();
        let empty = display.is_empty();
        let text = if empty {
            state.settings.placeholder.clone()
        } else if revealed {
            display
        } else {
            "•".repeat(state.display_boundaries.len().saturating_sub(1))
        };
        let mut run = TextRun::new(
            text,
            content,
            if empty {
                style.placeholder
            } else {
                style.foreground
            },
            style.font_size,
        );
        run.sensitive = revealed;
        run.clip = text_clip;
        let measured = self
            .engine
            .prepare(&run, 1.0)
            .map_err(|error| ControlError::Text(error.to_string()))?;
        let raw_selection = state.document.display_selection();
        let index = |byte| {
            if revealed {
                byte
            } else {
                state
                    .display_boundaries
                    .partition_point(|index| *index < byte)
                    * '•'.len_utf8()
            }
        };
        let focus = if empty { 0 } else { index(raw_selection.focus) };
        let caret = measured.caret_rect(focus);
        if focused {
            if caret.x - state.scroll_x < content.x {
                state.scroll_x = (caret.x - content.x).max(0.0);
            }
            if caret.x - state.scroll_x > content.x + content.width - 2.0 {
                state.scroll_x = (caret.x - content.x - content.width + 2.0).max(0.0);
            }
        }
        state.scroll_x = state
            .scroll_x
            .min((measured.width - content.width + 2.0).max(0.0));
        run.bounds.x -= state.scroll_x;
        run.bounds.width += state.scroll_x;
        let dy = ((content.height - measured.height) * 0.5).max(0.0);
        run.bounds.y += dy;
        run.bounds.height = (content.height - dy).max(0.0);
        let layout = self
            .engine
            .prepare(&run, 1.0)
            .map_err(|error| ControlError::Text(error.to_string()))?;
        if focused && !empty {
            let a = index(raw_selection.anchor);
            let b = index(raw_selection.focus);
            for rect in layout.selection_rects(a.min(b)..a.max(b)) {
                fill_clipped(&mut self.scene, rect, text_clip, style.selection);
            }
        }
        if focused && self.caret_visible && self.window_focused {
            fill_clipped(
                &mut self.scene,
                layout.caret_rect(focus),
                text_clip,
                style.accent,
            );
        }
        self.scene.text(run);
        self.text_layouts.insert(id, layout);
        Ok(true)
    }
}

fn validate_password_input(value: &str) -> Result<(), ControlError> {
    if value
        .chars()
        .any(|character| character.is_control() || matches!(character, '\u{2028}' | '\u{2029}'))
    {
        Err(ControlError::Invalid(
            "Password must be a single line without control characters".into(),
        ))
    } else {
        Ok(())
    }
}
