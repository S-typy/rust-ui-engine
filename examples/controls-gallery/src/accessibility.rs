//! Example-specific mapping of controls and visible data rows into a single native tree.
use crate::gallery::Gallery;
use rust_desktop_ui_controls::{Role as ControlRole, SemanticAction};
use rust_desktop_ui_core::{Mutation, Rect};
use rust_desktop_ui_platform_winit::accesskit::{
    Action, ActionData, ActionRequest, HasPopup, Invalid, Node, NodeId, Role, Toggled, TreeId,
    TreeInfo, TreeUpdate,
};
use rust_desktop_ui_treegrid::{ColumnId, RowKey, SelectionModifiers};
use std::collections::HashSet;

const ROOT: NodeId = NodeId(1);
// BookSource keys are bounded to 4,000,000; this namespace cannot overlap widget IDs.
const ROW_BASE: u64 = 1 << 48;
fn row_id(row: RowKey) -> NodeId {
    NodeId(ROW_BASE + row.0 * 32 + 1)
}
fn cell_id(row: RowKey, column: ColumnId) -> NodeId {
    NodeId(ROW_BASE + row.0 * 32 + column.0 + 2)
}
fn bounds(bounds: Rect, scale: f32) -> rust_desktop_ui_platform_winit::accesskit::Rect {
    rust_desktop_ui_platform_winit::accesskit::Rect {
        x0: f64::from(bounds.x * scale),
        y0: f64::from(bounds.y * scale),
        x1: f64::from((bounds.x + bounds.width) * scale),
        y1: f64::from((bounds.y + bounds.height) * scale),
    }
}

