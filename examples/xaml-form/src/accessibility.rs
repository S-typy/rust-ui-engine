//! Native accessibility projection of the complete retained form.
use crate::view::MountedView;
use rust_desktop_ui_controls::{Controls, Role as ControlRole, SemanticAction};
use rust_desktop_ui_core::{Rect, WidgetId};
use rust_desktop_ui_platform_winit::accesskit::{self, Action, Node, NodeId, Role};
use std::collections::{HashMap, HashSet};

const WINDOW: NodeId = NodeId(1);

pub(crate) struct Accessibility {
    root: NodeId,
    widgets: HashMap<WidgetId, NodeId>,
    errors: HashMap<WidgetId, NodeId>,
    next: u64,
}
impl Default for Accessibility {
    fn default() -> Self {
        Self {
            root: WINDOW,
            widgets: HashMap::new(),
            errors: HashMap::new(),
            next: 2,
        }
    }
}
impl Accessibility {
    /// Distinct stable ranges let independently retained panes share one native
    /// accessibility tree without remapping their internal relationships.
    pub(crate) fn with_base(base: u64) -> Self {
        Self {
            root: NodeId(base),
            next: base + 1,
            widgets: HashMap::new(),
            errors: HashMap::new(),
        }
    }

    pub(crate) fn root_id(&self) -> NodeId {
        self.root
    }
    pub(crate) fn id(&self, widget: WidgetId) -> Option<NodeId> {
        self.widgets.get(&widget).copied()
    }

    pub(crate) fn tree(
        &mut self,
        ui: &Controls,
        view: &MountedView,
        title: &str,
        scale: f32,
    ) -> accesskit::TreeUpdate {
        let semantics = ui.semantics();
        let present: HashSet<_> = semantics.iter().map(|node| node.id).collect();
        self.widgets
            .retain(|widget, _| ui.runtime.tree().node(*widget).is_some());
        self.errors
            .retain(|widget, _| ui.runtime.tree().node(*widget).is_some());
        for semantic in &semantics {
            self.widgets.entry(semantic.id).or_insert_with(|| {
                let id = NodeId(self.next);
                self.next += 1;
                id
            });
        }
        let mut window = Node::new(Role::Window);
        window.set_label(title);
        window.set_children(
            semantics
                .iter()
                .filter(|node| !node.parent.is_some_and(|parent| present.contains(&parent)))
                .map(|node| self.widgets[&node.id])
                .collect::<Vec<_>>(),
        );
        let mut nodes = vec![(self.root, window)];
        let mut focus = self.root;
        for semantic in semantics {
            let widget = semantic.id;
            let id = self.widgets[&widget];
            let status = view.find_name("Status") == Some(widget);
            let mut node = Node::new(if status {
                Role::Status
            } else if semantic.role == ControlRole::MenuItem && semantic.checked.is_some() {
                Role::MenuItemCheckBox
            } else {
                role(semantic.role)
            });
            node.set_label(
                view.automation_names
                    .get(&widget)
                    .cloned()
                    .unwrap_or(semantic.label),
            );
            node.set_bounds(bounds(semantic.bounds, scale));
            if status {
                node.set_live(accesskit::Live::Polite);
            }
            if let Some(value) = semantic.value {
                node.set_value(value);
            }
            if let Some(checked) = semantic.checked {
                node.set_toggled(if checked {
                    accesskit::Toggled::True
                } else {
                    accesskit::Toggled::False
                });
            }
            if semantic.has_popup {
                node.set_has_popup(if semantic.role == ControlRole::ComboBox {
                    accesskit::HasPopup::Listbox
                } else {
                    accesskit::HasPopup::Menu
                });
            }
            if let Some(expanded) = semantic.expanded {
                node.set_expanded(expanded);
            }
            if let Some(tooltip) = semantic.tooltip {
                node.set_tooltip(tooltip);
            }
            if semantic.disabled {
                node.set_disabled();
            }
            if semantic.read_only {
                node.set_read_only();
            }
            if matches!(semantic.role, ControlRole::Tab | ControlRole::ListItem) {
                node.set_selected(semantic.selected);
            }
            if let Some(settings) = ui.editor_settings(widget) {
                node.set_description(&settings.placeholder);
            }
            if ui
                .editor_validation(widget)
                .is_some_and(|validation| validation.is_required)
            {
                node.set_required();
            }
            if view.find_name("Message") == Some(widget)
                && let Some(help) = view
                    .find_name("Help")
                    .filter(|help| present.contains(help))
                    .and_then(|help| self.id(help))
            {
                node.set_described_by(vec![help]);
            }
            let mut children: Vec<_> = semantic
                .children
                .iter()
                .filter_map(|child| self.id(*child))
                .collect();
            if let Some(message) = semantic.validation_error {
                let error_id = *self.errors.entry(widget).or_insert_with(|| {
                    let id = NodeId(self.next);
                    self.next += 1;
                    id
                });
                let mut error = Node::new(Role::Label);
                error.set_label(message);
                error.set_live(accesskit::Live::Polite);
                nodes.push((error_id, error));
                children.push(error_id);
                node.set_invalid(accesskit::Invalid::True);
                node.set_error_message(error_id);
            }
            for action in semantic.actions {
                match action {
                    SemanticAction::Focus => node.add_action(Action::Focus),
                    SemanticAction::Activate | SemanticAction::Toggle => {
                        node.add_action(Action::Click)
                    }
                    SemanticAction::SetValue => node.add_action(Action::SetValue),
                    SemanticAction::Expand => node.add_action(Action::Expand),
                    SemanticAction::Collapse => node.add_action(Action::Collapse),
                    SemanticAction::Scroll => {
                        node.add_action(Action::ScrollDown);
                        node.add_action(Action::ScrollUp);
                    }
                }
            }
            node.set_children(children);
            if semantic.focused {
                focus = id;
            }
            nodes.push((id, node));
        }
        accesskit::TreeUpdate {
            nodes,
            tree: Some(accesskit::TreeInfo::new(self.root)),
            tree_id: accesskit::TreeId::ROOT,
            focus,
        }
    }

