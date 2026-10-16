//! Bounded hierarchy projection using stable keys, never arithmetic NodeIds.
use super::{Controls, Node, NodeId, Role, SemanticAction, WidgetId, accesskit, bounds};
use rust_desktop_ui_platform_winit::accesskit::{Action, ActionRequest};
use rust_desktop_ui_treegrid::{ColumnId, GridHitKind, RowKey};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Part {
    HeaderRow,
    Header(ColumnId),
    Row(RowKey),
    Cell(RowKey, ColumnId),
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Key {
    widget: WidgetId,
    part: Part,
}
#[derive(Default)]
pub(super) struct TreeAccessibility {
    ids: HashMap<Key, NodeId>,
    seen: HashSet<Key>,
}
impl TreeAccessibility {
    pub(super) fn begin(&mut self) {
        self.seen.clear();
    }
    pub(super) fn finish(&mut self) {
        self.ids.retain(|key, _| self.seen.contains(key));
    }
    fn id(&mut self, widget: WidgetId, part: Part, next: &mut u64) -> NodeId {
        let key = Key { widget, part };
        self.seen.insert(key);
        *self.ids.entry(key).or_insert_with(|| {
            let id = NodeId(*next);
            *next += 1;
            id
        })
    }
    pub(super) fn project(
        &mut self,
        ui: &Controls,
        widget: WidgetId,
        scale: f32,
        next: &mut u64,
        node: &mut Node,
        nodes: &mut Vec<(NodeId, Node)>,
    ) -> Option<NodeId> {
        let frame = ui.tree_frame(widget)?;
        let settings = ui.tree_settings(widget)?;
        let model = ui.tree_model(widget)?;
        // Painting submits pinned columns last for occlusion. Accessibility
        // reading order follows their visual placement, independently of paint.
        let column_order: HashMap<_, _> = model
            .columns()
            .iter()
            .filter(|c| c.pinned)
            .chain(model.columns().iter().filter(|c| !c.pinned))
            .enumerate()
            .map(|(index, column)| (column.id, index))
            .collect();
        let mut visual_columns: Vec<_> = frame.columns.iter().collect();
        visual_columns.sort_by_key(|column| column_order[&column.id]);
        let enabled = ui.is_enabled(widget);
        let has_focus = ui.runtime.focused() == Some(widget);
        if settings.show_header {
            node.set_role(Role::TreeGrid);
            node.set_row_count(frame.semantics.row_count);
            node.set_column_count(frame.semantics.column_count);
        }
        node.set_read_only();
        if settings.selection_mode == rust_desktop_ui_controls::SelectionMode::Multiple {
            node.set_multiselectable();
        }
        if let Some(error) = ui.tree_error(widget) {
            node.set_invalid(accesskit::Invalid::True);
            node.set_description(error);
        }
        if enabled {
            for action in [
                Action::ScrollDown,
                Action::ScrollUp,
                Action::ScrollLeft,
                Action::ScrollRight,
            ] {
                node.add_action(action);
            }
        }
        let mut children = node.children().to_vec();
        if settings.show_header {
            let mut headers = Vec::new();
            for column in &visual_columns {
                let id = self.id(widget, Part::Header(column.id), next);
                let mut header = Node::new(Role::ColumnHeader);
                header.set_label(
                    model
                        .columns()
                        .iter()
                        .find(|c| c.id == column.id)?
                        .title
                        .clone(),
                );
                header.set_column_index(column_order[&column.id]);
                header.set_read_only();
                if !enabled {
                    header.set_disabled();
                }
                if let Some(hit) = frame
                    .hits
                    .iter()
                    .find(|hit| hit.kind == GridHitKind::Header(column.id))
                {
                    header.set_bounds(bounds(hit.bounds, scale));
                }
                headers.push(id);
                nodes.push((id, header));
            }
            if !headers.is_empty() {
                let id = self.id(widget, Part::HeaderRow, next);
                let mut row = Node::new(Role::Row);
                row.set_children(headers);
                nodes.push((id, row));
                children.push(id);
            }
        }
        let mut focus = None;
        let mut row_nodes = HashMap::new();
        let mut row_links = Vec::new();
        for row in &frame.semantics.rows {
            let mut visual_cells: Vec<_> = row.cells.iter().collect();
            visual_cells.sort_by_key(|cell| column_order[&cell.column]);
            let id = self.id(widget, Part::Row(row.key), next);
            let mut row_node = Node::new(if settings.show_header {
                Role::Row
            } else {
                Role::TreeItem
            });
            row_node.set_label(
                visual_cells
                    .iter()
                    .map(|cell| cell.label.as_str())
                    .collect::<Vec<_>>()
                    .join(" · "),
            );
            // AccessKit levels are zero-based; platform adapters expose native numbering.
            row_node.set_level(row.depth);
            row_node.set_position_in_set(row.position_in_set.saturating_sub(1));
            row_node.set_size_of_set(row.set_size);
            row_node.set_row_index(row.index);
            row_node.set_selected(row.selected);
            row_node.set_bounds(bounds(row.bounds, scale));
            row_node.set_read_only();
            if enabled {
                row_node.add_action(Action::Focus);
                row_node.add_action(Action::Click);
            } else {
                row_node.set_disabled();
            }
            if let Some(expanded) = row.expanded {
                row_node.set_expanded(expanded);
                if enabled {
                    row_node.add_action(if expanded {
                        Action::Collapse
                    } else {
                        Action::Expand
                    });
                }
            }
            if row.busy {
                row_node.set_busy();
            }
            if has_focus && row.focused {
                focus = Some(id);
            }
            if settings.show_header {
                let mut cells = Vec::new();
                for cell in &visual_cells {
                    let cell_id = self.id(widget, Part::Cell(row.key, cell.column), next);
                    let mut cell_node = Node::new(Role::Cell);
                    cell_node.set_label(cell.label.clone());
                    cell_node.set_value(cell.label.clone());
                    cell_node.set_row_index(row.index);
                    cell_node.set_selected(row.selected);
                    cell_node.set_bounds(bounds(cell.bounds, scale));
                    cell_node.set_read_only();
                    cell_node.set_column_index(column_order[&cell.column]);
                    if enabled {
                        cell_node.add_action(Action::Focus);
                        cell_node.add_action(Action::Click);
                    } else {
                        cell_node.set_disabled();
                    }
                    if has_focus && row.focused && model.focused_column() == Some(cell.column) {
                        focus = Some(cell_id);
                    }
                    cells.push(cell_id);
                    nodes.push((cell_id, cell_node));
                }
                row_node.set_children(cells);
            }
            row_links.push((row.key, row.parent, id));
            row_nodes.insert(row.key, nodes.len());
            nodes.push((id, row_node));
        }
        for (_, parent, id) in row_links {
            if let Some(index) = parent.and_then(|key| row_nodes.get(&key)).copied() {
                let mut nested = nodes[index].1.children().to_vec();
                nested.push(id);
                nodes[index].1.set_children(nested);
            } else {
                children.push(id);
            }
        }
        node.set_children(children);
        focus
    }
    pub(super) fn action(
        &self,
        ui: &mut Controls,
        request: &ActionRequest,
    ) -> Result<bool, String> {
        let Some(key) = self
            .ids
            .iter()
            .find_map(|(key, id)| (*id == request.target_node).then_some(*key))
        else {
            return Ok(false);
        };
        if !ui.is_enabled(key.widget) || ui.runtime.bounds(key.widget).is_none() {
            return Ok(true);
        }
        let (row, column) = match key.part {
            Part::Row(row) => (row, None),
            Part::Cell(row, column) => (row, Some(column)),
            _ => return Ok(true),
        };
        let Some(materialized) = ui
            .tree_frame(key.widget)
            .and_then(|frame| frame.semantics.rows.iter().find(|r| r.key == row))
        else {
            return Ok(true);
        };
        if column.is_some_and(|column| !materialized.cells.iter().any(|cell| cell.column == column))
        {
            return Ok(true);
        }
        let action = match request.action {
            Action::Focus => SemanticAction::Focus,
            Action::Click => SemanticAction::Activate,
            Action::Expand if materialized.expanded == Some(false) => SemanticAction::Expand,
            Action::Collapse if materialized.expanded == Some(true) => SemanticAction::Collapse,
            _ => return Ok(true),
        };
        ignore_invalid(ui.tree_perform_action(key.widget, row, column, action))?;
        Ok(true)
    }
}
pub(super) fn scroll(ui: &mut Controls, widget: WidgetId, action: Action) -> Result<bool, String> {
    let delta = match action {
        Action::ScrollDown => (0.0, 140.0),
        Action::ScrollUp => (0.0, -140.0),
        Action::ScrollLeft => (-140.0, 0.0),
        Action::ScrollRight => (140.0, 0.0),
        _ => return Ok(false),
    };
    let Some(model) = ui.tree_model(widget) else {
        return Ok(false);
    };
    let (x, y) = model.scroll_offset();
    if ui.is_enabled(widget) && ui.runtime.bounds(widget).is_some() {
        ignore_invalid(ui.tree_scroll_to(widget, x + delta.0, y + delta.1))?;
    }
    Ok(true)
}
fn ignore_invalid(
    result: Result<(), rust_desktop_ui_controls::ControlError>,
) -> Result<(), String> {
    match result {
        Ok(()) | Err(rust_desktop_ui_controls::ControlError::Invalid(_)) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::super::Accessibility;
    use super::*;
    use crate::view::MountedView;
    use rust_desktop_ui_controls::{Theme, TreeControlSettings, TreeSourceHandle};
    use rust_desktop_ui_core::{LayoutStyle, Length, Size};
    use rust_desktop_ui_treegrid::{GridColumn, IndexedTreeSource, TreeNode};
    fn request(target: NodeId, action: Action) -> ActionRequest {
        ActionRequest {
            target_tree: accesskit::TreeId::ROOT,
            target_node: target,
            action,
            data: None,
        }
    }
    #[test]
    fn pinned_columns_follow_visual_reading_order_after_reorder_and_repin() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let mut record = TreeNode::new(RowKey(1), None, "Project");
        record.cells.insert(ColumnId(1), "Folder".into());
        record.cells.insert(ColumnId(2), "Open".into());
        let mut columns = vec![
            GridColumn::new(ColumnId(0), "Name", 160.0),
            GridColumn::new(ColumnId(1), "Type", 160.0),
            GridColumn::new(ColumnId(2), "State", 160.0),
        ];
        columns[0].pinned = true;
        let owner = ui
            .tree_grid(
                ui.root(),
                TreeSourceHandle::new(IndexedTreeSource::new(vec![record]).unwrap()),
                TreeControlSettings {
                    columns,
                    ..Default::default()
                },
                LayoutStyle {
                    width: Length::Px(520.0),
                    height: Length::Px(240.0),
                    ..Default::default()
                },
            )
            .unwrap();
        let mut accessibility = Accessibility::default();
        for (step, (headers, labels)) in [
            (["Name", "Type", "State"], ["Project", "Folder", "Open"]),
            (["Name", "State", "Type"], ["Project", "Open", "Folder"]),
            (["State", "Name", "Type"], ["Open", "Project", "Folder"]),
        ]
        .into_iter()
        .enumerate()
        {
            if step == 1 {
                ui.tree_reorder_column(owner, ColumnId(2), Some(ColumnId(1)))
                    .unwrap();
            }
            if step == 2 {
                ui.tree_pin_column(owner, ColumnId(0), false).unwrap();
                ui.tree_pin_column(owner, ColumnId(2), true).unwrap();
            }
            ui.update(Size::new(640.0, 480.0)).unwrap();
            let update = accessibility.tree(&ui, &MountedView::default(), "Tree", 1.0);
            let find = |part| {
                let id = accessibility.trees.ids[&Key {
                    widget: owner,
                    part,
                }];
                &update
                    .nodes
                    .iter()
                    .find(|(candidate, _)| *candidate == id)
                    .unwrap()
                    .1
            };
            let header = find(Part::HeaderRow);
            let names: Vec<_> = header
                .children()
                .iter()
                .map(|id| {
                    update
                        .nodes
                        .iter()
                        .find(|(key, _)| key == id)
                        .unwrap()
                        .1
                        .label()
                        .unwrap()
                })
                .collect();
            assert_eq!(names, headers);
            let row = find(Part::Row(RowKey(1)));
            assert_eq!(row.label(), Some(labels.join(" · ").as_str()));
            let cells: Vec<_> = row
                .children()
                .iter()
                .map(|id| &update.nodes.iter().find(|(key, _)| key == id).unwrap().1)
                .collect();
            assert_eq!(
                cells.iter().map(|n| n.label().unwrap()).collect::<Vec<_>>(),
                labels
            );
            assert_eq!(
                cells
                    .iter()
                    .map(|n| n.column_index().unwrap())
                    .collect::<Vec<_>>(),
                [0, 1, 2]
            );
        }
    }
    #[test]
    fn hierarchy_projection_actions_positions_and_focus_match_both_presentations() {
        for header in [false, true] {
            let mut ui = Controls::new(Theme::FluentLight).unwrap();
            let source = IndexedTreeSource::new(vec![
                TreeNode::new(RowKey(u64::MAX), None, "Parent"),
                TreeNode::new(RowKey(1), Some(RowKey(u64::MAX)), "Child"),
            ])
            .unwrap();
            let owner = ui
                .tree_grid(
                    ui.root(),
                    TreeSourceHandle::new(source),
                    TreeControlSettings {
                        show_header: header,
                        columns: vec![GridColumn::new(ColumnId(0), "Name", 240.0)],
                        ..Default::default()
                    },
                    LayoutStyle {
                        width: Length::Px(400.0),
                        height: Length::Px(220.0),
                        ..Default::default()
                    },
                )
                .unwrap();
            let mut accessibility = Accessibility::default();
            let view = MountedView::default();
            ui.update(Size::new(640.0, 480.0)).unwrap();
            let update = accessibility.tree(&ui, &view, "Tree", 1.5);
            let host = accessibility.id(owner).unwrap();
            assert_eq!(
                update
                    .nodes
                    .iter()
                    .find(|(id, _)| *id == host)
                    .unwrap()
                    .1
                    .role(),
                if header { Role::TreeGrid } else { Role::Tree }
            );
            let row = accessibility.trees.ids[&Key {
                widget: owner,
                part: Part::Row(RowKey(u64::MAX)),
            }];
            let node = &update.nodes.iter().find(|(id, _)| *id == row).unwrap().1;
            assert_eq!(node.role(), if header { Role::Row } else { Role::TreeItem });
            assert_eq!(node.level(), Some(0));
            assert_eq!(node.position_in_set(), Some(0));
            assert_eq!(node.size_of_set(), Some(1));
            assert_eq!(node.is_expanded(), Some(false));
            accessibility
                .action(&mut ui, request(row, Action::Expand))
                .unwrap();
            ui.update(Size::new(640.0, 480.0)).unwrap();
            let expanded = accessibility.tree(&ui, &view, "Tree", 1.5);
            let child = accessibility.trees.ids[&Key {
                widget: owner,
                part: Part::Row(RowKey(1)),
            }];
            assert_eq!(
                expanded
                    .nodes
                    .iter()
                    .find(|(id, _)| *id == child)
                    .unwrap()
                    .1
                    .level(),
                Some(1)
            );
            assert!(
                expanded
                    .nodes
                    .iter()
                    .find(|(id, _)| *id == row)
                    .unwrap()
                    .1
                    .children()
                    .contains(&child)
            );
            accessibility
                .action(&mut ui, request(child, Action::Focus))
                .unwrap();
            ui.update(Size::new(640.0, 480.0)).unwrap();
            let update = accessibility.tree(&ui, &view, "Tree", 1.5);
            assert_eq!(ui.tree_selected_keys(owner).unwrap(), [RowKey(1)]);
            let expected = if header {
                accessibility.trees.ids[&Key {
                    widget: owner,
                    part: Part::Cell(RowKey(1), ColumnId(0)),
                }]
            } else {
                child
            };
            assert_eq!(update.focus, expected);
            ui.set_enabled(owner, false).unwrap();
            accessibility
                .action(&mut ui, request(row, Action::Collapse))
                .unwrap();
            assert!(ui.tree_model(owner).unwrap().is_expanded(RowKey(u64::MAX)));
            ui.set_enabled(owner, true).unwrap();
            accessibility
                .action(&mut ui, request(row, Action::Collapse))
                .unwrap();
            ui.update(Size::new(640.0, 480.0)).unwrap();
            accessibility.tree(&ui, &view, "Tree", 1.5);
            assert!(!accessibility.trees.ids.values().any(|id| *id == child));
            accessibility
                .action(&mut ui, request(child, Action::Click))
                .unwrap();
            assert_eq!(ui.tree_selected_keys(owner).unwrap(), [RowKey(1)]);
        }
    }
}