impl Gallery {
    pub(crate) fn accessibility_tree(&mut self, scale: f32) -> TreeUpdate {
        let semantics = self.ui.semantics();
        let present: HashSet<_> = semantics.iter().map(|s| s.id).collect();
        self.accessible_widgets.retain(|id, _| present.contains(id));
        for item in &semantics {
            self.accessible_widgets.entry(item.id).or_insert_with(|| {
                let id = self.next_accessible_id;
                self.next_accessible_id += 1;
                id
            });
        }
        let mut root = Node::new(Role::Window);
        root.set_label("Rust UI Engine — галерея компонентов");
        root.set_children(
            semantics
                .iter()
                .filter(|s| !s.parent.is_some_and(|p| present.contains(&p)))
                .map(|s| NodeId(self.accessible_widgets[&s.id]))
                .collect::<Vec<_>>(),
        );
        let mut nodes = vec![(ROOT, root)];
        let mut focus = ROOT;
        for item in semantics {
            let id = NodeId(self.accessible_widgets[&item.id]);
            let mut node = Node::new(if item.id == self.grid_node {
                Role::TreeGrid
            } else if item.role == ControlRole::MenuItem && item.checked.is_some() {
                Role::MenuItemCheckBox
            } else {
                role(item.role)
            });
            node.set_label(item.label);
            node.set_bounds(bounds(item.bounds, scale));
            if item.role == ControlRole::StatusBar {
                node.set_live(rust_desktop_ui_platform_winit::accesskit::Live::Off);
            }
            if item.has_popup {
                node.set_has_popup(if item.role == ControlRole::ComboBox {
                    HasPopup::Listbox
                } else if self.ui.button_flyout(item.id).is_some() {
                    HasPopup::Dialog
                } else {
                    HasPopup::Menu
                });
            }
            if let Some(expanded) = item.expanded {
                node.set_expanded(expanded);
            }
            if let Some(settings) = self.ui.editor_settings(item.id) {
                append_description(&mut node, &settings.placeholder);
            }
            if let Some(tooltip) = item.tooltip {
                // Keep tooltip metadata while also supplying the description
                // consumed by the native Windows accessibility provider.
                append_description(&mut node, &tooltip);
                node.set_tooltip(tooltip);
            }
            if self.ui.progress_indeterminate(item.id) != Some(true)
                && let Some(value) = item.value
            {
                node.set_value(value);
            }
            if let Some(settings) = self.ui.progress_settings(item.id) {
                if self.ui.progress_indeterminate(item.id) != Some(true) {
                    node.set_min_numeric_value(settings.minimum);
                    node.set_max_numeric_value(settings.maximum);
                    node.set_numeric_value(settings.value);
                }
                node.set_read_only();
            }
            if let Some(settings) = self.ui.zoom_settings(item.id) {
                node.set_min_numeric_value(settings.minimum);
                node.set_max_numeric_value(settings.maximum);
                node.set_numeric_value(settings.value);
                node.set_numeric_value_step(settings.step);
                node.set_orientation(
                    rust_desktop_ui_platform_winit::accesskit::Orientation::Horizontal,
                );
            }
            if let Some(settings) = self.ui.slider_settings(item.id) {
                node.set_min_numeric_value(settings.minimum);
                node.set_max_numeric_value(settings.maximum);
                node.set_numeric_value(settings.value);
                node.set_numeric_value_step(settings.step);
                node.set_orientation(match settings.axis {
                    rust_desktop_ui_core::Axis::Horizontal => {
                        rust_desktop_ui_platform_winit::accesskit::Orientation::Horizontal
                    }
                    rust_desktop_ui_core::Axis::Vertical => {
                        rust_desktop_ui_platform_winit::accesskit::Orientation::Vertical
                    }
                });
            }
            if item.checked_mixed {
                node.set_toggled(Toggled::Mixed);
            } else if let Some(checked) = item.checked {
                node.set_toggled(if checked {
                    Toggled::True
                } else {
                    Toggled::False
                });
            }
            if let Some(settings) = self.ui.splitter_settings(item.id)
                && item.role == ControlRole::Splitter
            {
                node.set_min_numeric_value(0.0);
                node.set_max_numeric_value(1.0);
                node.set_numeric_value(f64::from(settings.ratio));
                node.set_numeric_value_step(0.02);
                node.set_orientation(match settings.axis {
                    rust_desktop_ui_core::Axis::Horizontal => {
                        rust_desktop_ui_platform_winit::accesskit::Orientation::Vertical
                    }
                    rust_desktop_ui_core::Axis::Vertical => {
                        rust_desktop_ui_platform_winit::accesskit::Orientation::Horizontal
                    }
                });
            }
            if matches!(item.role, ControlRole::Tab | ControlRole::ListItem) {
                node.set_selected(item.selected);
            }
            if item.disabled {
                node.set_disabled();
            }
            if item.read_only {
                node.set_read_only();
            }
            if let Some(error) = item.validation_error {
                node.set_invalid(Invalid::True);
                append_description(&mut node, &error);
            }
            for action in item.actions {
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
            let mut children: Vec<_> = item
                .children
                .into_iter()
                .filter_map(|child| self.accessible_widgets.get(&child).copied().map(NodeId))
                .collect();
            if item.focused {
                focus = id;
            }
            if item.id == self.grid_node && self.library_visible() {
                node.set_row_count(self.frame.semantics.row_count);
                node.set_column_count(self.frame.semantics.column_count);
                node.add_action(Action::ScrollDown);
                node.add_action(Action::ScrollUp);
                node.add_action(Action::ScrollLeft);
                node.add_action(Action::ScrollRight);
                for row in &self.frame.semantics.rows {
                    let mut row_node = Node::new(Role::Row);
                    row_node.set_row_index(row.index);
                    row_node.set_level(row.depth + 1);
                    row_node.set_selected(row.selected);
                    row_node.set_bounds(bounds(row.bounds, scale));
                    row_node.add_action(Action::Focus);
                    row_node.add_action(Action::Click);
                    if let Some(expanded) = row.expanded {
                        row_node.set_expanded(expanded);
                        row_node.add_action(if expanded {
                            Action::Collapse
                        } else {
                            Action::Expand
                        });
                    }
                    if row.busy {
                        row_node.set_busy();
                    }
                    let mut cells = Vec::new();
                    for cell in &row.cells {
                        let cell_id = cell_id(row.key, cell.column);
                        let mut cell_node = Node::new(Role::Cell);
                        cell_node.set_label(cell.label.clone());
                        cell_node.set_value(cell.label.clone());
                        cell_node.set_row_index(row.index);
                        if let Some(index) =
                            self.grid.columns().iter().position(|c| c.id == cell.column)
                        {
                            cell_node.set_column_index(index);
                        }
                        cell_node.set_bounds(bounds(cell.bounds, scale));
                        cell_node.add_action(Action::Focus);
                        cell_node.add_action(Action::Click);
                        if cell.editable {
                            cell_node.add_action(Action::SetValue);
                        } else {
                            cell_node.set_read_only();
                        }
                        if let Some(error) = &cell.validation_error {
                            cell_node.set_invalid(Invalid::True);
                            cell_node.set_description(error.clone());
                        }
                        if self.grid_focused()
                            && row.focused
                            && self.grid.focused_column() == Some(cell.column)
                        {
                            focus = cell_id;
                        }
                        cells.push(cell_id);
                        nodes.push((cell_id, cell_node));
                    }
                    if self.grid_focused() && row.focused && self.grid.focused_column().is_none() {
                        focus = row_id(row.key);
                    }
                    row_node.set_children(cells);
                    children.push(row_id(row.key));
                    nodes.push((row_id(row.key), row_node));
                }
            }
            node.set_children(children);
            nodes.push((id, node));
        }
        TreeUpdate {
            nodes,
            tree: Some(TreeInfo::new(ROOT)),
            tree_id: TreeId::ROOT,
            focus,
        }
    }

    pub(crate) fn accessibility_action(&mut self, request: ActionRequest) -> Result<(), String> {
        if request.target_tree != TreeId::ROOT {
            return Ok(());
        }
        let target = request.target_node;
        if target.0 >= ROW_BASE {
            let encoded = target.0 - ROW_BASE;
            let row = RowKey(encoded / 32);
            let part = encoded % 32;
            // Ignore stale/offscreen nodes; never turn arbitrary external IDs into model operations.
            if let Some(visible) = self.frame.semantics.rows.iter().find(|r| r.key == row) {
                let column = if part >= 2 {
                    Some(ColumnId(part - 2))
                } else {
                    None
                };
                if column
                    .is_some_and(|column| !visible.cells.iter().any(|cell| cell.column == column))
                {
                    return Ok(());
                }
                if matches!(
                    request.action,
                    Action::Focus | Action::Click | Action::Expand | Action::Collapse
                ) && !self.editor_finished()?
                {
                    return Ok(());
                }
                match request.action {
                    Action::Focus | Action::Click => {
                        if let Some(column) = column {
                            self.grid
                                .focus_cell(
                                    &self.source,
                                    row,
                                    column,
                                    SelectionModifiers::default(),
                                )
                                .map_err(|e| e.to_string())?;
                        } else {
                            self.grid
                                .select(&self.source, row, SelectionModifiers::default())
                                .map_err(|e| e.to_string())?;
                        }
                        self.ui
                            .runtime
                            .apply(Mutation::Focus(Some(self.grid_node)))
                            .map_err(|e| e.to_string())?;
                    }
                    Action::Expand | Action::Collapse => {
                        self.grid
                            .set_expanded(&mut self.source, row, request.action == Action::Expand)
                            .map_err(|e| e.to_string())?;
                    }
                    Action::SetValue => {
                        if let (Some(column), Some(ActionData::Value(value))) =
                            (column, request.data)
                        {
                            if !self.cell_edit_ready()? {
                                return Ok(());
                            }
                            let result = self
                                .grid
                                .begin_edit(&self.source, row, column)
                                .and_then(|()| self.grid.set_edit_text(value.as_ref()))
                                .and_then(|()| self.grid.commit_edit(&mut self.source));
                            if let Err(error) = result {
                                self.message = error.to_string();
                            }
                        }
                    }
                    _ => {}
                }
            }
        } else if let Some(widget) = self
            .accessible_widgets
            .iter()
            .find_map(|(widget, id)| (*id == target.0).then_some(*widget))
        {
            let delta = match request.action {
                Action::ScrollDown => (0.0, 140.0),
                Action::ScrollUp => (0.0, -140.0),
                Action::ScrollRight => (140.0, 0.0),
                Action::ScrollLeft => (-140.0, 0.0),
                _ => (0.0, 0.0),
            };
            if widget == self.grid_node && delta != (0.0, 0.0) {
                if !self.editor_finished()? {
                    return Ok(());
                }
                self.grid
                    .scroll_by(delta.0, delta.1)
                    .map_err(|e| e.to_string())?;
            } else {
                let action = match request.action {
                    Action::Focus => Some(SemanticAction::Focus),
                    Action::Click => Some(SemanticAction::Activate),
                    Action::SetValue => Some(SemanticAction::SetValue),
                    Action::Expand => Some(SemanticAction::Expand),
                    Action::Collapse => Some(SemanticAction::Collapse),
                    Action::ScrollDown | Action::ScrollUp => Some(SemanticAction::Scroll),
                    _ => None,
                };
                let value = match &request.data {
                    Some(ActionData::Value(value)) => Some(value.to_string()),
                    Some(ActionData::NumericValue(value))
                        if value.is_finite() && numeric_range(&self.ui, widget).is_some() =>
                    {
                        Some(value.to_string())
                    }
                    _ => None,
                };
                if let Some(action) = action {
                    if !self
                        .ui
                        .semantics()
                        .iter()
                        .any(|node| node.id == widget && node.actions.contains(&action))
                        || (action == SemanticAction::SetValue && value.is_none())
                    {
                        return Ok(());
                    }
                    if action == SemanticAction::SetValue
                        && let Some((minimum, maximum)) = numeric_range(&self.ui, widget)
                        && value
                            .as_deref()
                            .and_then(|value| value.parse::<f64>().ok())
                            .is_none_or(|value| {
                                !value.is_finite() || !(minimum..=maximum).contains(&value)
                            })
                    {
                        return Ok(());
                    }
                    let value = if action == SemanticAction::Scroll {
                        self.ui
                            .runtime
                            .tree()
                            .node(widget)
                            .map(|n| (n.props.scroll.y + delta.1 as f32).max(0.0).to_string())
                    } else {
                        value
                    };
                    self.ui
                        .perform_action(widget, action, value.as_deref())
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        self.process_activations()?;
        self.search_changed()?;
        Ok(())
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

fn numeric_range(
    ui: &rust_desktop_ui_controls::Controls,
    widget: rust_desktop_ui_core::WidgetId,
) -> Option<(f64, f64)> {
    ui.zoom_settings(widget)
        .map(|settings| (settings.minimum, settings.maximum))
        .or_else(|| {
            ui.slider_settings(widget)
                .map(|settings| (settings.minimum, settings.maximum))
        })
        .or_else(|| ui.splitter_settings(widget).map(|_| (0.0, 1.0)))
}

fn role(role: ControlRole) -> Role {
    match role {
        ControlRole::Panel => Role::GenericContainer,
        ControlRole::Label => Role::Label,
        ControlRole::Button => Role::Button,
        ControlRole::ToggleButton => Role::Button,
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
        ControlRole::Group => Role::Group,
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
