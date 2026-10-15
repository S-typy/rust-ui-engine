//! Virtual grid nodes use the same allocator as retained widgets. Full-width
//! signed data keys are identities, never arithmetic pieces of native NodeIds.
use super::{Controls, Node, NodeId, Role, WidgetId, accesskit, bounds};
use rust_desktop_ui_controls::ControlError;
use rust_desktop_ui_platform_winit::accesskit::{Action, ActionData, ActionRequest};
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
pub(super) struct GridAccessibility {
    ids: HashMap<Key, NodeId>,
    seen: HashSet<Key>,
}

impl GridAccessibility {
    pub(super) fn begin(&mut self) {
        self.seen.clear();
    }

    pub(super) fn finish(&mut self) {
        // Only materialized rows/cells survive. Scrolling an arbitrarily large
        // source cannot grow this map with every previously visited row.
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
        let frame = ui.grid_frame(widget)?;
        let settings = ui.grid_settings(widget)?;
        let enabled = ui.is_enabled(widget);
        let has_focus = ui.runtime.focused() == Some(widget);
        let focused_column = ui.grid_focused_column(widget);
        node.set_row_count(frame.semantics.row_count);
        node.set_column_count(frame.semantics.column_count);
        if settings.is_read_only {
            node.set_read_only();
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
        let mut headers = Vec::new();
        for column in &frame.columns {
            let Some(settings) = settings.columns.get(column.id.0 as usize) else {
                continue;
            };
            let id = self.id(widget, Part::Header(column.id), next);
            let mut header = Node::new(Role::ColumnHeader);
            header.set_label(settings.title.clone());
            header.set_column_index(column.index);
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
        let mut focus = None;
        for row in &frame.semantics.rows {
            let id = self.id(widget, Part::Row(row.key), next);
            let mut row_node = Node::new(Role::Row);
            row_node.set_row_index(row.index);
            row_node.set_selected(row.selected);
            row_node.set_bounds(bounds(row.bounds, scale));
            if enabled {
                row_node.add_action(Action::Focus);
                row_node.add_action(Action::Click);
            } else {
                row_node.set_disabled();
            }
            if row.busy {
                row_node.set_busy();
            }
            if has_focus && row.focused {
                focus = Some(id);
            }
            let mut cells = Vec::new();
            for cell in &row.cells {
                let cell_id = self.id(widget, Part::Cell(row.key, cell.column), next);
                let mut cell_node = Node::new(Role::Cell);
                cell_node.set_label(cell.label.clone());
                cell_node.set_value(cell.label.clone());
                cell_node.set_row_index(row.index);
                cell_node.set_selected(row.selected);
                if let Some(column) = frame.columns.iter().find(|column| column.id == cell.column) {
                    cell_node.set_column_index(column.index);
                }
                cell_node.set_bounds(bounds(cell.bounds, scale));
                let editable = cell.editable && !settings.is_read_only;
                if enabled {
                    cell_node.add_action(Action::Focus);
                    cell_node.add_action(Action::Click);
                    if editable {
                        cell_node.add_action(Action::SetValue);
                    }
                } else {
                    cell_node.set_disabled();
                }
                if !editable {
                    cell_node.set_read_only();
                }
                if let Some(message) = &cell.validation_error {
                    cell_node.set_invalid(accesskit::Invalid::True);
                    cell_node.set_description(message.clone());
                }
                if cell.editing
                    && let Some(message) = ui.grid_error(widget)
                {
                    cell_node.set_invalid(accesskit::Invalid::True);
                    cell_node.set_description(message);
                }
                if has_focus && row.focused && focused_column == Some(cell.column) {
                    focus = Some(cell_id);
                }
                cells.push(cell_id);
                nodes.push((cell_id, cell_node));
            }
            row_node.set_children(cells);
            nodes.push((id, row_node));
            children.push(id);
        }
        node.set_children(children);
        focus
    }

    /// True means the target belongs to this virtual projection, including an
    /// ignored request. Invalid external requests must never close the window.
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
        let Some(frame_row) = ui.grid_frame(key.widget).and_then(|frame| {
            frame
                .semantics
                .rows
                .iter()
                .find(|candidate| candidate.key == row)
        }) else {
            return Ok(true);
        };
        let cell =
            column.and_then(|column| frame_row.cells.iter().find(|cell| cell.column == column));
        if column.is_some() && cell.is_none() {
            return Ok(true);
        }
        let signed_key = i64::from_ne_bytes(row.0.to_ne_bytes());
        // A source may have changed since the last projected frame.
        if ui
            .grid_source(key.widget)
            .is_none_or(|source| source.borrow().position(signed_key).is_none())
        {
            return Ok(true);
        }
        let field = column.and_then(|column| {
            ui.grid_settings(key.widget)?
                .columns
                .get(column.0 as usize)
                .map(|column| column.field.clone())
        });
        match request.action {
            Action::Focus | Action::Click => {
                ignore_invalid(ui.grid_focus_cell(key.widget, signed_key, field.as_deref()))?
            }
            Action::SetValue => {
                if !cell.is_some_and(|cell| cell.editable) || ui.grid_editor(key.widget).is_some() {
                    return Ok(true);
                }
                let (Some(field), Some(ActionData::Value(value))) = (field, &request.data) else {
                    return Ok(true);
                };
                if value.len() > 1_048_576 {
                    return Ok(true);
                }
                match ui.begin_grid_edit(key.widget, signed_key, &field) {
                    Ok(()) => {}
                    Err(ControlError::Invalid(_)) => return Ok(true),
                    Err(error) => return Err(error.to_string()),
                }
                if let Some(editor) = ui.grid_editor(key.widget) {
                    match ui.set_text(editor, value) {
                        Ok(()) => {}
                        Err(ControlError::Invalid(_)) => {
                            ignore_invalid(ui.cancel_grid_edit(key.widget))?;
                            return Ok(true);
                        }
                        Err(error) => return Err(error.to_string()),
                    }
                    ignore_invalid(ui.commit_grid_edit(key.widget).map(|_| ()))?;
                }
            }
            _ => {}
        }
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
    if ui.grid_settings(widget).is_none() {
        return Ok(false);
    }
    if ui.is_enabled(widget) && ui.runtime.bounds(widget).is_some() {
        ignore_invalid(ui.grid_scroll_by(widget, delta.0, delta.1))?;
    }
    Ok(true)
}

fn ignore_invalid(result: Result<(), ControlError>) -> Result<(), String> {
    match result {
        Ok(()) | Err(ControlError::Invalid(_)) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
#[path = "accessibility_grid_tests.rs"]
mod tests;
