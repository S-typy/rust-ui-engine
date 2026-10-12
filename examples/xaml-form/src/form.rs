use rust_desktop_ui_controls::{
    Controls, EditBoxAppearance, EditBoxStyle, EditEvent, EditorSettings, EditorValidation,
    SemanticAction, Theme,
};
use rust_desktop_ui_core::{
    Axis, Color, Edges, LayoutKind, LayoutStyle, Length, Mutation, Rect, Scene, Size, WidgetId,
};
use rust_desktop_ui_platform_winit::{
    ClipboardRequest, DesktopApp, EventResponse, PlatformEvent, accesskit,
};
use rust_desktop_ui_xaml::WindowDefinition;
use std::time::Duration;

const WINDOW: accesskit::NodeId = accesskit::NodeId(1);
const EDIT: accesskit::NodeId = accesskit::NodeId(2);
const HELP: accesskit::NodeId = accesskit::NodeId(3);
const STATUS: accesskit::NodeId = accesskit::NodeId(4);
const EDIT_HELP: &str = "Enter — подтвердить · Escape — отменить изменения";

/// Mounts the compiled two-element profile using the existing controls and host.
/// This adapter stays in the example until the markup supports a general tree.
pub(crate) struct Form {
    pub(crate) ui: Controls,
    pub(crate) editor: WidgetId,
    helper: WidgetId,
    status: WidgetId,
    status_text: String,
    last_notice: &'static str,
    definition: WindowDefinition,
    diagnostics: bool,
}

