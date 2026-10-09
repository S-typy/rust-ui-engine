//! Example-specific mapping of controls and visible data rows into a single native tree.
use crate::gallery::Gallery;
use rust_desktop_ui_controls::{Role as ControlRole, SemanticAction};
use rust_desktop_ui_core::{Mutation, Rect};
use rust_desktop_ui_platform_winit::accesskit::{
    Action, ActionData, ActionRequest, Invalid, Node, NodeId, Role, Toggled, TreeId, TreeInfo,
    TreeUpdate,
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
            } else {
                role(item.role)
            });
            node.set_label(item.label);
            node.set_bounds(bounds(item.bounds, scale));
            if let Some(value) = item.value {
                node.set_value(value);
            }
            if let Some(checked) = item.checked {
                node.set_toggled(if checked {
                    Toggled::True
                } else {
                    Toggled::False
                });
            }
            if item.role == ControlRole::Tab {
                node.set_selected(item.selected);
            }
            if item.disabled {
                node.set_disabled();
            }
            for action in item.actions {
                match action {
                    SemanticAction::Focus => node.add_action(Action::Focus),
                    SemanticAction::Activate | SemanticAction::Toggle => {
                        node.add_action(Action::Click)
                    }
                    SemanticAction::SetValue => node.add_action(Action::SetValue),
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
                    Action::ScrollDown | Action::ScrollUp => Some(SemanticAction::Scroll),
                    _ => None,
                };
                let value = match &request.data {
                    Some(ActionData::Value(value)) => Some(value.to_string()),
                    _ => None,
                };
                if let Some(action) = action {
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

fn role(role: ControlRole) -> Role {
    match role {
        ControlRole::Panel => Role::GenericContainer,
        ControlRole::Label => Role::Label,
        ControlRole::Button => Role::Button,
        ControlRole::ToggleButton => Role::Button,
        ControlRole::CheckBox => Role::CheckBox,
        ControlRole::TextBox => Role::TextInput,
        ControlRole::ScrollView => Role::ScrollView,
        ControlRole::TabList => Role::TabList,
        ControlRole::Tab => Role::Tab,
        ControlRole::Menu => Role::Menu,
        ControlRole::MenuItem => Role::MenuItem,
        ControlRole::Group => Role::Group,
    }
}
