use crate::{
    accessibility::Accessibility,
    view::{MountedView, edges},
};
use rust_desktop_ui_controls::{CommitOutcome, Controls, EditEvent, Role, Theme};
use rust_desktop_ui_core::{
    Axis, LayoutKind, LayoutStyle, Length, Mutation, Rect, Scene, Size, WidgetId,
};
use rust_desktop_ui_platform_winit::{
    ClipboardRequest, DesktopApp, EventResponse, PlatformEvent, accesskit,
};
use rust_desktop_ui_xaml::WindowDefinition;
use std::{collections::HashMap, time::Duration};

/// The mounted XAML tree plus the example's explicit Message/Confirm behavior.
pub(crate) struct Form {
    pub(crate) ui: Controls,
    pub(crate) view: MountedView,
    pub(crate) accessible: Accessibility,
    title: String,
    status_text: String,
    last_notice: &'static str,
    checked: HashMap<WidgetId, bool>,
    last_focus: Option<WidgetId>,
    diagnostics: bool,
}

impl Form {
    pub(crate) fn new(definition: WindowDefinition, diagnostics: bool) -> Result<Self, String> {
        let theme = match definition.theme {
            rust_desktop_ui_xaml::Theme::Light => Theme::FluentLight,
            rust_desktop_ui_xaml::Theme::Dark => Theme::FluentDark,
        };
        let mut ui = Controls::new(theme).map_err(err)?;
        // Application-owned registration; the generic mount never invents handlers.
        ui.commands_mut().register("Confirm", "Подтвердить");
        ui.runtime
            .apply(Mutation::SetStyle {
                node: ui.root(),
                style: LayoutStyle {
                    kind: LayoutKind::Stack(Axis::Vertical),
                    // The form is the viewport host. Wide rows may clip inside
                    // it, but must not enlarge their Overlay parent.
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    padding: edges(definition.padding),
                    ..Default::default()
                },
            })
            .map_err(err)?;
        let root = ui.root();
        let view = MountedView::mount(&mut ui, root, &definition.content)?;
        for (name, expected) in [
            ("Message", Role::TextBox),
            ("Option", Role::CheckBox),
            ("Confirm", Role::Button),
            ("Help", Role::Label),
            ("Status", Role::Label),
        ] {
            if let Some(id) = view.find_name(name)
                && view.roles.get(&id) != Some(&expected)
            {
                return Err(format!("The example name '{name}' requires {expected:?}"));
            }
        }
        if view.commands.iter().any(|command| command == "Confirm")
            && view
                .find_name("Message")
                .is_none_or(|id| ui.document(id).is_none())
        {
            return Err("Confirm requires a TextBox named Message".into());
        }
        // Reflect either declarative read-only request in both controls. An
        // unchecked default must not discard an explicitly read-only editor.
        if let (Some(option), Some(message)) = (view.find_name("Option"), view.find_name("Message"))
        {
            let mut settings = ui
                .editor_settings(message)
                .expect("checked Message type")
                .clone();
            settings.is_read_only |= ui.checked(option).expect("checked Option type");
            ui.set_checked(option, settings.is_read_only).map_err(err)?;
            ui.set_editor_settings(message, settings).map_err(err)?;
        }
        let checked = view
            .checks
            .iter()
            .chain(&view.toggles)
            .filter_map(|id| ui.checked(*id).map(|checked| (*id, checked)))
            .collect();
        ui.drain_text_changes();
        ui.drain_edit_events();
        Ok(Self {
            ui,
            view,
            accessible: Accessibility::default(),
            title: definition.title,
            status_text: String::new(),
            last_notice: "Изменений нет.",
            checked,
            last_focus: None,
            diagnostics,
        })
    }
    pub(crate) fn find_name(&self, name: &str) -> Option<WidgetId> {
        self.view.find_name(name)
    }
    fn message(&self) -> Option<WidgetId> {
        self.find_name("Message")
            .filter(|id| self.ui.document(*id).is_some())
    }
    #[cfg(test)]
    pub(crate) fn editor(&self) -> WidgetId {
        self.message()
            .or_else(|| self.view.editors.first().copied())
            .expect("test form has an editor")
    }
    #[cfg(test)]
    pub(crate) fn text(&self) -> &str {
        self.ui
            .document(self.editor())
            .expect("test editor exists")
            .text()
    }

