use rust_desktop_ui_controls::{Controls, SemanticAction, Theme};
use rust_desktop_ui_core::{
    Axis, Edges, LayoutKind, LayoutStyle, Length, Mutation, Rect, Scene, Size, WidgetId,
};
use rust_desktop_ui_platform_winit::{
    ClipboardRequest, DesktopApp, EventResponse, PlatformEvent, accesskit,
};
use rust_desktop_ui_xaml::WindowDefinition;
use std::time::Duration;

const WINDOW: accesskit::NodeId = accesskit::NodeId(1);
const EDIT: accesskit::NodeId = accesskit::NodeId(2);

/// Mounts the compiled two-element profile using the existing controls and host.
/// This adapter stays in the example until the markup supports a general tree.
pub(crate) struct Form {
    pub(crate) ui: Controls,
    pub(crate) editor: WidgetId,
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
        ui.set_placeholder(editor, &field.placeholder)
            .map_err(err)?;
        ui.set_enabled(editor, field.is_enabled).map_err(err)?;
        Ok(Self {
            ui,
            editor,
            definition,
            diagnostics,
        })
    }

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
        self.ui.update(viewport).map_err(err)?;
        Ok(())
    }

    fn scene(&self) -> &Scene {
        self.ui.scene()
    }

    fn title(&self) -> String {
        self.definition.title.clone()
    }

    fn event(&mut self, event: PlatformEvent) -> Result<EventResponse, String> {
        let before = self.diagnostics.then(|| self.text().to_owned());
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
        if before.as_deref().is_some_and(|value| value != self.text()) {
            println!("edit-value={:?}", self.text());
        }
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
        self.ui.caret_bounds()
    }

    fn ime_target(&self) -> Option<WidgetId> {
        self.ui.runtime.focused().filter(|id| *id == self.editor)
    }

    fn ime_composition_active(&self) -> Option<bool> {
        self.ime_target()
            .and_then(|id| self.ui.document(id))
            .map(|doc| doc.composition().is_some())
    }

    fn wake_after(&self) -> Option<Duration> {
        self.ime_cursor().map(|_| Duration::from_millis(500))
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
            root.set_children(vec![EDIT]);
            let mut node = accesskit::Node::new(accesskit::Role::TextInput);
            node.set_label(semantic.label);
            node.set_value(semantic.value.unwrap_or_default());
            node.set_description(&self.definition.edit_box.placeholder);
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
                node.add_action(accesskit::Action::SetValue);
            }
            if semantic.focused {
                focus = EDIT;
            }
            nodes.push((EDIT, node));
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
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
