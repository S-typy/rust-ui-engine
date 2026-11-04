//! Worksheet projection uses the common platform Grid/Table provider.
use super::*;
use crate::accessibility::Accessibility;
use accesskit::{Action, Node, NodeId, Role};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Part {
    Row(u32),
    Column(u32),
    Cell(CellAddress),
}

#[derive(Default)]
pub(super) struct SheetAccessibility {
    ids: HashMap<Part, NodeId>,
    sheet: Option<usize>,
    generation: u64,
    root: Option<NodeId>,
    virtual_generation: usize,
    virtual_identity: Option<(usize, u64)>,
    focused_cell: Option<(usize, CellAddress, NodeId, Node)>,
}

impl SheetAccessibility {
    pub(super) fn clear(&mut self) {
        self.ids.clear();
        self.sheet = None;
        self.root = None;
        self.virtual_identity = None;
        self.focused_cell = None;
    }

    pub(super) fn project(
        &mut self,
        sheet: &Spreadsheet,
        ui: &rust_desktop_ui_controls::Controls,
        allocator: &mut Accessibility,
        tree: &mut accesskit::TreeUpdate,
        scale: f32,
    ) -> Result<(), String> {
        let index = sheet.session().sheet();
        if self.sheet != Some(index) {
            self.clear();
        }
        let identity = (index, sheet.session().workbook().revision());
        if self.virtual_identity != Some(identity) {
            self.virtual_generation = usize::try_from(allocator.allocate_virtual_id().0)
                .map_err(|_| "Accessibility generation exhausted")?;
            self.virtual_identity = Some(identity);
        }
        self.sheet = Some(index);
        self.generation = sheet.session().generation();
        let Some(root) = allocator.id(sheet.parts.canvas) else {
            self.clear();
            return Ok(());
        };
        let Some(position) = tree.nodes.iter().position(|(id, _)| *id == root) else {
            self.clear();
            return Ok(());
        };
        self.root = Some(root);
        let snapshot = sheet.semantics(ui).map_err(err)?;
        let mut seen = HashSet::new();
        let mut children = Vec::new();
        let mut allocate = |part| {
            seen.insert(part);
            *self
                .ids
                .entry(part)
                .or_insert_with(|| allocator.allocate_virtual_id())
        };
        let rect = |b: Rect| {
            accesskit::Rect::new(
                (b.x * scale) as f64,
                (b.y * scale) as f64,
                ((b.x + b.width) * scale) as f64,
                ((b.y + b.height) * scale) as f64,
            )
        };
        let enabled = ui.is_enabled(sheet.parts.canvas);
        for (column, headers) in [(true, &snapshot.columns), (false, &snapshot.rows)] {
            for header in headers {
                let id = allocate(if column {
                    Part::Column(header.index)
                } else {
                    Part::Row(header.index)
                });
                let mut node = Node::new(if column {
                    Role::ColumnHeader
                } else {
                    Role::RowHeader
                });
                node.set_label(header.label.clone());
                node.set_bounds(rect(header.bounds));
                if column {
                    node.set_column_index(header.index as usize)
                } else {
                    node.set_row_index(header.index as usize)
                }
                node.set_read_only();
                if !enabled {
                    node.set_disabled();
                }
                children.push(id);
                tree.nodes.push((id, node));
            }
        }
        let canvas_focused = ui.runtime.focused() == Some(sheet.parts.canvas);
        let mut focused_visible = false;
        for cell in snapshot.cells {
            let id = allocate(Part::Cell(cell.address));
            let mut node = Node::new(Role::Cell);
            node.set_label(format!("{}: {}", cell.address.a1(), cell.value));
            node.set_value(cell.value);
            node.set_bounds(rect(cell.bounds));
            node.set_row_index(cell.address.row() as usize);
            node.set_column_index(cell.address.column() as usize);
            node.set_row_span((cell.range.end().row() - cell.range.start().row() + 1) as usize);
            node.set_column_span(
                (cell.range.end().column() - cell.range.start().column() + 1) as usize,
            );
            node.set_selected(cell.selected);
            node.set_read_only();
            node.set_description("Enter/F2: edit in the formula bar");
            if enabled {
                node.add_action(Action::Focus);
                node.add_action(Action::Click)
            } else {
                node.set_disabled()
            }
            if canvas_focused && sheet.session().active() == cell.address {
                tree.focus = id;
                focused_visible = true;
                self.focused_cell = Some((self.virtual_generation, cell.address, id, node.clone()));
            }
            children.push(id);
            tree.nodes.push((id, node));
        }
        if canvas_focused
            && !focused_visible
            && let Some((generation, address, id, node)) = &self.focused_cell
            && *generation == self.virtual_generation
            && *address == sheet.session().active()
        {
            let mut node = node.clone();
            node.clear_bounds();
            children.push(*id);
            tree.nodes.push((*id, node));
            seen.insert(Part::Cell(*address));
            tree.focus = *id;
        } else if !canvas_focused || !focused_visible {
            self.focused_cell = None;
        }
        self.ids.retain(|key, _| seen.contains(key));
        let node = &mut tree.nodes[position].1;
        node.set_role(Role::Grid);
        node.set_row_count(snapshot.row_count as usize);
        node.set_column_count(snapshot.column_count as usize);
        node.set_virtualization_generation(self.virtual_generation);
        node.set_virtual_cell_spans(
            sheet.session().workbook().sheets()[index]
                .merges()
                .iter()
                .map(|range| accesskit::VirtualCellSpan {
                    row: range.start().row() as usize,
                    column: range.start().column() as usize,
                    rows: (range.end().row() - range.start().row() + 1) as usize,
                    columns: (range.end().column() - range.start().column() + 1) as usize,
                })
                .collect(),
        );
        if sheet.can_realize_accessible_cell(ui) {
            node.add_action(Action::RealizeCell);
        }
        node.set_multiselectable();
        node.set_children(children);
        Ok(())
    }

    pub(super) fn action(
        &self,
        sheet: &mut Spreadsheet,
        ui: &mut rust_desktop_ui_controls::Controls,
        request: &accesskit::ActionRequest,
    ) -> Result<bool, String> {
        if request.target_tree != accesskit::TreeId::ROOT {
            return Ok(false);
        }
        if self.root == Some(request.target_node) && request.action == Action::RealizeCell {
            if let Some(accesskit::ActionData::RealizeCell {
                row,
                column,
                generation,
            }) = request.data
                && generation == self.virtual_generation
                && self.sheet == Some(sheet.session().sheet())
                && self.generation == sheet.session().generation()
                && let (Ok(row), Ok(column)) = (u32::try_from(row), u32::try_from(column))
                && let Ok(address) = CellAddress::new(row, column)
            {
                sheet.realize_accessible_cell(ui, address).map_err(err)?;
            }
            return Ok(true);
        }
        let Some(part) = self
            .ids
            .iter()
            .find_map(|(part, id)| (*id == request.target_node).then_some(*part))
        else {
            return Ok(false);
        };
        if self.sheet != Some(sheet.session().sheet())
            || self.generation != sheet.session().generation()
        {
            return Ok(true);
        }
        if let Part::Cell(address) = part
            && matches!(request.action, Action::Focus | Action::Click)
        {
            sheet.select_accessible_cell(ui, address).map_err(err)?;
        }
        Ok(true)
    }
}
