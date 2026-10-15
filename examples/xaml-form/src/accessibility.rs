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
            let status =
                semantic.role == ControlRole::Label && view.find_name("Status") == Some(widget);
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
            } else if semantic.role == ControlRole::StatusBar {
                // Only the message is announced. Interactive status children
                // must not make the complete bar a repeated live announcement.
                node.set_live(accesskit::Live::Off);
            }
            if let Some(value) = semantic.value {
                node.set_value(value);
            }
            if let Some(settings) = ui.progress_settings(widget) {
                node.set_min_numeric_value(settings.minimum);
                node.set_max_numeric_value(settings.maximum);
                node.set_numeric_value(settings.value);
                node.set_read_only();
            }
            if let Some(settings) = ui.zoom_settings(widget) {
                node.set_min_numeric_value(settings.minimum);
                node.set_max_numeric_value(settings.maximum);
                node.set_numeric_value(settings.value);
                node.set_numeric_value_step(settings.step);
                node.set_orientation(accesskit::Orientation::Horizontal);
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
        let numeric_value = match &request.data {
            Some(accesskit::ActionData::NumericValue(value))
                if value.is_finite() && ui.zoom_settings(widget).is_some() =>
            {
                Some(value.to_string())
            }
            _ => None,
        };
        let value = match &request.data {
            Some(accesskit::ActionData::Value(value)) => Some(value.as_ref()),
            Some(accesskit::ActionData::NumericValue(_)) => numeric_value.as_deref(),
            _ => None,
        };
        if action == SemanticAction::SetValue && value.is_none() {
            return Ok(());
        }
        if action == SemanticAction::SetValue
            && let Some(settings) = ui.zoom_settings(widget)
            && value
                .and_then(|value| value.parse::<f64>().ok())
                .is_none_or(|value| {
                    !value.is_finite() || !(settings.minimum..=settings.maximum).contains(&value)
                })
        {
            // Native range requests are untrusted input. An invalid value
            // preserves state without terminating the application event loop.
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
        ControlRole::StatusBar => Role::Status,
        ControlRole::ProgressBar => Role::ProgressIndicator,
        ControlRole::Slider => Role::Slider,
    }
}

#[cfg(test)]
mod status_tests {
    use super::*;
    use rust_desktop_ui_controls::{ProgressBarSettings, Theme, ZoomControlSettings};
    use rust_desktop_ui_core::{LayoutStyle, Length, Mutation, Size};

    const VIEWPORT: Size = Size::new(800.0, 200.0);

    struct Fixture {
        ui: Controls,
        view: MountedView,
        accessibility: Accessibility,
        bar: WidgetId,
        message: WidgetId,
        progress: WidgetId,
        zoom: WidgetId,
    }

    impl Fixture {
        fn new(ribbon: bool) -> Self {
            let mut ui = Controls::new(Theme::FluentLight).unwrap();
            let root = ui.root();
            let style = LayoutStyle {
                width: Length::Px(VIEWPORT.width),
                ..Default::default()
            };
            let bar = if ribbon {
                ui.ribbon_status_bar(root, style)
            } else {
                ui.status_bar(root, style)
            }
            .unwrap();
            let message = ui
                .label(
                    bar.left,
                    "Ready",
                    LayoutStyle {
                        width: Length::Px(180.0),
                        height: Length::Px(20.0),
                        ..Default::default()
                    },
                )
                .unwrap();
            let progress = ui
                .progress_bar(
                    bar.right,
                    ProgressBarSettings {
                        minimum: 10.0,
                        maximum: 90.0,
                        value: 45.0,
                    },
                    LayoutStyle::default(),
                )
                .unwrap();
            let zoom = ui
                .zoom_control(
                    bar.right,
                    ZoomControlSettings {
                        minimum: 50.0,
                        maximum: 200.0,
                        value: 100.0,
                        step: 5.0,
                    },
                    LayoutStyle::default(),
                )
                .unwrap();
            let mut view = MountedView::default();
            view.names.insert("Status".into(), message);
            ui.update(VIEWPORT).unwrap();
            Self {
                ui,
                view,
                accessibility: Accessibility::default(),
                bar: bar.container,
                message,
                progress,
                zoom,
            }
        }

        fn tree(&mut self) -> accesskit::TreeUpdate {
            self.accessibility
                .tree(&self.ui, &self.view, "Status controls", 1.5)
        }

        fn set_value(&mut self, widget: WidgetId, value: f64) -> Result<(), String> {
            self.accessibility.action(
                &mut self.ui,
                accesskit::ActionRequest {
                    action: Action::SetValue,
                    target_tree: accesskit::TreeId::ROOT,
                    target_node: self.accessibility.id(widget).unwrap(),
                    data: Some(accesskit::ActionData::NumericValue(value)),
                },
            )
        }