    pub(crate) fn action(
        &self,
        ui: &mut Controls,
        request: accesskit::ActionRequest,
    ) -> Result<(), String> {
        if request.target_tree != accesskit::TreeId::ROOT {
            return Ok(());
        }
        let Some(widget) = self
            .widgets
            .iter()
            .find_map(|(widget, id)| (*id == request.target_node).then_some(*widget))
        else {
            return Ok(());
        };
        let action = match request.action {
            Action::Focus => SemanticAction::Focus,
            Action::Click => SemanticAction::Activate,
            Action::SetValue => SemanticAction::SetValue,
            Action::Expand => SemanticAction::Expand,
            Action::Collapse => SemanticAction::Collapse,
            _ => return Ok(()),
        };
        // Ignore stale/hidden nodes and unsupported actions, including labels.
        if !ui
            .semantics()
            .iter()
            .any(|node| node.id == widget && node.actions.contains(&action))
        {
            return Ok(());
        }
        let value = match &request.data {
            Some(accesskit::ActionData::Value(value)) => Some(value.as_ref()),
            _ => None,
        };
        if action == SemanticAction::SetValue && value.is_none() {
            return Ok(());
        }
        ui.perform_action(widget, action, value)
            .map_err(|error| error.to_string())
    }
}

fn bounds(rect: Rect, scale: f32) -> accesskit::Rect {
    accesskit::Rect {
        x0: f64::from(rect.x * scale),
        y0: f64::from(rect.y * scale),
        x1: f64::from((rect.x + rect.width) * scale),
        y1: f64::from((rect.y + rect.height) * scale),
    }
}
fn role(role: ControlRole) -> Role {
    match role {
        ControlRole::Panel => Role::GenericContainer,
        ControlRole::Group => Role::Group,
        ControlRole::Label => Role::Label,
        ControlRole::Button | ControlRole::ToggleButton => Role::Button,
        ControlRole::CheckBox => Role::CheckBox,
        ControlRole::RadioButton => Role::RadioButton,
        ControlRole::Image => Role::Image,
        ControlRole::Separator => Role::Splitter,
        ControlRole::TextBox => Role::TextInput,
        ControlRole::ListBox => Role::ListBox,
        ControlRole::ListItem => Role::ListBoxOption,
        ControlRole::ComboBox => Role::ComboBox,
        ControlRole::SpinButton => Role::SpinButton,
        ControlRole::ScrollView => Role::ScrollView,
        ControlRole::TabList => Role::TabList,
        ControlRole::Tab => Role::Tab,
        ControlRole::Menu => Role::Menu,
        ControlRole::MenuItem => Role::MenuItem,
    }
}
