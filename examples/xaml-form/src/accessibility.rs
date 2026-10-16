//! Native accessibility projection of the complete retained form.
use crate::view::MountedView;
use rust_desktop_ui_controls::{Controls, Role as ControlRole, SemanticAction};
use rust_desktop_ui_core::{Rect, WidgetId};
use rust_desktop_ui_platform_winit::accesskit::{self, Action, Node, NodeId, Role};
use std::collections::{HashMap, HashSet};

const WINDOW: NodeId = NodeId(1);

#[path = "accessibility_grid.rs"]
mod grid;

pub(crate) struct Accessibility {
    root: NodeId,
    widgets: HashMap<WidgetId, NodeId>,
    errors: HashMap<WidgetId, NodeId>,
    next: u64,
    grids: grid::GridAccessibility,
}
impl Default for Accessibility {
    fn default() -> Self {
        Self {
            root: WINDOW,
            widgets: HashMap::new(),
            errors: HashMap::new(),
            next: 2,
            grids: grid::GridAccessibility::default(),
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
            grids: grid::GridAccessibility::default(),
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
        self.grids.begin();
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
            if ui.progress_indeterminate(widget) != Some(true)
                && let Some(value) = semantic.value
            {
                node.set_value(value);
            }
            if let Some(settings) = ui.progress_settings(widget) {
                if ui.progress_indeterminate(widget) != Some(true) {
                    node.set_min_numeric_value(settings.minimum);
                    node.set_max_numeric_value(settings.maximum);
                    node.set_numeric_value(settings.value);
                }
                node.set_read_only();
            }
            if let Some(settings) = ui.zoom_settings(widget) {
                node.set_min_numeric_value(settings.minimum);
                node.set_max_numeric_value(settings.maximum);
                node.set_numeric_value(settings.value);
                node.set_numeric_value_step(settings.step);
                node.set_orientation(accesskit::Orientation::Horizontal);
            }
            if let Some(settings) = ui.slider_settings(widget) {
                node.set_min_numeric_value(settings.minimum);
                node.set_max_numeric_value(settings.maximum);
                node.set_numeric_value(settings.value);
                node.set_numeric_value_step(settings.step);
                node.set_orientation(match settings.axis {
                    rust_desktop_ui_core::Axis::Horizontal => accesskit::Orientation::Horizontal,
                    rust_desktop_ui_core::Axis::Vertical => accesskit::Orientation::Vertical,
                });
            }
            if semantic.checked_mixed {
                node.set_toggled(accesskit::Toggled::Mixed);
            } else if let Some(checked) = semantic.checked {
                node.set_toggled(if checked {
                    accesskit::Toggled::True
                } else {
                    accesskit::Toggled::False
                });
            }
            if let Some(settings) = ui.splitter_settings(widget)
                && semantic.role == ControlRole::Splitter
            {
                node.set_min_numeric_value(0.0);
                node.set_max_numeric_value(1.0);
                node.set_numeric_value(f64::from(settings.ratio));
                node.set_numeric_value_step(0.02);
                node.set_orientation(match settings.axis {
                    rust_desktop_ui_core::Axis::Horizontal => accesskit::Orientation::Vertical,
                    rust_desktop_ui_core::Axis::Vertical => accesskit::Orientation::Horizontal,
                });
            }
            if semantic.has_popup {
                node.set_has_popup(if semantic.role == ControlRole::ComboBox {
                    accesskit::HasPopup::Listbox
                } else if ui.button_flyout(widget).is_some() {
                    accesskit::HasPopup::Dialog
                } else {
                    accesskit::HasPopup::Menu
                });
            }
            if let Some(expanded) = semantic.expanded {
                node.set_expanded(expanded);
            }
            if let Some(settings) = ui.editor_settings(widget) {
                append_description(&mut node, &settings.placeholder);
            }
            if let Some(tooltip) = semantic.tooltip {
                // Windows UIA exposes description as FullDescription/HelpText;
                // the tooltip property alone is not projected by this backend.
                append_description(&mut node, &tooltip);
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
            if let Some(grid_focus) =
                self.grids
                    .project(ui, widget, scale, &mut self.next, &mut node, &mut nodes)
            {
                focus = grid_focus;
            }
            nodes.push((id, node));
        }
        self.grids.finish();
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
        if self.grids.action(ui, &request)? {
            return Ok(());
        }
        let Some(widget) = self
            .widgets
            .iter()
            .find_map(|(widget, id)| (*id == request.target_node).then_some(*widget))
        else {
            return Ok(());
        };
        if grid::scroll(ui, widget, request.action)? {
            return Ok(());
        }
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
                if value.is_finite() && numeric_range(ui, widget).is_some() =>
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
            && let Some((minimum, maximum)) = numeric_range(ui, widget)
            && value
                .and_then(|value| value.parse::<f64>().ok())
                .is_none_or(|value| !value.is_finite() || !(minimum..=maximum).contains(&value))
        {
            // Native range requests are untrusted input. An invalid value
            // preserves state without terminating the application event loop.
            return Ok(());
        }
        ui.perform_action(widget, action, value)
            .map_err(|error| error.to_string())
    }
}

fn append_description(node: &mut Node, text: &str) {
    let mut parts: Vec<_> = node.description().unwrap_or_default().lines().collect();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if !parts.contains(&line) {
            parts.push(line);
        }
    }
    if !parts.is_empty() {
        node.set_description(parts.join("\n"));
    }
}