    fn current_status(&self) -> &str {
        self.message()
            .and_then(|id| self.ui.validation_error(id))
            .unwrap_or_else(|| {
                if self.message().and_then(|id| self.ui.is_edit_dirty(id)) == Some(true) {
                    "Есть неподтверждённые изменения."
                } else {
                    self.last_notice
                }
            })
    }
    fn refresh_status(&mut self) -> Result<bool, String> {
        if self.message().is_none() {
            return Ok(false);
        }
        let Some(status) = self.find_name("Status") else {
            return Ok(false);
        };
        let text = self.current_status();
        if self.status_text == text {
            return Ok(false);
        }
        self.status_text = text.to_owned();
        self.ui.set_label(status, &self.status_text).map_err(err)?;
        Ok(true)
    }
    fn consume_edit_events(&mut self) {
        for event in self.ui.drain_edit_events() {
            let id = match &event {
                EditEvent::Committed { id, .. }
                | EditEvent::Canceled { id }
                | EditEvent::ValidationFailed { id, .. } => *id,
            };
            if self.message() == Some(id) {
                match &event {
                    EditEvent::Committed { value, .. } => {
                        self.last_notice = "Значение подтверждено.";
                        if self.diagnostics {
                            println!("edit-committed={value:?}");
                        }
                    }
                    EditEvent::Canceled { .. } => {
                        self.last_notice = "Изменения отменены.";
                        if self.diagnostics {
                            println!("edit-canceled=true");
                        }
                    }
                    EditEvent::ValidationFailed { message, .. } => {
                        if self.diagnostics {
                            println!("edit-invalid={message:?}");
                        }
                    }
                }
            } else if self.diagnostics {
                println!("editor-event={event:?}");
            }
        }
    }
    fn application_events(&mut self) -> Result<(), String> {
        for invocation in self.ui.commands_mut().drain_invocations() {
            if self.diagnostics {
                println!("command={}", invocation.id.0);
            }
            if invocation.id.0 == "Confirm" {
                let editor = self
                    .message()
                    .ok_or("Confirm requires a TextBox named Message")?;
                if matches!(
                    self.ui.commit_edit(editor).map_err(err)?,
                    CommitOutcome::Unchanged
                ) {
                    self.last_notice = "Значение подтверждено.";
                }
            }
        }
        for id in self.view.checks.iter().chain(&self.view.toggles) {
            let Some(checked) = self.ui.checked(*id) else {
                continue;
            };
            if self.checked.insert(*id, checked) == Some(checked) {
                continue;
            }
            let name = self
                .view
                .names
                .iter()
                .find_map(|(name, widget)| (*widget == *id).then_some(name.as_str()))
                .unwrap_or("");
            if self.diagnostics {
                let prefix = if self.view.roles.get(id) == Some(&Role::ToggleButton) {
                    "toggle-state"
                } else {
                    "checkbox-state"
                };
                println!("{prefix}={name} checked={checked}");
            }
            if name == "Option"
                && let Some(editor) = self.message()
            {
                let mut settings = self
                    .ui
                    .editor_settings(editor)
                    .expect("Message is an editor")
                    .clone();
                settings.is_read_only = checked;
                self.ui.set_editor_settings(editor, settings).map_err(err)?;
                if self.diagnostics {
                    println!("editor-read-only={checked}");
                }
            }
        }
        self.ui.drain_activated();
        let focus = self.ui.runtime.focused();
        if focus != self.last_focus {
            self.last_focus = focus;
            if self.diagnostics {
                let name = focus
                    .and_then(|id| {
                        self.view
                            .names
                            .iter()
                            .find_map(|(name, widget)| (*widget == id).then_some(name.as_str()))
                    })
                    .unwrap_or("");
                println!("focus-name={name:?}");
            }
        }
        Ok(())
    }
}