impl Form {
    pub(crate) fn new(definition: WindowDefinition, diagnostics: bool) -> Result<Self, String> {
        let theme = match definition.theme {
            rust_desktop_ui_xaml::Theme::Light => Theme::FluentLight,
            rust_desktop_ui_xaml::Theme::Dark => Theme::FluentDark,
        };
        let mut ui = Controls::new(theme).map_err(err)?;
        ui.runtime
            .apply(Mutation::SetStyle {
                node: ui.root(),
                style: LayoutStyle {
                    kind: LayoutKind::Stack(Axis::Vertical),
                    padding: edges(definition.padding),
                    ..Default::default()
                },
            })
            .map_err(err)?;
        let field = &definition.edit_box;
        let editor = ui
            .edit_box(
                ui.root(),
                &field.label,
                &field.text,
                LayoutStyle {
                    width: field.width.map_or(Length::Auto, Length::Px),
                    height: Length::Px(field.height),
                    margin: edges(field.margin),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )
            .map_err(err)?;
        ui.set_editor_settings(
            editor,
            EditorSettings {
                is_read_only: field.is_read_only,
                max_length: field.max_length,
                placeholder: field.placeholder.clone(),
                commit_on_focus_lost: field.commit_on_lost_focus,
            },
        )
        .map_err(err)?;
        ui.set_editor_validation(
            editor,
            EditorValidation {
                is_required: field.is_required,
                message: field.validation_message.clone(),
            },
        )
        .map_err(err)?;
        ui.set_edit_box_style(
            editor,
            EditBoxStyle {
                font_size: field.font_size,
                padding: field.padding.map(edges),
                corner_radius: field.corner_radius,
                border_width: field.border_thickness,
                normal: EditBoxAppearance {
                    background: field.background.map(color),
                    foreground: field.foreground.map(color),
                    placeholder: field.placeholder_foreground.map(color),
                    border: field.border_brush.map(color),
                    accent: field.focus_brush.map(color),
                    selection: field.selection_brush.map(color),
                },
                invalid: EditBoxAppearance {
                    border: field.validation_brush.map(color),
                    accent: field.validation_brush.map(color),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .map_err(err)?;
        ui.set_enabled(editor, field.is_enabled).map_err(err)?;
        let helper = ui
            .label(
                ui.root(),
                EDIT_HELP,
                LayoutStyle {
                    height: Length::Px(44.0),
                    margin: Edges {
                        top: 12.0,
                        ..Default::default()
                    },
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )
            .map_err(err)?;
        let status_text = "Изменений нет.".to_owned();
        let status = ui
            .label(
                ui.root(),
                &status_text,
                LayoutStyle {
                    height: Length::Px(56.0),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )
            .map_err(err)?;
        // Mounting establishes the initial value; subsequent mutations are events.
        ui.drain_text_changes();
        ui.drain_edit_events();
        Ok(Self {
            ui,
            editor,
            helper,
            status,
            status_text,
            last_notice: "Изменений нет.",
            definition,
            diagnostics,
        })
    }

    #[cfg(test)]
    pub(crate) fn text(&self) -> &str {
        self.ui
            .document(self.editor)
            .expect("form owns editor")
            .text()
    }

    /// Resolve the name from the compiled XAML namescope.
    pub(crate) fn find_name(&self, name: &str) -> Option<WidgetId> {
        (self.definition.edit_box.name.as_deref() == Some(name)).then_some(self.editor)
    }

    fn current_status(&self) -> &str {
        self.ui.validation_error(self.editor).unwrap_or_else(|| {
            if self.ui.is_edit_dirty(self.editor) == Some(true) {
                "Есть неподтверждённые изменения."
            } else {
                self.last_notice
            }
        })
    }

    fn refresh_status(&mut self) -> Result<bool, String> {
        let text = self.current_status();
        if self.status_text == text {
            return Ok(false);
        }
        self.status_text = text.to_owned();
        self.ui
            .set_label(self.status, &self.status_text)
            .map_err(err)?;
        Ok(true)
    }

    fn consume_edit_events(&mut self) {
        for event in self.ui.drain_edit_events() {
            match event {
                EditEvent::Committed { id, value } if id == self.editor => {
                    self.last_notice = "Значение подтверждено.";
                    if self.diagnostics {
                        println!("edit-committed={value:?}");
                    }
                }
                EditEvent::Canceled { id } if id == self.editor => {
                    self.last_notice = "Изменения отменены.";
                    if self.diagnostics {
                        println!("edit-canceled=true");
                    }
                }
                EditEvent::ValidationFailed { id, message }
                    if id == self.editor && self.diagnostics =>
                {
                    println!("edit-invalid={message:?}");
                }
                _ => {}
            }
        }
    }

    fn accessibility_action(&mut self, request: accesskit::ActionRequest) -> Result<(), String> {
        if request.target_tree != accesskit::TreeId::ROOT || request.target_node != EDIT {
            return Ok(());
        }
        match request.action {
            accesskit::Action::Focus => self
                .ui
                .perform_action(self.editor, SemanticAction::Focus, None)
                .map_err(err),
            accesskit::Action::SetValue => {
                if let Some(accesskit::ActionData::Value(value)) = request.data {
                    self.ui
                        .perform_action(self.editor, SemanticAction::SetValue, Some(&value))
                        .map_err(err)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

impl DesktopApp for Form {
    fn update(&mut self, viewport: Size, _scale: f32) -> Result<(), String> {
        self.consume_edit_events();
        self.refresh_status()?;
        self.ui.update(viewport).map_err(err)?;
        // Runtime focus changes can accept a draft during the controls update.
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
        self.definition.title.clone()
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
                self.ui.runtime.focused() == Some(self.editor)
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
            PlatformEvent::Accessibility(request) => self.accessibility_action(request)?,
        }
        for change in self.ui.drain_text_changes() {
            if self.diagnostics && change.id == self.editor {
                println!("edit-value={:?}", change.text);
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
            .filter(|id| *id == self.editor && self.ui.is_text_editable(*id))
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
        let mut root = accesskit::Node::new(accesskit::Role::Window);
        root.set_label(&self.definition.title);
        let mut nodes = Vec::new();
        let mut focus = WINDOW;
        if let Some(semantic) = self
            .ui
            .semantics()
            .into_iter()
            .find(|node| node.id == self.editor)
        {
            root.set_children(vec![EDIT, HELP, STATUS]);
            let mut node = accesskit::Node::new(accesskit::Role::TextInput);
            node.set_label(semantic.label);
            node.set_value(semantic.value.unwrap_or_default());
            if let Some(settings) = self.ui.editor_settings(self.editor) {
                node.set_description(&settings.placeholder);
            }
            node.set_described_by(vec![HELP]);
            if self
                .ui
                .editor_validation(self.editor)
                .is_some_and(|rules| rules.is_required)
            {
                node.set_required();
            }
            if semantic.validation_error.is_some() {
                node.set_invalid(accesskit::Invalid::True);
                node.set_error_message(STATUS);
            }
            let bounds = semantic.bounds;
            node.set_bounds(accesskit::Rect {
                x0: f64::from(bounds.x * scale),
                y0: f64::from(bounds.y * scale),
                x1: f64::from((bounds.x + bounds.width) * scale),
                y1: f64::from((bounds.y + bounds.height) * scale),
            });
            if semantic.disabled {
                node.set_disabled();
            } else {
                node.add_action(accesskit::Action::Focus);
                if !semantic.read_only {
                    node.add_action(accesskit::Action::SetValue);
                }
            }
            if semantic.read_only {
                node.set_read_only();
            }
            if semantic.focused {
                focus = EDIT;
            }
            nodes.push((EDIT, node));
            for (id, widget, label) in [
                (HELP, self.helper, EDIT_HELP),
                (STATUS, self.status, self.current_status()),
            ] {
                let mut node = accesskit::Node::new(if id == STATUS {
                    accesskit::Role::Status
                } else {
                    accesskit::Role::Label
                });
                node.set_label(label);
                if id == STATUS {
                    node.set_live(accesskit::Live::Polite);
                }
                if let Some(bounds) = self.ui.runtime.bounds(widget) {
                    node.set_bounds(accesskit::Rect {
                        x0: f64::from(bounds.x * scale),
                        y0: f64::from(bounds.y * scale),
                        x1: f64::from((bounds.x + bounds.width) * scale),
                        y1: f64::from((bounds.y + bounds.height) * scale),
                    });
                }
                nodes.push((id, node));
            }
        }
        nodes.insert(0, (WINDOW, root));
        accesskit::TreeUpdate {
            nodes,
            tree: Some(accesskit::TreeInfo::new(WINDOW)),
            tree_id: accesskit::TreeId::ROOT,
            focus,
        }
    }
}

fn edges([left, top, right, bottom]: [f32; 4]) -> Edges {
    Edges {
        left,
        top,
        right,
        bottom,
    }
}

fn color([red, green, blue, alpha]: [u8; 4]) -> Color {
    Color {
        r: f32::from(red) / 255.0,
        g: f32::from(green) / 255.0,
        b: f32::from(blue) / 255.0,
        a: f32::from(alpha) / 255.0,
    }
}

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
