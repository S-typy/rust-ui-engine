//! Retained aggregate projection over GridControl, with typed path provenance.
use super::*;
use rust_desktop_ui_core::{GridPlacement, GridTrack};
use rust_desktop_ui_data::{
    CellValue, CollectionAdapter, DataRow, DataSource, DataTable, FieldSchema, FieldType,
    PivotDefinition, PivotDrillDown, PivotPath, PivotPathId, PivotSnapshot, PivotSourceId,
    PivotSourceVersion, TableSchema,
};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PivotAxis {
    Rows,
    Columns,
}
#[derive(Clone, Debug)]
pub struct PivotParts {
    pub container: WidgetId,
    pub row_fields: WidgetId,
    pub column_fields: WidgetId,
    pub grid: WidgetId,
    pub status: WidgetId,
    pub expand_row: WidgetId,
    pub collapse_row: WidgetId,
    pub expand_column: WidgetId,
    pub collapse_column: WidgetId,
    pub drill_down: WidgetId,
}
#[derive(Clone, Debug)]
pub struct PivotDrillEvent {
    pub row: PivotPathId,
    pub column: PivotPathId,
    pub measure: usize,
    pub data: PivotDrillDown,
}
#[derive(Clone, Copy)]
enum Action {
    ExpandRow,
    CollapseRow,
    ExpandColumn,
    CollapseColumn,
    Drill,
}
#[derive(Clone)]
struct Column {
    path: PivotPathId,
    measure: usize,
    field: String,
}
struct Projection {
    table: DataTable,
    settings: GridControlSettings,
    rows: Vec<(i64, PivotPathId)>,
    columns: Vec<Column>,
    identities: HashMap<PivotPathId, i64>,
    next: i64,
}
#[derive(Clone)]
struct Drag {
    field: String,
    start: Point,
    widget: WidgetId,
    changed: bool,
}
/// Owns a stable Grid and field chrome. Call `before_input` before Controls dispatch,
/// `update` before the frame update, and `cancel_pointer` on native capture loss.
/// No provider I/O is performed; source synchronization remains application owned.
pub struct PivotView {
    pub parts: PivotParts,
    source: DataSourceHandle,
    projection_source: DataSourceHandle,
    projection_revision: u64,
    source_id: PivotSourceId,
    snapshot: PivotSnapshot,
    expanded_rows: HashSet<PivotPathId>,
    expanded_columns: HashSet<PivotPathId>,
    rows: Vec<(i64, PivotPathId)>,
    columns: Vec<Column>,
    identities: HashMap<PivotPathId, i64>,
    next: i64,
    fields: HashMap<String, WidgetId>,
    parents: Vec<(WidgetId, WidgetId)>,
    commands: Vec<CommandId>,
    actions: Rc<RefCell<VecDeque<Action>>>,
    events: VecDeque<PivotDrillEvent>,
    drag: Option<Drag>,
    error: Option<String>,
    rejected_revision: Option<u64>,
}
fn fail(e: impl ToString) -> ControlError {
    ControlError::Invalid(e.to_string())
}
fn cell(row: u16) -> LayoutStyle {
    LayoutStyle {
        grid_row: GridPlacement {
            start: Some(row + 1),
            span: 1,
        },
        ..Default::default()
    }
}
fn visible(paths: &[PivotPath], expanded: &HashSet<PivotPathId>) -> Vec<PivotPath> {
    let mut result = Vec::new();
    let mut shown = HashSet::new();
    for p in paths {
        if p.parent().is_none()
            || p.parent()
                .is_some_and(|id| shown.contains(id) && expanded.contains(id))
        {
            shown.insert(p.id().clone());
            result.push(p.clone());
        }
    }
    result
}
fn caption(path: &PivotPath) -> String {
    if path.depth() == 0 {
        return "Итого".into();
    }
    path.values()
        .iter()
        .map(|v| match v {
            CellValue::Null => "NULL".into(),
            CellValue::Text(t) if t.is_empty() => "\"\"".into(),
            v => v.display(),
        })
        .collect::<Vec<_>>()
        .join(" / ")
}
fn kind(v: &CellValue) -> FieldType {
    match v {
        CellValue::Bool(_) => FieldType::Bool,
        CellValue::Integer(_) => FieldType::Integer,
        CellValue::Real(_) => FieldType::Real,
        CellValue::Decimal(_) => FieldType::Decimal,
        CellValue::Date(_) => FieldType::Date,
        CellValue::Time(_) => FieldType::Time,
        CellValue::Binary(_) => FieldType::Binary,
        _ => FieldType::Text,
    }
}
impl PivotView {
    pub fn mount(
        ui: &mut Controls,
        parent: WidgetId,
        source: DataSourceHandle,
        definition: PivotDefinition,
        mut style: LayoutStyle,
    ) -> Result<Self, ControlError> {
        let source_id = PivotSourceId::new();
        let snapshot = {
            let d = source.try_borrow().map_err(fail)?;
            PivotSnapshot::build(
                d.table(),
                PivotSourceVersion {
                    source: source_id.clone(),
                    revision: d.revision(),
                },
                &definition,
            )
            .map_err(fail)?
        };
        let mut er = HashSet::new();
        er.insert(snapshot.row_paths()[0].id().clone());
        let mut ec = HashSet::new();
        ec.insert(snapshot.column_paths()[0].id().clone());
        let projected = Self::project(ui, &snapshot, &er, &ec, &HashMap::new(), 1)?;
        style.min_size.width = style.min_size.width.max(500.0);
        style.kind = LayoutKind::Grid;
        style.grid_columns = vec![GridTrack::Fr(1.0)];
        style.grid_rows = vec![
            GridTrack::Px(42.0),
            GridTrack::Px(42.0),
            GridTrack::Auto,
            GridTrack::Fr(1.0),
            GridTrack::Px(28.0),
        ];
        style.gap = 4.0;
        let owner = ui.panel(parent, style)?;
        let mut commands = Vec::new();
        let result = (|| {
            let rows = ui.panel(
                owner,
                LayoutStyle {
                    kind: LayoutKind::Stack(Axis::Horizontal),
                    gap: 6.0,
                    ..cell(0)
                },
            )?;
            let cols = ui.panel(
                owner,
                LayoutStyle {
                    kind: LayoutKind::Stack(Axis::Horizontal),
                    gap: 6.0,
                    ..cell(1)
                },
            )?;
            ui.label(
                rows,
                "Строки",
                LayoutStyle {
                    width: Length::Px(90.0),
                    ..Default::default()
                },
            )?;
            ui.label(
                cols,
                "Столбцы",
                LayoutStyle {
                    width: Length::Px(90.0),
                    ..Default::default()
                },
            )?;
            let mut fields = HashMap::new();
            for (axis, names, host) in [
                (PivotAxis::Rows, &definition.row_fields, rows),
                (PivotAxis::Columns, &definition.column_fields, cols),
            ] {
                for name in names {
                    let display = Self::field_caption(ui, name)?;
                    let b = ui.button(
                        host,
                        display,
                        None,
                        LayoutStyle {
                            width: Length::Px(170.0),
                            height: Length::Px(36.0),
                            ..Default::default()
                        },
                    )?;
                    ui.set_automation_name(
                        b,
                        format!(
                            "Поле {name}, {}. Ctrl+стрелки: перенести или упорядочить",
                            if axis == PivotAxis::Rows {
                                "строки"
                            } else {
                                "столбцы"
                            }
                        ),
                    )?;
                    ui.set_tooltip(b, name)?;
                    fields.insert(name.clone(), b);
                }
            }
            let toolbar = ui.panel(
                owner,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    width: Length::Percent(1.0),
                    height: Length::Px(76.0),
                    grid_columns: vec![GridTrack::Px(160.0); 3],
                    grid_rows: vec![GridTrack::Px(36.0); 2],
                    gap: 4.0,
                    ..cell(2)
                },
            )?;
            let actions = Rc::new(RefCell::new(VecDeque::new()));
            let mut buttons = Vec::new();
            for (index, (label, action)) in [
                ("Раскрыть строки", Action::ExpandRow),
                ("Свернуть строки", Action::CollapseRow),
                ("Раскрыть столбцы", Action::ExpandColumn),
                ("Свернуть столбцы", Action::CollapseColumn),
                ("Исходные записи", Action::Drill),
            ]
            .into_iter()
            .enumerate()
            {
                let command = CommandId::from(format!("pivot.{owner:?}.{index}"));
                ui.commands_mut().register(command.clone(), label);
                commands.push(command.clone());
                let queue = actions.clone();
                ui.commands_mut().on_execute(&command, move || {
                    let mut q = queue.borrow_mut();
                    if q.len() < 32 {
                        q.push_back(action)
                    }
                });
                let q = actions.clone();
                ui.commands_mut()
                    .set_can_execute(&command, move || q.borrow().len() < 32);
                let b = ui.button(
                    toolbar,
                    label,
                    Some(command.clone()),
                    LayoutStyle {
                        width: Length::Px(160.0),
                        height: Length::Px(36.0),
                        ..Default::default()
                    },
                )?;
                buttons.push(b);
            }
            let handle = Rc::new(RefCell::new(
                DataSource::new(CollectionAdapter::new(projected.table)).map_err(fail)?,
            ));
            let projection_source = handle.clone();
            let grid = ui.grid_control(
                owner,
                handle,
                projected.settings,
                LayoutStyle {
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    ..cell(3)
                },
            )?;
            ui.set_automation_name(grid, "Сводная таблица")?;
            let status = ui.label(
                owner,
                "",
                LayoutStyle {
                    height: Length::Px(28.0),
                    ..cell(4)
                },
            )?;
            let parts = PivotParts {
                container: owner,
                row_fields: rows,
                column_fields: cols,
                grid,
                status,
                expand_row: buttons[0],
                collapse_row: buttons[1],
                expand_column: buttons[2],
                collapse_column: buttons[3],
                drill_down: buttons[4],
            };
            let mut parents = Vec::new();
            let mut stack = vec![owner];
            while let Some(id) = stack.pop() {
                for child in ui.runtime.tree().children(id).unwrap_or(&[]) {
                    parents.push((*child, id));
                    stack.push(*child);
                }
            }
            ui.set_label(
                status,
                format!(
                    "Исходных записей: {} · Строк: {} · Столбцов: {}",
                    snapshot.matched_row_count(),
                    projected.rows.len(),
                    projected.columns.len()
                ),
            )?;
            let result = Self {
                parts,
                source,
                projection_source,
                projection_revision: 0,
                source_id,
                snapshot,
                expanded_rows: er,
                expanded_columns: ec,
                rows: projected.rows,
                columns: projected.columns,
                identities: projected.identities,
                next: projected.next,
                fields,
                parents,
                commands: std::mem::take(&mut commands),
                actions,
                events: VecDeque::new(),
                drag: None,
                error: None,
                rejected_revision: None,
            };
            Ok(result)
        })();
        if result.is_err() {
            for command in &commands {
                ui.commands_mut().unregister(command);
            }
            let _ = ui.remove(owner);
        }
        result
    }
    pub fn dispose(mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.cancel_pointer(ui)?;
        for command in &self.commands {
            ui.commands_mut().unregister(command);
        }
        if ui.runtime.tree().node(self.parts.container).is_some() {
            ui.remove(self.parts.container)?;
        }
        Ok(())
    }
    fn field_caption(ui: &mut Controls, name: &str) -> Result<String, ControlError> {
        let normalized = name.replace(['\r', '\n', '\t'], " ");
        let name = normalized.as_str();
        if ui.measure_text(name, 4096.0, false)?.width <= 132.0 {
            return Ok(name.into());
        }
        for end in TextDocument::new(name)
            .grapheme_boundaries()
            .into_iter()
            .rev()
        {
            let text = format!("{}…", &name[..end]);
            if ui.measure_text(&text, 4096.0, false)?.width <= 132.0 {
                return Ok(text);
            }
        }
        Ok("…".into())
    }
    pub fn snapshot(&self) -> &PivotSnapshot {
        &self.snapshot
    }
    pub fn field_widget(&self, field: &str) -> Option<WidgetId> {
        self.fields.get(field).copied()
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn selected_row_path(&self, ui: &Controls) -> Option<&PivotPathId> {
        let key = ui.grid_selected_key(self.parts.grid)?;
        self.rows.iter().find(|(k, _)| *k == key).map(|(_, p)| p)
    }
    pub fn selected_column_path(&self, ui: &Controls) -> Option<(&PivotPathId, usize)> {
        let i = ui.grid_focused_column(self.parts.grid)?.0.checked_sub(1)? as usize;
        self.columns.get(i).map(|c| (&c.path, c.measure))
    }
    pub fn take_drill_down_events(&mut self) -> Vec<PivotDrillEvent> {
        self.events.drain(..).collect()
    }
    pub fn cancel_pointer(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        if let Some(d) = self.drag.take()
            && ui.runtime.captured() == Some(d.widget)
        {
            ui.runtime.apply(Mutation::CapturePointer(None))?;
        }
        Ok(())
    }
    fn check(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        if ui.runtime.tree().node(self.parts.container).is_none()
            || self
                .parents
                .iter()
                .any(|(id, parent)| ui.runtime.tree().parent(*id) != Some(*parent))
        {
            self.cancel_pointer(ui)?;
            return Err(fail("Pivot required part was removed or reparented"));
        }
        match ui.check_pivot_grid(
            self.parts.grid,
            &self.projection_source,
            self.projection_revision,
        ) {
            Ok(revision) => {
                self.projection_revision = revision;
                Ok(())
            }
            Err(error) => {
                self.cancel_pointer(ui)?;
                Err(error)
            }
        }
    }
    fn status(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        let text = self.error.clone().unwrap_or_else(|| {
            format!(
                "Исходных записей: {} · Строк: {} · Столбцов: {}",
                self.snapshot.matched_row_count(),
                self.rows.len(),
                self.columns.len()
            )
        });
        ui.set_label(self.parts.status, text)
    }
    fn report(&mut self, ui: &mut Controls, error: ControlError) -> Result<(), ControlError> {
        let text: String = error.to_string().chars().take(140).collect();
        self.error = Some(if ui.measure_text(&text, 1200.0, false).is_ok() {
            text
        } else {
            "Pivot operation rejected".into()
        });
        self.status(ui)
    }
    fn project(
        ui: &mut Controls,
        snapshot: &PivotSnapshot,
        er: &HashSet<PivotPathId>,
        ec: &HashSet<PivotPathId>,
        old: &HashMap<PivotPathId, i64>,
        mut next: i64,
    ) -> Result<Projection, ControlError> {
        let rp = visible(snapshot.row_paths(), er);
        let cp = visible(snapshot.column_paths(), ec);
        let measures = &snapshot.definition().measures;
        let count = cp
            .len()
            .checked_mul(measures.len())
            .ok_or_else(|| fail("Pivot columns overflow"))?;
        if count + 1 > 64 || rp.len().checked_mul(count + 1).is_none_or(|n| n > 100_000) {
            return Err(fail(
                "Pivot presentation exceeds 64 columns or 100000 cells",
            ));
        }
        let mut identities = HashMap::new();
        for path in snapshot.row_paths() {
            let key = if let Some(k) = old.get(path.id()) {
                *k
            } else {
                let k = next;
                next = next
                    .checked_add(1)
                    .ok_or_else(|| fail("Pivot row identity exhausted"))?;
                k
            };
            identities.insert(path.id().clone(), key);
        }
        let mut columns = Vec::new();
        let mut axis_field = FieldSchema::new("axis", FieldType::Text);
        axis_field.read_only = true;
        let mut schema = vec![axis_field];
        let mut settings = vec![GridControlColumn::new("axis", "Строки / итоги", 260.0)];
        settings[0].pinned = true;
        for (ci, path) in cp.iter().enumerate() {
            for (m, measure) in measures.iter().enumerate() {
                let field = format!("pivot_{ci}_{m}");
                let title = format!("{} · {}", caption(path), measure.title);
                if title.len() > 4096 {
                    return Err(fail("Generated pivot column caption exceeds 4096 bytes"));
                }
                let title_width = ui.measure_text(&title, 4096.0, false)?.width;
                let sample = rp
                    .iter()
                    .map(|r| snapshot.value(r.id(), path.id(), m))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(fail)?
                    .into_iter()
                    .find(|v| !matches!(v, CellValue::Null));
                let mut f = FieldSchema::new(&field, sample.map_or(FieldType::Text, kind));
                f.nullable = true;
                f.read_only = true;
                schema.push(f);
                settings.push(GridControlColumn::new(
                    &field,
                    title,
                    (title_width + 24.0).clamp(240.0, 420.0),
                ));
                columns.push(Column {
                    path: path.id().clone(),
                    measure: m,
                    field,
                });
            }
        }
        let parent_ids: HashSet<_> = snapshot
            .row_paths()
            .iter()
            .filter_map(|p| p.parent())
            .collect();
        let mut data = Vec::new();
        let mut rows = Vec::new();
        for path in rp {
            let has_children = parent_ids.contains(path.id());
            let marker = if has_children {
                if er.contains(path.id()) { "− " } else { "+ " }
            } else {
                "  "
            };
            let label = format!("{}{}{}", "  ".repeat(path.depth()), marker, caption(&path));
            ui.measure_text(&label, 4096.0, false)?;
            let key = identities[path.id()];
            let mut values = vec![CellValue::Text(label)];
            for c in &columns {
                let value = snapshot
                    .value(path.id(), &c.path, c.measure)
                    .map_err(fail)?
                    .clone();
                ui.measure_text(&value.display(), 4096.0, false)?;
                values.push(value);
            }
            rows.push((key, path.id().clone()));
            data.push(DataRow::new(key, values));
        }
        let table = DataTable::new(TableSchema::new(schema).map_err(fail)?, data).map_err(fail)?;
        Ok(Projection {
            table,
            settings: GridControlSettings {
                columns: settings,
                is_read_only: true,
            },
            rows,
            columns,
            identities,
            next,
        })
    }
    fn install(
        &mut self,
        ui: &mut Controls,
        snapshot: PivotSnapshot,
        er: HashSet<PivotPathId>,
        ec: HashSet<PivotPathId>,
    ) -> Result<(), ControlError> {
        self.check(ui)?;
        let candidate = Self::project(ui, &snapshot, &er, &ec, &self.identities, self.next)?;
        let mut selected = self.selected_row_path(ui).cloned();
        while selected
            .as_ref()
            .is_some_and(|p| !candidate.rows.iter().any(|(_, id)| id == p))
        {
            selected = selected.and_then(|id| {
                self.snapshot
                    .row_paths()
                    .iter()
                    .find(|p| p.id() == &id)
                    .and_then(|p| p.parent().cloned())
            });
        }
        let selected = selected
            .and_then(|p| {
                candidate
                    .rows
                    .iter()
                    .find(|(_, id)| *id == p)
                    .map(|(k, _)| *k)
            })
            .or_else(|| candidate.rows.first().map(|(k, _)| *k));
        let focused = self.selected_column_path(ui).and_then(|(path, measure)| {
            candidate
                .columns
                .iter()
                .find(|c| &c.path == path && c.measure == measure)
                .map(|c| c.field.clone())
        });
        let visible: HashMap<_, _> = candidate
            .rows
            .iter()
            .map(|(key, id)| (id.clone(), *key))
            .collect();
        let parents: HashMap<_, _> = self
            .snapshot
            .row_paths()
            .iter()
            .map(|p| (p.id().clone(), p.parent().cloned()))
            .collect();
        let first = candidate.rows.first().map(|(key, _)| *key);
        let mut anchors = HashMap::new();
        for (key, path) in &self.rows {
            if !visible.contains_key(path) {
                let mut current = parents.get(path).cloned().flatten();
                let mut fallback = None;
                while let Some(id) = current {
                    if let Some(k) = visible.get(&id) {
                        fallback = Some(*k);
                        break;
                    }
                    current = parents.get(&id).cloned().flatten();
                }
                if let Some(k) = fallback.or(first) {
                    anchors.insert(*key, k);
                }
            }
        }
        let handle = Rc::new(RefCell::new(
            DataSource::new(CollectionAdapter::new(candidate.table)).map_err(fail)?,
        ));
        ui.replace_pivot_grid(
            self.parts.grid,
            handle.clone(),
            candidate.settings,
            selected,
            focused.as_deref(),
            &anchors,
        )?;
        self.projection_source = handle;
        self.projection_revision = 0;
        self.snapshot = snapshot;
        self.expanded_rows = er;
        self.expanded_columns = ec;
        self.rows = candidate.rows;
        self.columns = candidate.columns;
        self.identities = candidate.identities;
        self.next = candidate.next;
        self.error = None;
        self.rejected_revision = None;
        self.status(ui)
    }
    pub fn set_definition(
        &mut self,
        ui: &mut Controls,
        definition: PivotDefinition,
    ) -> Result<(), ControlError> {
        self.check(ui)?;
        let old_fields: HashSet<_> = self.fields.keys().cloned().collect();
        let new_fields: HashSet<_> = definition
            .row_fields
            .iter()
            .chain(&definition.column_fields)
            .cloned()
            .collect();
        if old_fields != new_fields {
            return Err(fail(
                "Retained Pivot definition must preserve its field catalog; remount to replace catalog",
            ));
        }
        let snapshot = {
            let source = self.source.try_borrow().map_err(fail)?;
            PivotSnapshot::build(
                source.table(),
                PivotSourceVersion {
                    source: self.source_id.clone(),
                    revision: source.revision(),
                },
                &definition,
            )
            .map_err(fail)?
        };
        let rows = self
            .expanded_rows
            .iter()
            .filter(|id| snapshot.row_paths().iter().any(|p| p.id() == *id))
            .cloned()
            .chain(std::iter::once(snapshot.row_paths()[0].id().clone()))
            .collect();
        let cols = self
            .expanded_columns
            .iter()
            .filter(|id| snapshot.column_paths().iter().any(|p| p.id() == *id))
            .cloned()
            .chain(std::iter::once(snapshot.column_paths()[0].id().clone()))
            .collect();
        self.install(ui, snapshot, rows, cols)?;
        self.cancel_pointer(ui)?;
        for (names, host) in [
            (&definition.row_fields, self.parts.row_fields),
            (&definition.column_fields, self.parts.column_fields),
        ] {
            for name in names {
                let id = self.fields[name];
                ui.runtime.apply(Mutation::Reparent {
                    node: id,
                    parent: host,
                })?;
                for pair in &mut self.parents {
                    if pair.0 == id {
                        pair.1 = host;
                    }
                }
                ui.set_automation_name(
                    id,
                    format!(
                        "Поле {name}, {}",
                        if host == self.parts.row_fields {
                            "строки"
                        } else {
                            "столбцы"
                        }
                    ),
                )?;
            }
        }
        Ok(())
    }
    pub fn set_expanded(
        &mut self,
        ui: &mut Controls,
        axis: PivotAxis,
        path: &PivotPathId,
        expanded: bool,
    ) -> Result<(), ControlError> {
        let paths = if axis == PivotAxis::Rows {
            self.snapshot.row_paths()
        } else {
            self.snapshot.column_paths()
        };
        if !paths.iter().any(|p| p.id() == path) {
            return Err(fail("Unknown pivot path"));
        }
        let (mut rows, mut cols) = (self.expanded_rows.clone(), self.expanded_columns.clone());
        let set = if axis == PivotAxis::Rows {
            &mut rows
        } else {
            &mut cols
        };
        if expanded {
            set.insert(path.clone());
        } else {
            set.remove(path);
        }
        self.install(ui, self.snapshot.clone(), rows, cols)
    }
    pub fn move_field(
        &mut self,
        ui: &mut Controls,
        field: &str,
        axis: PivotAxis,
        index: usize,
    ) -> Result<(), ControlError> {
        if !ui.is_enabled(self.parts.container) {
            return Err(fail("Pivot is unavailable"));
        }
        let mut definition = self.snapshot.definition().clone();
        if !self.fields.contains_key(field) {
            return Err(fail("Unknown pivot field"));
        }
        definition.row_fields.retain(|f| f != field);
        definition.column_fields.retain(|f| f != field);
        let target = if axis == PivotAxis::Rows {
            &mut definition.row_fields
        } else {
            &mut definition.column_fields
        };
        if target.len() >= 2 || index > target.len() {
            return Err(fail("Pivot axis insertion exceeds its field limit"));
        }
        target.insert(index, field.into());
        self.set_definition(ui, definition)
    }
    pub fn request_drill_down(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.check(ui)?;
        if self.events.len() >= 32 {
            return Err(fail("Pivot drill-down queue is full"));
        }
        let row = self
            .selected_row_path(ui)
            .cloned()
            .unwrap_or_else(|| self.snapshot.row_paths()[0].id().clone());
        let (column, measure) = self
            .selected_column_path(ui)
            .map(|(p, m)| (p.clone(), m))
            .unwrap_or_else(|| (self.snapshot.column_paths()[0].id().clone(), 0));
        let version = {
            let s = self.source.try_borrow().map_err(fail)?;
            PivotSourceVersion {
                source: self.source_id.clone(),
                revision: s.revision(),
            }
        };
        let data = self
            .snapshot
            .drill_down(&row, &column, &version)
            .map_err(fail)?;
        self.events.push_back(PivotDrillEvent {
            row,
            column,
            measure,
            data,
        });
        Ok(())
    }
    fn action(&mut self, ui: &mut Controls, action: Action) -> Result<(), ControlError> {
        match action {
            Action::Drill => self.request_drill_down(ui),
            a => {
                let axis = if matches!(a, Action::ExpandRow | Action::CollapseRow) {
                    PivotAxis::Rows
                } else {
                    PivotAxis::Columns
                };
                let path = if axis == PivotAxis::Rows {
                    self.selected_row_path(ui)
                        .cloned()
                        .unwrap_or_else(|| self.snapshot.row_paths()[0].id().clone())
                } else {
                    self.selected_column_path(ui)
                        .map(|(p, _)| p.clone())
                        .unwrap_or_else(|| self.snapshot.column_paths()[0].id().clone())
                };
                self.set_expanded(
                    ui,
                    axis,
                    &path,
                    matches!(a, Action::ExpandRow | Action::ExpandColumn),
                )
            }
        }
    }
    pub fn update(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        self.check(ui)?;
        if !ui.effective_enabled(self.parts.container) {
            self.cancel_pointer(ui)?;
            self.actions.borrow_mut().clear();
        }
        let revision = match self.source.try_borrow() {
            Ok(source) => Ok(source.revision()),
            Err(error) => Err(fail(error)),
        };
        let revision = match revision {
            Ok(v) => v,
            Err(error) => {
                self.cancel_pointer(ui)?;
                return Err(error);
            }
        };
        if revision != self.snapshot.source_version().revision
            && self.rejected_revision != Some(revision)
        {
            self.cancel_pointer(ui)?;
            if let Err(e) = self.set_definition(ui, self.snapshot.definition().clone()) {
                self.rejected_revision = Some(revision);
                self.report(ui, e)?;
            }
        }
        let actions = self.actions.borrow_mut().drain(..).collect::<Vec<_>>();
        for action in actions {
            if ui.effective_enabled(self.parts.container)
                && let Err(e) = self.action(ui, action)
            {
                self.report(ui, e)?;
            }
        }
        let available = ui.effective_enabled(self.parts.container);
        for command in &self.commands {
            ui.commands_mut().set_enabled(command, available);
        }
        Ok(())
    }
    pub fn before_input(
        &mut self,
        ui: &mut Controls,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        if matches!(
            event,
            InputEvent::WindowFocused(false)
                | InputEvent::KeyDown {
                    key: Key::Escape,
                    ..
                }
        ) && self.drag.is_some()
        {
            self.cancel_pointer(ui)?;
            return Ok(true);
        }
        self.update(ui)?;
        if !ui.effective_enabled(self.parts.container) {
            self.cancel_pointer(ui)?;
            return Ok(false);
        }
        if let Some(mut drag) = self.drag.clone() {
            if ui.runtime.captured() != Some(drag.widget) {
                self.cancel_pointer(ui)?;
                return Ok(false);
            }
            match event {
                InputEvent::PointerMoved { position } => {
                    drag.changed |=
                        (position.x - drag.start.x).abs() + (position.y - drag.start.y).abs() > 4.0;
                    self.drag = Some(drag);
                    return Ok(true);
                }
                InputEvent::PointerUp {
                    position,
                    button: PointerButton::Primary,
                    ..
                } => {
                    self.cancel_pointer(ui)?;
                    if drag.changed {
                        let hit = [
                            (PivotAxis::Rows, self.parts.row_fields),
                            (PivotAxis::Columns, self.parts.column_fields),
                        ]
                        .into_iter()
                        .find(|(_, id)| {
                            ui.runtime
                                .bounds(*id)
                                .zip(ui.runtime.clip(*id))
                                .and_then(|(b, c)| b.intersection(c))
                                .is_some_and(|r| r.contains(position.x, position.y))
                        });
                        if let Some((axis, host)) = hit {
                            let names = if axis == PivotAxis::Rows {
                                &self.snapshot.definition().row_fields
                            } else {
                                &self.snapshot.definition().column_fields
                            };
                            let index = names
                                .iter()
                                .filter(|f| *f != &drag.field)
                                .filter(|f| {
                                    ui.runtime
                                        .bounds(self.fields[*f])
                                        .is_some_and(|r| position.x > r.x + r.width / 2.0)
                                })
                                .count();
                            if let Err(e) = self.move_field(ui, &drag.field, axis, index) {
                                self.report(ui, e)?;
                            }
                            let _ = host;
                        }
                    }
                    return Ok(true);
                }
                _ => {}
            }
        }
        match event {
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                ..
            } => {
                let hit = ui.runtime.hit_test(*position);
                if let Some((field, widget)) = self.fields.iter().find(|(_, id)| Some(**id) == hit)
                {
                    let field = field.clone();
                    let widget = *widget;
                    ui.runtime.apply(Mutation::Focus(Some(widget)))?;
                    ui.synchronize_editor_focus()?;
                    ui.runtime.apply(Mutation::CapturePointer(Some(widget)))?;
                    self.drag = Some(Drag {
                        field,
                        start: *position,
                        widget,
                        changed: false,
                    });
                    return Ok(true);
                }
            }
            InputEvent::KeyDown {
                key,
                modifiers,
                repeat: false,
            } if modifiers.control => {
                if let Some((field, _)) = self
                    .fields
                    .iter()
                    .find(|(_, id)| Some(**id) == ui.runtime.focused())
                {
                    let field = field.clone();
                    let definition = self.snapshot.definition();
                    let axis = if definition.row_fields.contains(&field) {
                        PivotAxis::Rows
                    } else {
                        PivotAxis::Columns
                    };
                    let names = if axis == PivotAxis::Rows {
                        &definition.row_fields
                    } else {
                        &definition.column_fields
                    };
                    let index = names.iter().position(|f| f == &field).unwrap();
                    let target = match key {
                        Key::ArrowLeft => Some((axis, index.saturating_sub(1))),
                        Key::ArrowRight => Some((axis, (index + 1).min(names.len() - 1))),
                        Key::ArrowUp => Some((
                            PivotAxis::Rows,
                            definition
                                .row_fields
                                .iter()
                                .filter(|f| *f != &field)
                                .count(),
                        )),
                        Key::ArrowDown => Some((
                            PivotAxis::Columns,
                            definition
                                .column_fields
                                .iter()
                                .filter(|f| *f != &field)
                                .count(),
                        )),
                        _ => None,
                    };
                    if let Some((axis, index)) = target {
                        if let Err(e) = self.move_field(ui, &field, axis, index) {
                            self.report(ui, e)?;
                        }
                        return Ok(true);
                    }
                }
            }
            _ => {}
        }
        Ok(false)
    }
}
#[cfg(test)]
mod tests;