impl DesktopApp for Form {
    fn update(&mut self, viewport: Size, _scale: f32) -> Result<(), String> {
        self.consume_edit_events();
        self.refresh_status()?;
        self.ui.update(viewport).map_err(err)?;
        self.application_events()?;
        self.consume_edit_events();
        if self.refresh_status()? {
            self.ui.update(viewport).map_err(err)?;
        }
        Ok(())
    }
    fn scene(&self) -> &Scene {
        self.ui.scene()
    }
    fn title(&self) -> String {
        self.title.clone()
    }
    fn event(&mut self, event: PlatformEvent) -> Result<EventResponse, String> {
        if self.diagnostics
            && matches!(
                &event,
                PlatformEvent::Input(
                    rust_desktop_ui_core::InputEvent::KeyDown { .. }
                        | rust_desktop_ui_core::InputEvent::KeyUp { .. }
                        | rust_desktop_ui_core::InputEvent::WindowFocused(_)
                ) | PlatformEvent::Text(_)
                    | PlatformEvent::Paste(_)
                    | PlatformEvent::ImePreedit(..)
                    | PlatformEvent::ImeCommit(_)
                    | PlatformEvent::ImeCancel
            )
        {
            println!(
                "edit-input={event:?} focused={}",
                self.message().is_some() && self.ui.runtime.focused() == self.message()
            );
        }
        let mut redraw = true;
        match event {
            PlatformEvent::Input(input) => self.ui.dispatch(input).map_err(err)?,
            PlatformEvent::Text(text) | PlatformEvent::Paste(text) => {
                self.ui.text_input(&text).map_err(err)?
            }
            PlatformEvent::ImePreedit(text, cursor) => {
                self.ui.ime_preedit(&text, cursor).map_err(err)?
            }
            PlatformEvent::ImeCommit(text) => self.ui.ime_commit(&text).map_err(err)?,
            PlatformEvent::ImeCancel => self.ui.cancel_preedit(),
            PlatformEvent::Tick => redraw = self.ui.blink_caret(),
            PlatformEvent::Accessibility(request) => {
                self.accessible.action(&mut self.ui, request)?
            }
        }
        self.application_events()?;
        for change in self.ui.drain_text_changes() {
            if self.diagnostics {
                if self.message() == Some(change.id) {
                    println!("edit-value={:?}", change.text);
                } else {
                    println!("editor-value={:?} value={:?}", change.id, change.text);
                }
            }
        }
        self.consume_edit_events();
        redraw |= self.refresh_status()?;
        let clipboard = self
            .ui
            .drain_clipboard_requests()
            .into_iter()
            .map(|request| match request {
                rust_desktop_ui_controls::ClipboardRequest::Copy(text) => {
                    ClipboardRequest::Copy(text)
                }
                rust_desktop_ui_controls::ClipboardRequest::Paste => ClipboardRequest::Paste,
            })
            .collect();
        Ok(EventResponse { redraw, clipboard })
    }
    fn ime_cursor(&self) -> Option<Rect> {
        self.ime_target().and_then(|_| self.ui.caret_bounds())
    }
    fn ime_target(&self) -> Option<WidgetId> {
        self.ui
            .runtime
            .focused()
            .filter(|id| self.ui.is_text_editable(*id))
    }
    fn ime_composition_active(&self) -> Option<bool> {
        self.ime_target()
            .and_then(|id| self.ui.document(id))
            .map(|doc| doc.composition().is_some())
    }
    fn wake_after(&self) -> Option<Duration> {
        self.ui.caret_bounds().map(|_| Duration::from_millis(500))
    }
    fn accessibility(&mut self, scale: f32) -> accesskit::TreeUpdate {
        self.accessible
            .tree(&self.ui, &self.view, &self.title, scale)
    }
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