fn numeric_range(ui: &Controls, widget: WidgetId) -> Option<(f64, f64)> {
    ui.zoom_settings(widget)
        .map(|settings| (settings.minimum, settings.maximum))
        .or_else(|| {
            ui.slider_settings(widget)
                .map(|settings| (settings.minimum, settings.maximum))
        })
        .or_else(|| ui.splitter_settings(widget).map(|_| (0.0, 1.0)))
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
        ControlRole::PasswordBox => Role::PasswordInput,
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
        ControlRole::RangeSlider => Role::Group,
        ControlRole::ToolBar => Role::Toolbar,
        ControlRole::MenuBar => Role::MenuBar,
        ControlRole::Link => Role::Link,
        ControlRole::Switch => Role::Switch,
        ControlRole::Splitter => Role::Splitter,
        ControlRole::Grid => Role::Grid,
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

    #[test]
    fn sliders_project_range_thumbs_and_route_native_numeric_actions() {
        use rust_desktop_ui_controls::{RangeSliderSettings, SliderSettings, SplitterSettings};
        let mut fixture = Fixture::new(false);
        let root = fixture.ui.root();
        let slider = fixture
            .ui
            .slider(
                root,
                SliderSettings {
                    minimum: -20.0,
                    maximum: 80.0,
                    value: 10.0,
                    step: 2.5,
                    axis: rust_desktop_ui_core::Axis::Vertical,
                    ..Default::default()
                },
                LayoutStyle::default(),
            )
            .unwrap();
        let range = fixture
            .ui
            .range_slider(root, RangeSliderSettings::default(), LayoutStyle::default())
            .unwrap();
        let (lower, upper) = fixture.ui.range_slider_thumbs(range).unwrap();
        let split = fixture
            .ui
            .split_container(
                root,
                SplitterSettings::default(),
                LayoutStyle {
                    width: Length::Px(400.0),
                    height: Length::Px(80.0),
                    ..Default::default()
                },
            )
            .unwrap();
        fixture.ui.update(Size::new(800.0, 600.0)).unwrap();
        let tree = fixture.tree();
        let scalar = fixture.node(&tree, slider);
        assert_eq!(scalar.role(), Role::Slider);
        assert_eq!(scalar.orientation(), Some(accesskit::Orientation::Vertical));
        assert_eq!(scalar.min_numeric_value(), Some(-20.0));
        assert_eq!(scalar.max_numeric_value(), Some(80.0));
        assert_eq!(scalar.numeric_value_step(), Some(2.5));
        assert_eq!(fixture.node(&tree, range).role(), Role::Group);
        assert_eq!(fixture.node(&tree, range).numeric_value(), None);
        assert_eq!(fixture.node(&tree, lower).numeric_value(), Some(25.0));
        assert_eq!(fixture.node(&tree, upper).numeric_value(), Some(75.0));
        assert!(fixture.node(&tree, lower).supports_action(Action::SetValue));
        fixture.set_value(slider, 42.5).unwrap();
        fixture.set_value(lower, 30.0).unwrap();
        fixture.set_value(upper, 85.0).unwrap();
        fixture.set_value(split.divider, 0.75).unwrap();
        assert_eq!(fixture.ui.slider_value(slider), Some(42.5));
        assert_eq!(fixture.ui.range_slider_values(range), Some((30.0, 85.0)));
        assert_eq!(fixture.ui.split_ratio(split.container), Some(0.75));
        for invalid in [f64::NAN, f64::INFINITY, -100.0, 1000.0] {
            fixture.set_value(slider, invalid).unwrap();
            fixture.set_value(lower, invalid).unwrap();
            fixture.set_value(split.divider, invalid).unwrap();
        }
        assert_eq!(fixture.ui.slider_value(slider), Some(42.5));
        assert_eq!(fixture.ui.range_slider_values(range), Some((30.0, 85.0)));
        assert_eq!(fixture.ui.split_ratio(split.container), Some(0.75));
    }

    #[test]
    fn indeterminate_progress_omits_a_fabricated_numeric_percentage() {
        let mut fixture = Fixture::new(false);
        fixture
            .ui
            .set_progress_indeterminate(fixture.progress, true)
            .unwrap();
        let tree = fixture.tree();
        let node = fixture.node(&tree, fixture.progress);
        assert_eq!(node.role(), Role::ProgressIndicator);
        assert_eq!(node.numeric_value(), None);
        assert_eq!(node.min_numeric_value(), None);
        assert_eq!(node.max_numeric_value(), None);
        assert_eq!(node.value(), None);
        assert!(node.is_read_only());
        fixture
            .ui
            .set_progress_indeterminate(fixture.progress, false)
            .unwrap();
        let tree = fixture.tree();
        assert_eq!(
            fixture.node(&tree, fixture.progress).numeric_value(),
            Some(45.0)
        );
    }

    #[test]
    fn custom_editor_flyout_is_a_dialog_disclosure() {
        let mut fixture = Fixture::new(false);
        let button = fixture
            .ui
            .button(fixture.ui.root(), "Details", None, LayoutStyle::default())
            .unwrap();
        let flyout = fixture.ui.flyout(button, Size::new(240.0, 100.0)).unwrap();
        fixture
            .ui
            .edit_box(flyout.content, "Value", "Draft", LayoutStyle::default())
            .unwrap();
        fixture.ui.set_button_flyout(button, flyout).unwrap();
        fixture.ui.update(VIEWPORT).unwrap();
        let tree = fixture.tree();
        let node = fixture.node(&tree, button);
        assert_eq!(node.has_popup(), Some(accesskit::HasPopup::Dialog));
        assert_eq!(node.is_expanded(), Some(false));
        assert!(node.supports_action(Action::Expand));
    }

    #[test]
    fn rich_tooltip_description_preserves_placeholder_without_duplicate_help() {
        use rust_desktop_ui_controls::{EditorSettings, RichToolTip};
        use rust_desktop_ui_core::Bitmap;
        let mut fixture = Fixture::new(false);
        let root = fixture.ui.root();
        let button = fixture
            .ui
            .button(root, "Details", None, LayoutStyle::default())
            .unwrap();
        let editor = fixture
            .ui
            .edit_box(root, "Document code", "", LayoutStyle::default())
            .unwrap();
        fixture
            .ui
            .set_editor_settings(
                editor,
                EditorSettings {
                    placeholder: "Enter a document code".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let tip = RichToolTip {
            header: "Document details".into(),
            text: "Fields are checked before saving".into(),
            image: Some(Bitmap::new_rgba8(1, 1, vec![255; 4]).unwrap()),
            image_description: "Example document preview".into(),
            ..Default::default()
        };
        for owner in [button, editor] {
            fixture.ui.set_rich_tooltip(owner, tip.clone()).unwrap();
        }
        fixture.ui.update(VIEWPORT).unwrap();
        let tree = fixture.tree();
        let rich = "Document details\nFields are checked before saving\nExample document preview";
        let button_node = fixture.node(&tree, button);
        assert_eq!(button_node.tooltip(), Some(rich));
        assert_eq!(button_node.description(), Some(rich));
        let editor_node = fixture.node(&tree, editor);
        assert_eq!(editor_node.tooltip(), Some(rich));
        assert_eq!(
            editor_node.description(),
            Some(format!("Enter a document code\n{rich}").as_str())
        );
        assert_eq!(fixture.ui.tooltip_bounds(), None); // Native help is present before hover.
        fixture
            .ui
            .set_tooltip(editor, "Enter a document code")
            .unwrap();
        let tree = fixture.tree();
        assert_eq!(
            fixture.node(&tree, editor).description(),
            Some("Enter a document code")
        );
        assert_eq!(
            fixture.node(&tree, editor).tooltip(),
            Some("Enter a document code")
        );
    }
}
