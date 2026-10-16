//! Virtual grid nodes use the same allocator as retained widgets. Full-width
//! signed data keys are identities, never arithmetic pieces of native NodeIds.
use super::{Controls, Node, NodeId, Role, WidgetId, accesskit, bounds};
use rust_desktop_ui_controls::{ControlError, GridRowKind, GridViewMode};
use rust_desktop_ui_platform_winit::accesskit::{Action, ActionData, ActionRequest};
use rust_desktop_ui_treegrid::{ColumnId, GridHitKind, RowKey};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Part {
    HeaderRow,
    Header(ColumnId),
    Row(RowKey),
    Cell(RowKey, ColumnId),
    Summary(usize),
    Band(usize),
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
    identities: HashMap<(WidgetId, RowKey), GridRowKind>,
}

impl GridAccessibility {
    pub(super) fn begin(&mut self) {
        self.seen.clear();
    }

    pub(super) fn finish(&mut self) {
        // Only materialized rows/cells survive. Scrolling an arbitrarily large
        // source cannot grow this map with every previously visited row.
        self.ids.retain(|key, _| self.seen.contains(key));
        self.identities.retain(|(widget, row), _| {
            self.seen.contains(&Key {
                widget: *widget,
                part: Part::Row(*row),
            })
        });
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
        let presentation = ui.grid_presentation(widget)?;
        let cards = presentation.view == GridViewMode::Cards;
        if frame
            .semantics
            .rows
            .iter()
            .any(|r| r.depth > 0 || r.expanded.is_some())
        {
            node.set_role(Role::TreeGrid);
        }
        let arrangement = ui.grid_arrangement(widget)?;
        let order: HashMap<_, _> = arrangement
            .iter()
            .filter(|c| c.visible && (cards || c.pinned))
            .chain(
                arrangement
                    .iter()
                    .filter(|c| c.visible && !cards && !c.pinned),
            )
            .enumerate()
            .filter_map(|(rank, column)| {
                settings
                    .columns
                    .iter()
                    .position(|c| c.field == column.field)
                    .map(|i| (ColumnId(i as u64), rank))
            })
            .collect();
        let mut visible_columns: Vec<_> = frame.columns.iter().collect();
        visible_columns.sort_by_key(|c| order[&c.id]);
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
        for column in visible_columns.iter().filter(|_| !cards) {
            let Some(settings) = settings.columns.get(column.id.0 as usize) else {
                continue;
            };
            let id = self.id(widget, Part::Header(column.id), next);
            let mut header = Node::new(Role::ColumnHeader);
            header.set_label(settings.title.clone());
            header.set_column_index(order[&column.id]);
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
            let group_header = matches!(
                ui.grid_row_kind(widget, row.key),
                Some(GridRowKind::Group { .. })
            );
            if let Some(identity) = ui.grid_row_kind(widget, row.key) {
                if self
                    .identities
                    .get(&(widget, row.key))
                    .is_some_and(|old| *old != identity)
                {
                    self.ids.retain(|key, _| {
                        key.widget != widget
                            || !matches!(key.part,Part::Row(k)|Part::Cell(k,_) if k==row.key)
                    });
                }
                self.identities.insert((widget, row.key), identity);
            }
            let mut visible_cells: Vec<_> = row.cells.iter().collect();
            visible_cells.sort_by_key(|c| order[&c.column]);
            let id = self.id(widget, Part::Row(row.key), next);
            let mut row_node = Node::new(Role::Row);
            row_node.set_row_index(row.index);
            row_node.set_level(row.depth);
            row_node.set_label(
                visible_cells
                    .iter()
                    .map(|c| c.label.as_str())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · "),
            );
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
            for cell in visible_cells {
                let cell_id = self.id(widget, Part::Cell(row.key, cell.column), next);
                let mut cell_node = Node::new(Role::Cell);
                cell_node.set_label(cell.label.clone());
                cell_node.set_value(cell.label.clone());
                cell_node.set_row_index(row.index);
                cell_node.set_selected(row.selected);
                cell_node.set_column_index(order[&cell.column]);
                cell_node.set_bounds(bounds(cell.bounds, scale));
                let editable = cell.editable && !settings.is_read_only;
                if enabled {
                    cell_node.add_action(Action::Focus);
                    cell_node.add_action(Action::Click);
                    if editable {
                        cell_node.add_action(Action::SetValue);
                        // Native Cell remains a display value; the writable
                        // Value/RangeValue pattern belongs to its actual editor.
                        cell_node.set_description("F2 — редактировать значение");
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
                // A group is one disclosure target. Focusing its decorative
                // first cell hides the row's expanded/selected state from the
                // native reader; ordinary records retain column navigation.
                if has_focus && row.focused && !group_header && focused_column == Some(cell.column)
                {
                    focus = Some(cell_id);
                }
                cells.push(cell_id);
                nodes.push((cell_id, cell_node));
            }
            row_node.set_children(cells);
            nodes.push((id, row_node));
            children.push(id);
        }
        if let Some(host) = ui.runtime.bounds(widget) {
            for (index, (summary, value)) in presentation
                .summaries
                .iter()
                .zip(ui.grid_summaries(widget).unwrap_or(&[]))
                .enumerate()
            {
                let id = self.id(widget, Part::Summary(index), next);
                let mut summary_node = Node::new(Role::Label);
                summary_node.set_label(format!("{}: {}", summary.title, value.display()));
                summary_node.set_bounds(bounds(
                    rust_desktop_ui_core::Rect::new(
                        host.x,
                        host.y + (host.height - 28.0).max(0.0),
                        host.width,
                        28.0_f32.min(host.height),
                    ),
                    scale,
                ));
                nodes.push((id, summary_node));
                children.push(id);
            }
            if !cards {
                for (index, band) in presentation.bands.iter().enumerate() {
                    let id = self.id(widget, Part::Band(index), next);
                    let mut band_node = Node::new(Role::ColumnHeader);
                    band_node.set_label(band.title.clone());
                    band_node.set_read_only();
                    nodes.push((id, band_node));
                    children.push(id);
                }
            }
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
        // Ignore stale virtual IDs until update has reconciled both snapshots.
        if !ui.grid_snapshot_is_current(key.widget).unwrap_or(false) {
            return Ok(true);
        }
        let record = match ui.grid_row_kind(key.widget, row) {
            Some(GridRowKind::Record(key)) => Some(key),
            _ => None,
        };
        let field = column.and_then(|column| {
            ui.grid_settings(key.widget)?
                .columns
                .get(column.0 as usize)
                .map(|column| column.field.clone())
        });
        match request.action {
            Action::Focus | Action::Click => {
                ignore_invalid(ui.grid_focus_row(key.widget, row, column))?
            }
            Action::Expand | Action::Collapse if frame_row.expanded.is_some() => {
                ignore_invalid(ui.set_grid_expanded(
                    key.widget,
                    row,
                    request.action == Action::Expand,
                ))?;
            }
            Action::SetValue => {
                let Some(signed_key) = record else {
                    return Ok(true);
                };
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
                    let typed = ui.grid_source(key.widget).and_then(|source| {
                        let source = source.try_borrow().ok()?;
                        source.table().schema().field(&field)?.parse(value).ok()
                    });
                    let replacement = match typed {
                        Some(typed) => ui.set_grid_edit_value(key.widget, typed),
                        None if ui.document(editor).is_some() => ui.set_text(editor, value),
                        None => {
                            ignore_invalid(ui.cancel_grid_edit(key.widget))?;
                            return Ok(true);
                        }
                    };
                    match replacement {
                        Ok(()) => {}
                        // Whole-value requests can exceed the editor's byte or
                        // grapheme limits. They are rejected input, not a host
                        // failure; discard the temporary cell session.
                        Err(ControlError::Invalid(_) | ControlError::Edit(_)) => {
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