        fn node<'a>(&self, tree: &'a accesskit::TreeUpdate, widget: WidgetId) -> &'a Node {
            let id = self.accessibility.id(widget).unwrap();
            &tree.nodes.iter().find(|(node, _)| *node == id).unwrap().1
        }
    }

    #[test]
    fn status_containers_preserve_message_announcements_and_numeric_roles() {
        for ribbon in [false, true] {
            let mut fixture = Fixture::new(ribbon);
            let tree = fixture.tree();
            let bar = fixture.node(&tree, fixture.bar);
            assert_eq!(bar.role(), Role::Status);
            assert_eq!(bar.live(), Some(accesskit::Live::Off));
            assert!(!bar.supports_action(Action::Focus));
            let message = fixture.node(&tree, fixture.message);
            assert_eq!(message.live(), Some(accesskit::Live::Polite));
            assert_eq!(message.label(), Some("Ready"));
            let progress = fixture.node(&tree, fixture.progress);
            assert_eq!(progress.role(), Role::ProgressIndicator);
            assert_eq!(progress.min_numeric_value(), Some(10.0));
            assert_eq!(progress.max_numeric_value(), Some(90.0));
            assert_eq!(progress.numeric_value(), Some(45.0));
            assert!(progress.is_read_only());
            assert!(!progress.supports_action(Action::Focus));
            assert!(!progress.supports_action(Action::Click));
            assert!(!progress.supports_action(Action::SetValue));
            let zoom = fixture.node(&tree, fixture.zoom);
            assert_eq!(zoom.role(), Role::Slider);
            assert_eq!(zoom.min_numeric_value(), Some(50.0));
            assert_eq!(zoom.max_numeric_value(), Some(200.0));
            assert_eq!(zoom.numeric_value(), Some(100.0));
            assert_eq!(zoom.numeric_value_step(), Some(5.0));
            assert_eq!(zoom.orientation(), Some(accesskit::Orientation::Horizontal));
            assert!(zoom.supports_action(Action::Focus));
            assert!(zoom.supports_action(Action::SetValue));
            fixture.view.names.insert("Status".into(), fixture.bar);
            let tree = fixture.tree();
            assert_eq!(
                fixture.node(&tree, fixture.bar).live(),
                Some(accesskit::Live::Off)
            );
        }
    }

    #[test]
    fn numeric_zoom_requests_ignore_nonfinite_disabled_and_unsupported_targets() {
        let mut fixture = Fixture::new(true);
        fixture.tree();
        fixture.set_value(fixture.zoom, 137.5).unwrap();
        assert_eq!(fixture.ui.zoom_value(fixture.zoom), Some(137.5));
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            fixture.set_value(fixture.zoom, value).unwrap();
            assert_eq!(fixture.ui.zoom_value(fixture.zoom), Some(137.5));
        }
        fixture.set_value(fixture.zoom, 201.0).unwrap();
        assert_eq!(fixture.ui.zoom_value(fixture.zoom), Some(137.5));
        for value in ["NaN", "inf", "49", "201", "invalid"] {
            fixture
                .accessibility
                .action(
                    &mut fixture.ui,
                    accesskit::ActionRequest {
                        action: Action::SetValue,
                        target_tree: accesskit::TreeId::ROOT,
                        target_node: fixture.accessibility.id(fixture.zoom).unwrap(),
                        data: Some(accesskit::ActionData::Value(value.into())),
                    },
                )
                .unwrap();
            assert_eq!(fixture.ui.zoom_value(fixture.zoom), Some(137.5));
        }
        let tree = fixture.tree();
        assert_eq!(
            fixture.node(&tree, fixture.zoom).numeric_value(),
            Some(137.5)
        );
        fixture.set_value(fixture.progress, 60.0).unwrap();
        assert_eq!(fixture.ui.progress_value(fixture.progress), Some(45.0));
        fixture.set_value(fixture.message, 60.0).unwrap();
        fixture.ui.set_enabled(fixture.bar, false).unwrap();
        fixture.set_value(fixture.zoom, 150.0).unwrap();
        assert_eq!(fixture.ui.zoom_value(fixture.zoom), Some(137.5));
        fixture.ui.set_enabled(fixture.bar, true).unwrap();
        fixture.set_value(fixture.zoom, 150.0).unwrap();
        assert_eq!(fixture.ui.zoom_value(fixture.zoom), Some(150.0));
    }

    #[test]
    fn numeric_zoom_requests_ignore_hidden_and_removed_nodes_with_stable_ids() {
        let mut fixture = Fixture::new(false);
        fixture.tree();
        let zoom_id = fixture.accessibility.id(fixture.zoom);
        let mut props = fixture
            .ui
            .runtime
            .tree()
            .node(fixture.bar)
            .unwrap()
            .props
            .clone();
        props.visible = false;
        fixture
            .ui
            .runtime
            .apply(Mutation::SetProps {
                node: fixture.bar,
                props: props.clone(),
            })
            .unwrap();
        fixture.ui.update(VIEWPORT).unwrap();
        fixture.set_value(fixture.zoom, 150.0).unwrap();
        assert_eq!(fixture.ui.zoom_value(fixture.zoom), Some(100.0));
        let hidden = fixture.tree();
        assert!(hidden.nodes.iter().all(|(id, _)| Some(*id) != zoom_id));
        props.visible = true;
        fixture
            .ui
            .runtime
            .apply(Mutation::SetProps {
                node: fixture.bar,
                props,
            })
            .unwrap();
        fixture.ui.update(VIEWPORT).unwrap();
        fixture.tree();
        assert_eq!(fixture.accessibility.id(fixture.zoom), zoom_id);
        fixture.ui.remove(fixture.bar).unwrap();
        fixture.set_value(fixture.zoom, 180.0).unwrap();
        assert_eq!(fixture.ui.zoom_value(fixture.zoom), None);
    }
}
