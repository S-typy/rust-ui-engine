//! Retained flat grid: a virtual TreeGrid view over a typed, snapshot data source.
use super::*;
use rust_desktop_ui_data::{
    CellValue, DataChange, DataQuery, DataSource, DataTable, SortDescriptor,
};
use rust_desktop_ui_treegrid::{
    ColumnId, GridColumn, GridFrame, GridKey, GridPalette, GridQuery, RowKey, SelectionModifiers,
    SortDirection, SortOrder, TreeDataSource, TreeGrid,
};
use std::{cell::RefCell, rc::Rc};

pub type DataSourceHandle = Rc<RefCell<DataSource>>;

#[derive(Clone, Debug, PartialEq)]
pub struct GridControlColumn {
    pub field: String,
    pub title: String,
    pub width: f32,
    pub min_width: f32,
    pub max_width: f32,
    pub pinned: bool,
    pub read_only: bool,
}

impl GridControlColumn {
    pub fn new(field: impl Into<String>, title: impl Into<String>, width: f32) -> Self {
        Self {
            field: field.into(),
            title: title.into(),
            width,
            min_width: 32.0,
            max_width: 4096.0,
            pinned: false,
            read_only: false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridControlSettings {
    /// Empty generates one column for every field in schema order.
    pub columns: Vec<GridControlColumn>,
    pub is_read_only: bool,
}

struct SnapshotSource {
    handle: DataSourceHandle,
    table: DataTable,
    keys: Vec<i64>,
    positions: HashMap<i64, usize>,
    fields: Vec<String>,
    revision: u64,
    data_query: DataQuery,
}

impl SnapshotSource {
    fn new(handle: DataSourceHandle, fields: Vec<String>) -> Self {
        let data = handle.borrow();
        let keys: Vec<_> = (0..data.len()).filter_map(|i| data.row_key(i)).collect();
        let result = Self {
            table: data.table().clone(),
            positions: keys.iter().enumerate().map(|(i, k)| (*k, i)).collect(),
            keys,
            fields,
            revision: data.revision(),
            data_query: data.query().clone(),
            handle: Rc::clone(&handle),
        };
        drop(data);
        result
    }
    fn reload(&mut self) {
        *self = Self::new(Rc::clone(&self.handle), self.fields.clone());
    }
    fn query(&self) -> GridQuery {
        let data = self.handle.borrow();
        GridQuery {
            filter: data.query().filter.clone(),
            sort: data
                .query()
                .sort
                .iter()
                .filter_map(|sort| {
                    self.fields
                        .iter()
                        .position(|field| field == &sort.field)
                        .map(|index| SortOrder {
                            column: ColumnId(index as u64),
                            direction: match sort.direction {
                                rust_desktop_ui_data::SortDirection::Ascending => {
                                    SortDirection::Ascending
                                }
                                rust_desktop_ui_data::SortDirection::Descending => {
                                    SortDirection::Descending
                                }
                            },
                        })
                })
                .collect(),
        }
    }
}

impl TreeDataSource for SnapshotSource {
    fn child_count(&self, parent: Option<RowKey>) -> Option<usize> {
        Some(if parent.is_none() { self.keys.len() } else { 0 })
    }
    fn child_at(&self, parent: Option<RowKey>, index: usize) -> Option<RowKey> {
        parent
            .is_none()
            .then(|| self.keys.get(index).copied())
            .flatten()
            .map(row_key)
    }
    fn parent(&self, _: RowKey) -> Option<RowKey> {
        None
    }
    fn position(&self, row: RowKey) -> Option<usize> {
        self.positions.get(&data_key(row)).copied()
    }
    fn cell(&self, row: RowKey, column: ColumnId) -> String {
        self.fields
            .get(column.0 as usize)
            .and_then(|field| self.table.value(data_key(row), field))
            .map(CellValue::display)
            .unwrap_or_default()
    }
    fn editable(&self, _: RowKey, column: ColumnId) -> bool {
        self.fields
            .get(column.0 as usize)
            .and_then(|field| self.table.schema().field(field))
            .is_some_and(|field| !field.read_only)
    }
    fn apply_query(&mut self, query: &GridQuery) -> Result<(), String> {
        let sort = query
            .sort
            .iter()
            .map(|sort| {
                Ok(SortDescriptor {
                    field: self
                        .fields
                        .get(sort.column.0 as usize)
                        .ok_or("Unknown data field")?
                        .clone(),
                    direction: match sort.direction {
                        SortDirection::Ascending => rust_desktop_ui_data::SortDirection::Ascending,
                        SortDirection::Descending => {
                            rust_desktop_ui_data::SortDirection::Descending
                        }
                    },
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        self.handle
            .borrow_mut()
            .set_query(DataQuery {
                sort,
                filter: query.filter.clone(),
            })
            .map_err(|e| e.to_string())?;
        self.reload();
        Ok(())
    }
}

struct CellSession {
    row: i64,
    field: String,
    expected: CellValue,
}

pub(super) struct GridState {
    settings: GridControlSettings,
    source: SnapshotSource,
    model: TreeGrid,
    frame: GridFrame,
    editor: WidgetId,
    session: Option<CellSession>,
    error: Option<String>,
    source_error: Option<String>,
    scroll_bars: Vec<GridScrollBar>,
    scroll_drag: Option<GridScrollDrag>,
    selection_validator: Option<SelectionValidator>,
    filter_validator: Option<EditorValidator>,
    reveal_selected: bool,
    bounds: Option<Rect>,
    selected_key: Option<i64>,
}

#[derive(Clone, Copy)]
struct GridScrollBar {
    track: Rect,
    thumb: Rect,
    horizontal: bool,
    maximum: f64,
}
#[derive(Clone, Copy)]
struct GridScrollDrag {
    bar: GridScrollBar,
    start: f32,
    offset: f64,
}

impl GridState {
    /// Preserve the first visible surviving row rather than a numeric index
    /// when collection inserts, removals, moves or field changes reorder rows.
    fn refresh_source(&mut self) -> Result<(), ControlError> {
        let (scroll_x, scroll_y) = self.model.scroll_offset();
        let row_height = f64::from(self.model.config().row_height);
        let old_index = (scroll_y / row_height).floor() as usize;
        let remainder = scroll_y % row_height;
        let anchor = {
            let source = self.source.handle.borrow();
            let query_changed = source
                .changes_since(self.source.revision)
                .changes
                .iter()
                .any(|change| matches!(change, DataChange::Query));
            if !query_changed && self.source.data_query == *source.query() {
                self.source
                    .keys
                    .iter()
                    .skip(old_index)
                    .find_map(|key| source.position(*key))
                    .or_else(|| {
                        self.source
                            .keys
                            .iter()
                            .take(old_index)
                            .rev()
                            .find_map(|key| source.position(*key))
                    })
            } else {
                None
            }
        };
        self.source.reload();
        self.cancel_pointer();
        self.model.refresh(&mut self.source).map_err(error)?;
        self.model.set_query_presentation(self.source.query());
        self.restore_selection()?;
        if let Some(index) = anchor {
            self.model
                .scroll_to(scroll_x, index as f64 * row_height + remainder)
                .map_err(error)?;
            self.reveal_selected = false;
        }
        Ok(())
    }
    fn restore_selection(&mut self) -> Result<(), ControlError> {
        if let Some(key) = self.selected_key {
            if self.source.table.row(key).is_none() {
                self.selected_key = None;
                self.model.clear_selection();
            } else if self.model.focused().is_none() && self.source.position(row_key(key)).is_some()
            {
                self.model
                    .select(&self.source, row_key(key), SelectionModifiers::default())
                    .map_err(error)?;
                self.reveal_selected = true;
            }
        }
        Ok(())
    }
    fn selection_candidate(&self, event: &InputEvent) -> Option<Option<i64>> {
        use rust_desktop_ui_treegrid::GridHitKind;
        let (row, control, shift, navigation) = match event {
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                modifiers,
            } => {
                let GridHitKind::Cell { row, .. } = self.frame.hit_test(*position)? else {
                    return None;
                };
                (row, modifiers.control, modifiers.shift, false)
            }
            InputEvent::KeyDown { key, modifiers, .. } => {
                if modifiers.alt || modifiers.super_key || self.model.row_count() == 0 {
                    return None;
                }
                let current = self
                    .model
                    .focused()
                    .and_then(|row| self.source.position(row));
                let height = self
                    .frame
                    .columns
                    .first()
                    .map_or(0.0, |column| column.bounds.height);
                let page = ((height - self.model.config().header_height).max(0.0)
                    / self.model.config().row_height)
                    .floor()
                    .max(1.0) as usize;
                let next = match key {
                    Key::ArrowUp => current.unwrap_or(1).saturating_sub(1),
                    Key::ArrowDown => current.map_or(0, |i| i.saturating_add(1)),
                    Key::PageUp => current.unwrap_or(0).saturating_sub(page),
                    Key::PageDown => current.unwrap_or(0).saturating_add(page),
                    Key::Home => 0,
                    Key::End => self.model.row_count() - 1,
                    Key::ArrowLeft | Key::ArrowRight => current?,
                    _ => return None,
                }
                .min(self.model.row_count() - 1);
                let row = self.source.child_at(None, next)?;
                (
                    row,
                    modifiers.control,
                    modifiers.shift,
                    !matches!(key, Key::ArrowLeft | Key::ArrowRight),
                )
            }
            _ => return None,
        };
        let selected = if control && !shift {
            if navigation {
                self.model.is_selected(row)
            } else {
                !self.model.is_selected(row)
            }
        } else {
            true
        };
        Some(selected.then(|| data_key(row)))
    }
    fn validate_selection(&self, key: Option<i64>) -> Result<(), ControlError> {
        if let Some(validator) = &self.selection_validator {
            validator
                .check(&key.map(|key| vec![key.to_string()]).unwrap_or_default())
                .map_err(error)?;
        }
        Ok(())
    }
    fn pointer_captured(&self) -> bool {
        self.model.pointer_captured() || self.scroll_drag.is_some()
    }
    fn cancel_pointer(&mut self) {
        self.model.cancel_pointer();
        self.scroll_drag = None;
    }
    fn scroll_geometry(&mut self, bounds: Rect) -> Rect {
        let thickness = 12.0_f32.min(bounds.width).min(bounds.height);
        let total_height =
            self.model.row_count() as f64 * f64::from(self.model.config().row_height);
        let total_width: f64 = self
            .model
            .columns()
            .iter()
            .map(|c| f64::from(c.width))
            .sum();
        let pinned: f64 = self
            .model
            .columns()
            .iter()
            .filter(|c| c.pinned)
            .map(|c| f64::from(c.width))
            .sum();
        let mut vertical =
            total_height > f64::from((bounds.height - self.model.config().header_height).max(0.0));
        let horizontal =
            total_width > f64::from(bounds.width - if vertical { thickness } else { 0.0 });
        vertical |= total_height
            > f64::from(
                (bounds.height
                    - self.model.config().header_height
                    - if horizontal { thickness } else { 0.0 })
                .max(0.0),
            );
        let viewport = Rect::new(
            bounds.x,
            bounds.y,
            (bounds.width - if vertical { thickness } else { 0.0 }).max(0.0),
            (bounds.height - if horizontal { thickness } else { 0.0 }).max(0.0),
        );
        self.scroll_bars.clear();
        let (x, y) = self.model.scroll_offset();
        let mut add = |track: Rect, horizontal: bool, visible: f64, total: f64, offset: f64| {
            let length = if horizontal {
                track.width
            } else {
                track.height
            };
            let thumb_length = (length * (visible / total.max(1.0)) as f32)
                .max(20.0)
                .min(length);
            let maximum = (total - visible).max(0.0);
            let start = if maximum > 0.0 {
                (length - thumb_length) * (offset / maximum).clamp(0.0, 1.0) as f32
            } else {
                0.0
            };
            let thumb = if horizontal {
                Rect::new(
                    track.x + start,
                    track.y + 3.0,
                    thumb_length,
                    (track.height - 6.0).max(0.0),
                )
            } else {
                Rect::new(
                    track.x + 3.0,
                    track.y + start,
                    (track.width - 6.0).max(0.0),
                    thumb_length,
                )
            };
            self.scroll_bars.push(GridScrollBar {
                track,
                thumb,
                horizontal,
                maximum,
            });
        };
        if vertical {
            add(
                Rect::new(
                    viewport.x + viewport.width,
                    viewport.y,
                    thickness,
                    viewport.height,
                ),
                false,
                f64::from((viewport.height - self.model.config().header_height).max(0.0)),
                total_height,
                y,
            );
        }
        if horizontal {
            add(
                Rect::new(
                    viewport.x,
                    viewport.y + viewport.height,
                    viewport.width,
                    thickness,
                ),
                true,
                (f64::from(viewport.width) - pinned).max(0.0),
                (total_width - pinned).max(0.0),
                x,
            );
        }
        viewport
    }
    fn scroll_pointer(
        &mut self,
        event: &InputEvent,
    ) -> Result<bool, rust_desktop_ui_treegrid::GridError> {
        match event {
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                ..
            } => {
                let Some(bar) = self
                    .scroll_bars
                    .iter()
                    .copied()
                    .find(|bar| bar.track.contains(position.x, position.y))
                else {
                    return Ok(false);
                };
                let coord = if bar.horizontal {
                    position.x
                } else {
                    position.y
                };
                let (x, y) = self.model.scroll_offset();
                let mut offset = if bar.horizontal { x } else { y };
                if !bar.thumb.contains(position.x, position.y) {
                    let (start, length, thumb) = if bar.horizontal {
                        (bar.track.x, bar.track.width, bar.thumb.width)
                    } else {
                        (bar.track.y, bar.track.height, bar.thumb.height)
                    };
                    offset = f64::from(
                        ((coord - start - thumb / 2.0) / (length - thumb).max(1.0)).clamp(0.0, 1.0),
                    ) * bar.maximum;
                    self.model.scroll_to(
                        if bar.horizontal { offset } else { x },
                        if bar.horizontal { y } else { offset },
                    )?;
                }
                self.scroll_drag = Some(GridScrollDrag {
                    bar,
                    start: coord,
                    offset,
                });
                Ok(true)
            }
            InputEvent::PointerMoved { position } if self.scroll_drag.is_some() => {
                let drag = self.scroll_drag.unwrap();
                let (coord, travel) = if drag.bar.horizontal {
                    (position.x, drag.bar.track.width - drag.bar.thumb.width)
                } else {
                    (position.y, drag.bar.track.height - drag.bar.thumb.height)
                };
                let offset = drag.offset
                    + f64::from((coord - drag.start) / travel.max(1.0)) * drag.bar.maximum;
                let (x, y) = self.model.scroll_offset();
                self.model.scroll_to(
                    if drag.bar.horizontal { offset } else { x },
                    if drag.bar.horizontal { y } else { offset },
                )?;
                Ok(true)
            }
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                ..
            } if self.scroll_drag.is_some() => {
                self.scroll_drag = None;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

fn row_key(key: i64) -> RowKey {
    RowKey(u64::from_ne_bytes(key.to_ne_bytes()))
}
fn data_key(key: RowKey) -> i64 {
    i64::from_ne_bytes(key.0.to_ne_bytes())
}
fn error(error: impl fmt::Display) -> ControlError {
    ControlError::Invalid(error.to_string())
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .enumerate()
            .all(|(i, c)| c == b'_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()))
}

impl Controls {
    pub fn register_data_source(
        &mut self,
        name: impl Into<String>,
        source: DataSourceHandle,
    ) -> Result<(), ControlError> {
        let name = name.into();
        if !valid_name(&name) || self.data_sources.contains_key(&name) {
            return Err(error("Data source name must be nonempty and unique"));
        }
        self.data_sources.insert(name, source);
        Ok(())
    }
    pub fn data_source(&self, name: &str) -> Option<DataSourceHandle> {
        self.data_sources.get(name).cloned()
    }

    pub fn grid_control(
        &mut self,
        parent: WidgetId,
        source: DataSourceHandle,
        mut settings: GridControlSettings,
        mut style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        let _ = source.borrow_mut().synchronize();
        {
            let data = source.borrow();
            if settings.columns.is_empty() {
                settings.columns = data
                    .table()
                    .schema()
                    .fields()
                    .iter()
                    .map(|field| GridControlColumn {
                        read_only: field.read_only,
                        ..GridControlColumn::new(&field.name, &field.title, 160.0)
                    })
                    .collect();
            }
            if settings.columns.len() > 64 {
                return Err(error("GridControl supports at most 64 columns"));
            }
            let mut seen = HashSet::new();
            for column in &settings.columns {
                if !seen.insert(&column.field)
                    || data.table().schema().field(&column.field).is_none()
                {
                    return Err(error("Grid columns require unique, known data fields"));
                }
                if column.field.is_empty()
                    || column.field.len() > 256
                    || column.field.contains('\0')
                    || column.title.len() > 4096
                    || column.min_width < 32.0
                    || column.max_width > 4096.0
                {
                    return Err(error(
                        "Invalid GridControl column identifier, caption or width limits",
                    ));
                }
            }
        }
        let columns = settings
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| GridColumn {
                id: ColumnId(i as u64),
                title: c.title.clone(),
                width: c.width,
                min_width: c.min_width,
                max_width: c.max_width,
                pinned: c.pinned,
                editable: !settings.is_read_only && !c.read_only,
            })
            .collect();
        let mut model = TreeGrid::new(columns).map_err(error)?;
        model.flat_presentation = true;
        let mut snapshot = SnapshotSource::new(
            source,
            settings.columns.iter().map(|c| c.field.clone()).collect(),
        );
        model.refresh(&mut snapshot).map_err(error)?;
        // Synchronize accepted query indicators without calling the adapter.
        let query = snapshot.query();
        model.set_query_presentation(query);
        style.kind = LayoutKind::Overlay;
        if style.height == Length::Auto && style.flex_grow == 0.0 {
            style.height = Length::Px(320.0);
        }
        let id = self.panel(parent, style)?;
        self.controls.get_mut(&id).unwrap().role = Some(Role::Grid);
        self.edit_props(id, |props| props.focusable = true)?;
        let editor = self.edit_box(
            id,
            "Cell value",
            "",
            LayoutStyle {
                width: Length::Px(100.0),
                height: Length::Px(28.0),
                ..Default::default()
            },
        )?;
        self.edit_props(editor, |props| {
            props.visible = false;
            props.collapsed = true;
        })?;
        self.grids.insert(
            id,
            GridState {
                settings,
                source: snapshot,
                model,
                frame: GridFrame::default(),
                editor,
                session: None,
                error: None,
                source_error: None,
                scroll_bars: Vec::new(),
                scroll_drag: None,
                selection_validator: None,
                filter_validator: None,
                reveal_selected: false,
                bounds: None,
                selected_key: None,
            },
        );
        Ok(id)
    }

    pub fn grid_frame(&self, id: WidgetId) -> Option<&GridFrame> {
        self.grids.get(&id).map(|g| &g.frame)
    }
    pub fn grid_source(&self, id: WidgetId) -> Option<DataSourceHandle> {
        self.grids.get(&id).map(|g| Rc::clone(&g.source.handle))
    }
    pub fn grid_settings(&self, id: WidgetId) -> Option<&GridControlSettings> {
        self.grids.get(&id).map(|g| &g.settings)
    }
    pub fn grid_editor(&self, id: WidgetId) -> Option<WidgetId> {
        self.grids
            .get(&id)
            .and_then(|g| g.session.as_ref().map(|_| g.editor))
    }
    pub fn grid_focused_column(&self, id: WidgetId) -> Option<ColumnId> {
        self.grids.get(&id).and_then(|g| g.model.focused_column())
    }
    pub fn grid_error(&self, id: WidgetId) -> Option<&str> {
        self.grids
            .get(&id)
            .and_then(|g| g.error.as_deref().or(g.source_error.as_deref()))
    }
    pub fn grid_selected_key(&self, id: WidgetId) -> Option<i64> {
        self.grids.get(&id).and_then(|g| g.selected_key)
    }
    pub fn grid_filter(&self, id: WidgetId) -> Option<&str> {
        self.grids.get(&id).map(|g| g.model.query().filter.as_str())
    }
    pub(crate) fn chain_grid_selection_validator(
        &mut self,
        id: WidgetId,
        validator: SelectionValidator,
    ) -> Result<(), ControlError> {
        let grid = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        let previous = grid.selection_validator.take();
        grid.selection_validator = Some(SelectionValidator::new(move |keys| {
            if let Some(previous) = &previous {
                previous.check(keys)?;
            }
            validator.check(keys)
        }));
        Ok(())
    }
    pub(crate) fn chain_grid_filter_validator(
        &mut self,
        id: WidgetId,
        validator: EditorValidator,
    ) -> Result<(), ControlError> {
        let grid = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        let previous = grid.filter_validator.take();
        grid.filter_validator = Some(EditorValidator::new(move |text| {
            if let Some(previous) = &previous {
                previous.check(text)?;
            }
            validator.check(text)
        }));
        Ok(())
    }
    pub fn set_grid_selected_key(
        &mut self,
        id: WidgetId,
        key: Option<i64>,
    ) -> Result<(), ControlError> {
        if self.grids.contains_key(&id) && self.grid_selected_key(id) == key {
            return Ok(());
        }
        if self.grid_editor(id).is_some() {
            return Err(error(
                "Finish the active cell edit before changing selection",
            ));
        }
        self.sync_grids()?;
        let grid = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        grid.validate_selection(key)?;
        if let Some(key) = key {
            if grid.source.table.row(key).is_none() {
                return Err(error("Unknown grid row"));
            }
            if grid.source.position(row_key(key)).is_some() {
                grid.model
                    .select(&grid.source, row_key(key), SelectionModifiers::default())
                    .map_err(error)?;
                grid.reveal_selected = true;
            } else {
                grid.model.clear_selection();
            }
        } else {
            grid.model.clear_selection();
        }
        grid.selected_key = key;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn set_grid_filter(&mut self, id: WidgetId, filter: &str) -> Result<(), ControlError> {
        if filter.len() > 4096 {
            return Err(error("Grid FilterText exceeds 4096 bytes"));
        }
        if self.grid_editor(id).is_some() {
            return Err(error("Finish the active cell edit before filtering"));
        }
        self.sync_grids()?;
        let grid = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        let mut query = grid.source.handle.borrow().query().clone();
        if let Some(validator) = &grid.filter_validator {
            validator.check(filter).map_err(error)?;
        }
        query.filter = filter.to_owned();
        let result = grid.source.handle.borrow_mut().set_query(query);
        grid.error = result.as_ref().err().map(ToString::to_string);
        if result.is_ok() {
            grid.source.reload();
            grid.model.refresh(&mut grid.source).map_err(error)?;
            grid.model.set_query_presentation(grid.source.query());
            grid.restore_selection()?;
        }
        self.scene_dirty = true;
        result.map(|_| ()).map_err(error)
    }
    pub fn set_grid_read_only(&mut self, id: WidgetId, value: bool) -> Result<(), ControlError> {
        let grid = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        grid.settings.is_read_only = value;
        for (index, column) in grid.settings.columns.iter().enumerate() {
            grid.model
                .set_column_editable(ColumnId(index as u64), !value && !column.read_only)
                .map_err(error)?;
        }
        let editor = grid.editor;
        if grid.session.is_some() {
            self.set_read_only(editor, value)?;
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub fn grid_scroll_to(&mut self, id: WidgetId, x: f64, y: f64) -> Result<(), ControlError> {
        let grid = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        if grid.session.is_some() {
            return Err(error("Finish the active cell edit before scrolling"));
        }
        grid.model.scroll_to(x, y).map_err(error)?;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn grid_scroll_by(&mut self, id: WidgetId, dx: f64, dy: f64) -> Result<(), ControlError> {
        if self.grid_editor(id).is_some() && !self.commit_grid_edit(id)? {
            return Err(error("The active grid edit is invalid"));
        }
        let grid = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        grid.model.scroll_by(dx, dy).map_err(error)?;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn grid_focus_cell(
        &mut self,
        id: WidgetId,
        key: i64,
        field: Option<&str>,
    ) -> Result<(), ControlError> {
        if self.grid_editor(id).is_some() && !self.commit_grid_edit(id)? {
            return Err(error("The active grid edit is invalid"));
        }
        self.sync_grids()?;
        let grid = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        let column = if let Some(field) = field {
            ColumnId(
                grid.source
                    .fields
                    .iter()
                    .position(|candidate| candidate == field)
                    .ok_or_else(|| error("Unknown grid field"))? as u64,
            )
        } else {
            grid.model.focused_column().unwrap_or(ColumnId(0))
        };
        grid.validate_selection(Some(key))?;
        grid.model
            .focus_cell(
                &grid.source,
                row_key(key),
                column,
                SelectionModifiers::default(),
            )
            .map_err(error)?;
        grid.selected_key = Some(key);
        self.runtime.apply(Mutation::Focus(Some(id)))?;
        self.synchronize_editor_focus()?;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn pin_grid_column(
        &mut self,
        id: WidgetId,
        field: &str,
        pinned: bool,
    ) -> Result<(), ControlError> {
        let grid = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        let index = grid
            .settings
            .columns
            .iter()
            .position(|c| c.field == field)
            .ok_or_else(|| error("Unknown grid field"))?;
        grid.model
            .pin_column(ColumnId(index as u64), pinned)
            .map_err(error)?;
        grid.settings.columns[index].pinned = pinned;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn begin_grid_edit(
        &mut self,
        id: WidgetId,
        key: i64,
        field: &str,
    ) -> Result<(), ControlError> {
        if self.grid_editor(id).is_some() {
            return Err(error("A cell edit is already active"));
        }
        self.sync_grids()?;
        let grid = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        let index = grid
            .settings
            .columns
            .iter()
            .position(|c| c.field == field)
            .ok_or_else(|| error("Unknown grid field"))?;
        if grid.settings.is_read_only || grid.settings.columns[index].read_only {
            return Err(error("Grid cell is read-only"));
        }
        let expected = grid
            .source
            .table
            .value(key, field)
            .cloned()
            .ok_or_else(|| error("Unknown row or field"))?;
        if expected.display().chars().any(single_line_separator) {
            return Err(error(
                "The cell contains multiline text; use the typed data source to edit it losslessly",
            ));
        }
        grid.validate_selection(Some(key))?;
        grid.model
            .focus_cell(
                &grid.source,
                row_key(key),
                ColumnId(index as u64),
                SelectionModifiers::default(),
            )
            .map_err(error)?;
        grid.selected_key = Some(key);
        grid.model
            .begin_edit(&grid.source, row_key(key), ColumnId(index as u64))
            .map_err(error)?;
        let editor = grid.editor;
        grid.session = Some(CellSession {
            row: key,
            field: field.to_owned(),
            expected: expected.clone(),
        });
        grid.error = None;
        self.set_editor_value(editor, &expected.display())?;
        self.set_read_only(editor, false)?;
        self.edit_props(editor, |props| {
            props.visible = true;
            props.collapsed = false;
        })?;
        self.sync_grids()?;
        self.runtime.apply(Mutation::Focus(Some(editor)))?;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn commit_grid_edit(&mut self, id: WidgetId) -> Result<bool, ControlError> {
        Ok(matches!(
            self.commit_grid_editor(id)?,
            CommitOutcome::Committed | CommitOutcome::Unchanged
        ))
    }
    pub(super) fn commit_grid_editor(
        &mut self,
        id: WidgetId,
    ) -> Result<CommitOutcome, ControlError> {
        let grid = self
            .grids
            .get(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        let editor = grid.editor;
        let session = grid
            .session
            .as_ref()
            .ok_or_else(|| error("No active grid cell edit"))?;
        let document = self.document(editor).unwrap();
        if document.composition().is_some() {
            return Ok(CommitOutcome::CompositionActive);
        }
        let text = document.text().to_owned();
        let result = if grid.settings.is_read_only {
            Err("Grid cell is read-only".to_owned())
        } else {
            grid.source
                .table
                .schema()
                .field(&session.field)
                .ok_or_else(|| "Unknown field".to_owned())
                .and_then(|field| {
                    if text == session.expected.display() {
                        Ok(session.expected.clone())
                    } else {
                        field.parse(&text).map_err(|e| e.to_string())
                    }
                })
                .and_then(|value| {
                    grid.source
                        .handle
                        .borrow_mut()
                        .set_cell_expected(session.row, &session.field, &session.expected, value)
                        .map_err(|e| e.to_string())
                })
        };
        match result {
            Ok(changed) => {
                self.controls
                    .get_mut(&editor)
                    .unwrap()
                    .accepted_value
                    .clone_from(&text);
                self.controls.get_mut(&editor).unwrap().validation_error = None;
                if changed {
                    self.edit_events.push(EditEvent::Committed {
                        id: editor,
                        value: text,
                    });
                }
                self.finish_grid_edit(id)?;
                Ok(if changed {
                    CommitOutcome::Committed
                } else {
                    CommitOutcome::Unchanged
                })
            }
            Err(message) => {
                self.grids.get_mut(&id).unwrap().error = Some(message.clone());
                self.controls.get_mut(&editor).unwrap().validation_error = Some(message.clone());
                self.edit_events.push(EditEvent::ValidationFailed {
                    id: editor,
                    message: message.clone(),
                });
                self.scene_dirty = true;
                Ok(CommitOutcome::Invalid(message))
            }
        }
    }
    pub fn cancel_grid_edit(&mut self, id: WidgetId) -> Result<(), ControlError> {
        if self.grid_editor(id).is_none() {
            return Ok(());
        }
        let editor = self.grids[&id].editor;
        self.cancel_edit(editor)?;
        self.finish_grid_edit(id)
    }
    fn finish_grid_edit(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let grid = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown GridControl"))?;
        grid.session = None;
        grid.model.cancel_edit();
        grid.error = None;
        let editor = grid.editor;
        if self.runtime.focused() == Some(editor) {
            self.runtime.apply(Mutation::Focus(Some(id)))?;
        }
        self.edit_props(editor, |props| {
            props.visible = false;
            props.collapsed = true;
        })?;
        self.scene_dirty = true;
        Ok(())
    }
    pub(super) fn grid_owner_of_editor(&self, editor: WidgetId) -> Option<WidgetId> {
        self.grids
            .iter()
            .find_map(|(&id, g)| (g.editor == editor && g.session.is_some()).then_some(id))
    }
    pub(super) fn preflight_grid_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.grids.values().any(|g| g.editor == id) {
            return Err(error("Remove the GridControl, not its cell editor"));
        }
        Ok(())
    }
    pub(super) fn cancel_grid_pointers(&mut self) {
        for grid in self.grids.values_mut() {
            grid.cancel_pointer();
        }
    }

    pub(super) fn sync_grids(&mut self) -> Result<(), ControlError> {
        let ids: Vec<_> = self.grids.keys().copied().collect();
        let theme = self.theme.tokens();
        let chrome = self.theme.chrome();
        let mut synchronized = HashSet::new();
        for id in ids {
            let source = &self.grids[&id].source.handle;
            if synchronized.insert(Rc::as_ptr(source)) {
                // Only adapters with notifications participate. SQL and default
                // custom adapters keep explicit refresh; idle polling never loads data.
                // The source retains a failed synchronization diagnostic and
                // its previous accepted snapshot for every observing grid.
                let _ = source.borrow_mut().synchronize();
            }
            let available = self.is_enabled(id);
            let bounds = self.runtime.bounds(id);
            let grid = self.grids.get_mut(&id).unwrap();
            let was_captured = grid.pointer_captured();
            if grid.bounds.is_some_and(|previous| Some(previous) != bounds) {
                grid.cancel_pointer();
            }
            grid.bounds = bounds;
            if !available {
                grid.cancel_pointer();
            }
            if grid.session.is_none()
                && grid.source.revision != grid.source.handle.borrow().revision()
            {
                grid.refresh_source()?;
            }
            if was_captured && !grid.pointer_captured() && self.runtime.captured() == Some(id) {
                self.runtime.apply(Mutation::CapturePointer(None))?;
            }
            let Some(bounds) = bounds else {
                continue;
            };
            grid.model.palette = GridPalette {
                background: chrome.card_surface,
                alternate_row: theme.canvas,
                header: chrome.control_hover,
                text: theme.text,
                muted: theme.muted,
                line: chrome.divider,
                selection: theme.selected,
                selection_text: theme.text,
                focus: theme.focus,
                error: theme.error,
            };
            if let Some(session) = &grid.session {
                let document = match &self.controls[&grid.editor].kind {
                    Kind::TextBox(doc) => doc,
                    _ => unreachable!(),
                };
                grid.model.set_edit_text(document.text()).map_err(error)?;
                let _ = session;
            }
            let view_bounds = grid.scroll_geometry(bounds);
            grid.model
                .set_viewport(Size::new(view_bounds.width, view_bounds.height))
                .map_err(error)?;
            if grid.reveal_selected {
                if let (Some(row), Some(column)) =
                    (grid.model.focused(), grid.model.focused_column())
                {
                    grid.model
                        .focus_cell(&grid.source, row, column, SelectionModifiers::default())
                        .map_err(error)?;
                }
                grid.reveal_selected = false;
            }
            // Thumb positions must use the offset after selection reveal/clamping.
            grid.scroll_geometry(bounds);
            grid.frame = grid.model.paint(&grid.source, view_bounds).map_err(error)?;
            for row in &mut grid.frame.semantics.rows {
                for cell in &mut row.cells {
                    if cell.editing {
                        cell.validation_error =
                            self.controls[&grid.editor].validation_error.clone();
                    }
                }
            }
            let mut scene = grid.frame.scene.clone();
            for bar in &grid.scroll_bars {
                scene.fill(bar.track, chrome.card_surface);
                scene.fill(bar.thumb, theme.muted);
            }
            grid.source_error = grid
                .source
                .handle
                .borrow()
                .last_error()
                .map(ToString::to_string);
            let message = grid
                .error
                .clone()
                .or(grid.source_error.clone())
                .or_else(|| (grid.model.row_count() == 0).then(|| "Нет данных".to_owned()));
            if let Some(message) = message {
                let height =
                    28.0_f32.min((bounds.height - grid.model.config().header_height).max(0.0));
                let band = Rect::new(
                    bounds.x,
                    bounds.y + bounds.height - height,
                    bounds.width,
                    height,
                );
                scene.fill(band, chrome.card_surface);
                scene.text(TextRun {
                    bounds: Rect::new(
                        band.x + 8.0,
                        band.y + 3.0,
                        (band.width - 16.0).max(0.0),
                        (band.height - 6.0).max(0.0),
                    ),
                    clip: band,
                    text: message,
                    font_size: theme.font_size,
                    color: if grid.model.row_count() == 0 && grid.error.is_none() {
                        theme.muted
                    } else {
                        theme.error
                    },
                    weight: 400,
                    align: TextAlign::Start,
                    wrap: false,
                });
            }
            let editor = grid.editor;
            let placement = grid
                .model
                .editor()
                .and_then(|e| grid.frame.cell_bounds(e.row, e.column));
            if let Some(placement) = placement {
                self.edit_props(editor, |props| {
                    props.style.offset = Point::new(placement.x - bounds.x, placement.y - bounds.y);
                    props.style.width = Length::Px(placement.width);
                    props.style.height = Length::Px(placement.height);
                })?;
            }
            self.set_custom_scene(id, scene)?;
        }
        Ok(())
    }

    pub(super) fn grid_before_input(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        if self.popup.is_some() || self.rich.popup.is_some() {
            return Ok(false);
        }
        if matches!(event, InputEvent::WindowFocused(false)) {
            self.cancel_grid_pointers();
            return Ok(false);
        }
        if let Some(owner) = self
            .runtime
            .focused()
            .and_then(|editor| self.grid_owner_of_editor(editor))
            && let InputEvent::KeyDown {
                key,
                repeat: false,
                modifiers,
            } = event
            && !modifiers.alt
            && !modifiers.control
            && !modifiers.super_key
        {
            match key {
                Key::Enter => {
                    self.commit_grid_edit(owner)?;
                    return Ok(true);
                }
                Key::Escape
                    if self
                        .document(self.grids[&owner].editor)
                        .is_some_and(|d| d.composition().is_none()) =>
                {
                    self.cancel_grid_edit(owner)?;
                    return Ok(true);
                }
                _ => {}
            }
        }
        let hit = match event {
            InputEvent::PointerDown { position, .. }
            | InputEvent::PointerUp { position, .. }
            | InputEvent::PointerMoved { position }
            | InputEvent::Wheel { position, .. } => self.runtime.hit_test(*position),
            _ => self.runtime.focused(),
        };
        let captured = self
            .grids
            .iter()
            .find_map(|(&id, g)| g.pointer_captured().then_some(id));
        let id = captured.or_else(|| hit.filter(|id| self.grids.contains_key(id)));
        let Some(id) = id.filter(|id| self.is_enabled(*id)) else {
            return Ok(false);
        };
        if self.grids[&id].session.is_some() {
            if matches!(
                event,
                InputEvent::PointerDown { .. } | InputEvent::Wheel { .. }
            ) && !self.commit_grid_edit(id)?
            {
                return Ok(true);
            }
            if self.grids[&id].session.is_some() {
                return Ok(false);
            }
        }
        let matches_grid_event = matches!(
            event,
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                ..
            } | InputEvent::Wheel { .. }
        ) || (captured.is_some()
            && matches!(
                event,
                InputEvent::PointerMoved { .. }
                    | InputEvent::PointerUp {
                        button: PointerButton::Primary,
                        ..
                    }
            ))
            || matches!(
                event,
                InputEvent::KeyDown {
                    key: Key::F2
                        | Key::Enter
                        | Key::ArrowLeft
                        | Key::ArrowRight
                        | Key::ArrowUp
                        | Key::ArrowDown
                        | Key::PageUp
                        | Key::PageDown
                        | Key::Home
                        | Key::End,
                    ..
                }
            );
        if !matches_grid_event {
            return Ok(false);
        }
        let report = self.runtime.dispatch(event.clone());
        if let Some(error) = report.errors.into_iter().next() {
            return Err(error.into());
        }
        if report.default_prevented {
            return Ok(true);
        }
        self.synchronize_editor_focus()?;
        let selection_candidate = self.grids[&id].selection_candidate(event);
        if let Some(candidate) = selection_candidate
            && let Err(message) = self.grids[&id].validate_selection(candidate)
        {
            self.grids.get_mut(&id).unwrap().error = Some(message.to_string());
            self.scene_dirty = true;
            return Ok(true);
        }
        let mut handled = true;
        let mut begin = None;
        let result = {
            let grid = self.grids.get_mut(&id).unwrap();
            if grid.scroll_pointer(event).map_err(error)? {
                Ok(())
            } else {
                match event {
                    InputEvent::PointerDown {
                        position,
                        button: PointerButton::Primary,
                        modifiers,
                    } => grid
                        .model
                        .pointer_down(
                            &mut grid.source,
                            &grid.frame,
                            *position,
                            SelectionModifiers {
                                shift: modifiers.shift,
                                control: modifiers.control,
                            },
                        )
                        .map(|_| ()),
                    InputEvent::PointerMoved { position } if captured.is_some() => {
                        grid.model.pointer_move(*position).map(|_| ())
                    }
                    InputEvent::PointerUp {
                        position,
                        button: PointerButton::Primary,
                        ..
                    } if captured.is_some() => grid
                        .model
                        .pointer_up(&mut grid.source, &grid.frame, *position)
                        .map(|_| ()),
                    InputEvent::Wheel {
                        delta, modifiers, ..
                    } => {
                        let (dx, dy) = if modifiers.shift {
                            (delta.y + delta.x, 0.0)
                        } else {
                            (delta.x, delta.y)
                        };
                        grid.model.scroll_by(-f64::from(dx), -f64::from(dy))
                    }
                    InputEvent::KeyDown {
                        key,
                        modifiers,
                        repeat,
                    } if !modifiers.alt && !modifiers.super_key => {
                        let selected = SelectionModifiers {
                            shift: modifiers.shift,
                            control: modifiers.control,
                        };
                        match key {
                            Key::F2 | Key::Enter if !repeat => {
                                if let (Some(row), Some(col)) =
                                    (grid.model.focused(), grid.model.focused_column())
                                {
                                    begin = Some((
                                        data_key(row),
                                        grid.source.fields[col.0 as usize].clone(),
                                    ));
                                }
                                Ok(())
                            }
                            Key::ArrowLeft | Key::ArrowRight => {
                                if let Some(row) = grid.model.focused() {
                                    let columns = grid.model.columns();
                                    let index = columns
                                        .iter()
                                        .position(|c| Some(c.id) == grid.model.focused_column())
                                        .unwrap_or(0);
                                    let next = if *key == Key::ArrowRight {
                                        (index + 1).min(columns.len() - 1)
                                    } else {
                                        index.saturating_sub(1)
                                    };
                                    grid.model.focus_cell(
                                        &grid.source,
                                        row,
                                        columns[next].id,
                                        selected,
                                    )
                                } else {
                                    Ok(())
                                }
                            }
                            _ => {
                                let key = match key {
                                    Key::ArrowUp => Some(GridKey::Up),
                                    Key::ArrowDown => Some(GridKey::Down),
                                    Key::PageUp => Some(GridKey::PageUp),
                                    Key::PageDown => Some(GridKey::PageDown),
                                    Key::Home => Some(GridKey::Home),
                                    Key::End => Some(GridKey::End),
                                    _ => None,
                                };
                                if let Some(key) = key {
                                    grid.model.navigate(&mut grid.source, key, selected)
                                } else {
                                    handled = false;
                                    Ok(())
                                }
                            }
                        }
                    }
                    _ => {
                        handled = false;
                        Ok(())
                    }
                }
            }
        };
        if !handled {
            return Ok(true);
        }
        if result.is_ok() {
            if let Some(candidate) = selection_candidate {
                self.grids.get_mut(&id).unwrap().selected_key = candidate;
            }
            self.grids.get_mut(&id).unwrap().restore_selection()?;
        }
        if let Err(message) = result {
            self.grids.get_mut(&id).unwrap().error = Some(message.to_string());
        }
        if self.grids[&id].pointer_captured() {
            self.runtime.apply(Mutation::CapturePointer(Some(id)))?;
        } else if captured.is_some() {
            self.runtime.apply(Mutation::CapturePointer(None))?;
        }
        if let Some((key, field)) = begin
            && let Err(message) = self.begin_grid_edit(id, key, &field)
        {
            self.grids.get_mut(&id).unwrap().error = Some(message.to_string());
        }
        self.scene_dirty = true;
        Ok(true)
    }
}
