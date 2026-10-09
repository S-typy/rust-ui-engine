use std::{
    collections::{HashMap, HashSet},
    sync::atomic::{AtomicU64, Ordering},
};

use rust_desktop_ui_core::{Point, Rect, Size};

use crate::index::{self, Locations, SparseIndex};
use crate::{
    ChildrenLoad, ColumnId, GridError, GridFrame, GridHitKind, GridPalette, GridQuery, RequestId,
    RowKey, SortDirection, SortOrder, TreeDataSource, TreeGridView,
};

static NEXT_MODEL: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq)]
pub struct GridColumn {
    pub id: ColumnId,
    pub title: String,
    pub width: f32,
    pub min_width: f32,
    pub max_width: f32,
    pub pinned: bool,
    pub editable: bool,
}

impl GridColumn {
    pub fn new(id: ColumnId, title: impl Into<String>, width: f32) -> Self {
        Self {
            id,
            title: title.into(),
            width,
            min_width: 32.0,
            max_width: 4096.0,
            pinned: false,
            editable: false,
        }
    }

    fn validate(&self) -> Result<(), GridError> {
        if !self.width.is_finite()
            || !self.min_width.is_finite()
            || !self.max_width.is_finite()
            || self.min_width <= 0.0
            || self.max_width < self.min_width
            || self.width < self.min_width
            || self.width > self.max_width
        {
            return Err(GridError::InvalidColumn(
                "width must be finite and within positive min/max bounds",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridConfig {
    pub row_height: f32,
    pub header_height: f32,
    pub font_size: f32,
    pub overscan: usize,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self {
            row_height: 28.0,
            header_height: 32.0,
            font_size: 14.0,
            overscan: 2,
        }
    }
}

impl GridConfig {
    fn validate(self) -> Result<(), GridError> {
        if !self.row_height.is_finite()
            || self.row_height <= 0.0
            || !self.header_height.is_finite()
            || self.header_height < 0.0
            || !self.font_size.is_finite()
            || self.font_size <= 0.0
            || self.overscan > 1024
        {
            return Err(GridError::InvalidGeometry(
                "invalid row/header/font height or overscan",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SelectionModifiers {
    pub shift: bool,
    pub control: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridKey {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Left,
    Right,
    Enter,
    F2,
    Escape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellEdit {
    pub row: RowKey,
    pub column: ColumnId,
    pub original: String,
    pub value: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleRow {
    pub index: usize,
    pub key: Option<RowKey>,
    pub parent: Option<RowKey>,
    pub depth: usize,
    pub loading: bool,
    pub error: Option<String>,
}

enum ColumnDrag {
    Resize {
        id: ColumnId,
        start_x: f32,
        width: f32,
    },
    Header {
        id: ColumnId,
        start: Point,
        moved: bool,
    },
}

/// Retained state without per-logical-row widgets or render objects.
///
/// The sparse index stores one segment per expanded boundary, not every row.
/// Hierarchy mutations may rebuild these segments; viewport access uses binary
/// search and direct indexed source access. Explicit range selection can visit
/// the selected range once, preserving stable keys across sorting/collapse.
pub struct TreeGrid {
    pub palette: GridPalette,
    pub(crate) columns: Vec<GridColumn>,
    pub(crate) config: GridConfig,
    pub(crate) regular: Vec<usize>,
    pub(crate) pinned: Vec<usize>,
    pub(crate) column_ends: Vec<f64>,
    pub(crate) pinned_width: f64,
    pub(crate) scroll_x: f64,
    pub(crate) scroll_y: f64,
    pub(crate) viewport: Size,
    pub(crate) expanded: HashSet<RowKey>,
    pub(crate) selected: HashSet<RowKey>,
    pub(crate) focused: Option<RowKey>,
    pub(crate) focused_column: Option<ColumnId>,
    pub(crate) editor: Option<CellEdit>,
    query: GridQuery,
    tree_column: ColumnId,
    drag: Option<ColumnDrag>,
    anchor: Option<RowKey>,
    model_id: u64,
    epoch: u64,
    sequence: u64,
    index: SparseIndex,
    loaded: HashMap<RowKey, Vec<RowKey>>,
    locations: Locations,
    pending: HashMap<RowKey, RequestId>,
    failures: HashMap<RowKey, String>,
}

impl TreeGrid {
    pub fn new(columns: Vec<GridColumn>) -> Result<Self, GridError> {
        Self::with_config(columns, GridConfig::default())
    }

    pub fn with_config(columns: Vec<GridColumn>, config: GridConfig) -> Result<Self, GridError> {
        config.validate()?;
        validate_columns(&columns)?;
        let model_id = NEXT_MODEL
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| GridError::Capacity)?;
        let tree_column = columns[0].id;
        let focused_column = Some(tree_column);
        let mut grid = Self {
            palette: GridPalette::default(),
            columns,
            config,
            regular: Vec::new(),
            pinned: Vec::new(),
            column_ends: Vec::new(),
            pinned_width: 0.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            viewport: Size::ZERO,
            expanded: HashSet::new(),
            selected: HashSet::new(),
            focused: None,
            focused_column,
            editor: None,
            query: GridQuery::default(),
            tree_column,
            drag: None,
            anchor: None,
            model_id,
            epoch: 0,
            sequence: 0,
            index: SparseIndex::default(),
            loaded: HashMap::new(),
            locations: HashMap::new(),
            pending: HashMap::new(),
            failures: HashMap::new(),
        };
        grid.index_columns();
        Ok(grid)
    }

    pub fn columns(&self) -> &[GridColumn] {
        &self.columns
    }
    pub fn config(&self) -> GridConfig {
        self.config
    }
    pub fn row_count(&self) -> usize {
        self.index.count
    }
    pub fn index_segment_count(&self) -> usize {
        self.index.spans.len()
    }
    pub fn focused(&self) -> Option<RowKey> {
        self.focused
    }
    pub fn focused_column(&self) -> Option<ColumnId> {
        self.focused_column
    }
    pub fn selected(&self) -> &HashSet<RowKey> {
        &self.selected
    }
    pub fn is_selected(&self, row: RowKey) -> bool {
        self.selected.contains(&row)
    }
    pub fn is_expanded(&self, row: RowKey) -> bool {
        self.expanded.contains(&row)
    }
    pub fn editor(&self) -> Option<&CellEdit> {
        self.editor.as_ref()
    }
    pub fn query(&self) -> &GridQuery {
        &self.query
    }
    pub fn tree_column(&self) -> ColumnId {
        self.tree_column
    }
    pub fn is_loading(&self, row: RowKey) -> bool {
        self.pending.contains_key(&row)
    }
    pub fn pointer_captured(&self) -> bool {
        self.drag.is_some()
    }
    pub fn scroll_offset(&self) -> (f64, f64) {
        (self.scroll_x, self.scroll_y)
    }
    pub fn pending_requests(&self) -> Vec<RequestId> {
        self.pending.values().copied().collect()
    }

    /// Call before detaching a grid from a long-lived data source. Collapse and
    /// refresh also cancel their affected requests. Late responses are rejected.
    pub fn cancel_pending_requests(&mut self, source: &mut impl TreeDataSource) {
        for request in self.pending.values().copied() {
            source.cancel_request(request);
        }
        self.pending.clear();
    }

    /// Refresh after the source's indexed data changes. Invalidates pending
    /// requests and cached lazy responses, while preserving stable selected keys.
    pub fn refresh(&mut self, source: &mut impl TreeDataSource) -> Result<(), GridError> {
        let next = self.epoch.checked_add(1).ok_or(GridError::Capacity)?;
        let index = SparseIndex::rebuild(source, &self.expanded, &HashMap::new(), &HashMap::new())?;
        for request in self.pending.values().copied() {
            source.cancel_request(request);
        }
        self.pending.clear();
        self.loaded.clear();
        self.locations.clear();
        self.failures.clear();
        self.epoch = next;
        self.index = index;
        self.editor = None;
        if self
            .focused
            .is_some_and(|row| self.visible_index(source, row).is_none())
        {
            self.focused = None;
        }
        self.clamp_scroll();
        Ok(())
    }

    pub fn set_viewport(&mut self, viewport: Size) -> Result<(), GridError> {
        if !viewport.is_valid() {
            return Err(GridError::InvalidGeometry(
                "viewport must be finite and nonnegative",
            ));
        }
        self.viewport = viewport;
        self.clamp_scroll();
        Ok(())
    }

    pub fn scroll_to(&mut self, x: f64, y: f64) -> Result<(), GridError> {
        if !x.is_finite() || !y.is_finite() {
            return Err(GridError::InvalidGeometry("scroll offsets must be finite"));
        }
        self.scroll_x = x.max(0.0);
        self.scroll_y = y.max(0.0);
        self.clamp_scroll();
        Ok(())
    }

    pub fn scroll_by(&mut self, dx: f64, dy: f64) -> Result<(), GridError> {
        self.scroll_to(self.scroll_x + dx, self.scroll_y + dy)
    }

    pub fn paint(
        &mut self,
        source: &impl TreeDataSource,
        bounds: Rect,
    ) -> Result<GridFrame, GridError> {
        if !bounds.is_valid() {
            return Err(GridError::InvalidGeometry(
                "grid bounds must be finite and nonnegative",
            ));
        }
        self.set_viewport(Size::new(bounds.width, bounds.height))?;
        TreeGridView::paint(self, source, bounds)
    }

    /// Pointer interaction against the most recently painted frame. The host
    /// continues forwarding move/up while `pointer_captured()` is true.
    pub fn pointer_down(
        &mut self,
        source: &mut impl TreeDataSource,
        frame: &GridFrame,
        point: Point,
        modifiers: SelectionModifiers,
    ) -> Result<bool, GridError> {
        if !point.is_finite() {
            return Err(GridError::InvalidGeometry("pointer must be finite"));
        }
        let Some(hit) = frame.hit_test(point) else {
            return Ok(false);
        };
        match hit {
            GridHitKind::ResizeColumn(id) => {
                let width = self.require_column(id)?.width;
                self.drag = Some(ColumnDrag::Resize {
                    id,
                    start_x: point.x,
                    width,
                });
            }
            GridHitKind::Header(id) => {
                self.drag = Some(ColumnDrag::Header {
                    id,
                    start: point,
                    moved: false,
                })
            }
            GridHitKind::Expander(row) => {
                self.set_expanded(source, row, !self.is_expanded(row))?;
            }
            GridHitKind::Cell { row, column } => self.focus_cell(source, row, column, modifiers)?,
        }
        Ok(true)
    }

    pub fn pointer_move(&mut self, point: Point) -> Result<bool, GridError> {
        if !point.is_finite() {
            return Err(GridError::InvalidGeometry("pointer must be finite"));
        }
        let Some(drag) = &mut self.drag else {
            return Ok(false);
        };
        match drag {
            ColumnDrag::Resize { id, start_x, width } => {
                let (id, width) = (*id, *width + point.x - *start_x);
                self.resize_column(id, width)?;
            }
            ColumnDrag::Header { start, moved, .. } => {
                *moved |= (point.x - start.x).abs() > 4.0 || (point.y - start.y).abs() > 4.0;
            }
        }
        Ok(true)
    }

    pub fn pointer_up(
        &mut self,
        source: &mut impl TreeDataSource,
        frame: &GridFrame,
        point: Point,
    ) -> Result<bool, GridError> {
        self.pointer_move(point)?;
        let Some(drag) = self.drag.take() else {
            return Ok(false);
        };
        if let ColumnDrag::Header { id, moved, .. } = drag {
            if moved {
                let before = match frame.hit_test(point) {
                    Some(GridHitKind::Header(column) | GridHitKind::ResizeColumn(column)) => {
                        Some(column)
                    }
                    _ => None,
                };
                self.reorder_column(id, before)?;
            } else {
                self.toggle_sort(source, id)?;
            }
        }
        Ok(true)
    }

    pub fn cancel_pointer(&mut self) {
        self.drag = None;
    }

    pub fn row(
        &self,
        source: &impl TreeDataSource,
        index: usize,
    ) -> Result<Option<VisibleRow>, GridError> {
        let Some(span) = self.index.span_at(index) else {
            return Ok(None);
        };
        if span.placeholder {
            return Ok(Some(VisibleRow {
                index,
                key: None,
                parent: span.parent,
                depth: span.depth,
                loading: span
                    .parent
                    .is_some_and(|parent| self.pending.contains_key(&parent)),
                error: span
                    .parent
                    .and_then(|parent| self.failures.get(&parent).cloned()),
            }));
        }
        let key = index::child(
            source,
            &self.loaded,
            span.parent,
            span.child_start + index - span.start,
        )
        .ok_or(GridError::InvalidHierarchy(
            "source omitted an indexed child",
        ))?;
        Ok(Some(VisibleRow {
            index,
            key: Some(key),
            parent: span.parent,
            depth: span.depth,
            loading: false,
            error: None,
        }))
    }

    pub fn visible_index(&self, source: &impl TreeDataSource, row: RowKey) -> Option<usize> {
        let (parent, position) = index::location(source, &self.locations, row)?;
        self.index.index_of(parent, position)
    }

    pub fn has_children(&self, source: &impl TreeDataSource, row: RowKey) -> bool {
        index::count(source, &self.loaded, Some(row)) != Some(0)
    }

    pub fn set_expanded(
        &mut self,
        source: &mut impl TreeDataSource,
        row: RowKey,
        expanded: bool,
    ) -> Result<Option<RequestId>, GridError> {
        if index::location(source, &self.locations, row).is_none() {
            return Err(GridError::UnknownRow(row));
        }
        let mut lineage = Some(row);
        let mut seen = HashSet::new();
        while let Some(key) = lineage {
            if !seen.insert(key) || seen.len() > 256 {
                return Err(GridError::InvalidHierarchy(
                    "cycle or hierarchy depth above 256",
                ));
            }
            lineage = index::location(source, &self.locations, key)
                .ok_or(GridError::UnknownRow(key))?
                .0;
        }
        if expanded && !self.has_children(source, row) {
            return Ok(None);
        }
        let mut next_expanded = self.expanded.clone();
        if expanded {
            next_expanded.insert(row);
        } else {
            next_expanded.remove(&row);
        }
        let next_index =
            SparseIndex::rebuild(source, &next_expanded, &self.loaded, &self.locations)?;
        let next_sequence = if expanded
            && index::count(source, &self.loaded, Some(row)).is_none()
            && !self.pending.contains_key(&row)
        {
            Some(self.sequence.checked_add(1).ok_or(GridError::Capacity)?)
        } else {
            None
        };
        self.expanded = next_expanded;
        self.index = next_index;
        if expanded {
            if let Some(sequence) = next_sequence {
                self.sequence = sequence;
                let request = RequestId {
                    model: self.model_id,
                    epoch: self.epoch,
                    sequence: self.sequence,
                    parent: row,
                };
                self.pending.insert(row, request);
                self.failures.remove(&row);
                self.clamp_scroll();
                match source.request_children(row, request) {
                    Ok(ChildrenLoad::Ready(rows)) => {
                        self.complete_children(source, request, Ok(rows))?;
                    }
                    Ok(ChildrenLoad::Pending) => {}
                    Err(error) => {
                        self.complete_children(source, request, Err(error))?;
                    }
                }
                return Ok(Some(request));
            }
        } else {
            let cancelled = self
                .pending
                .values()
                .copied()
                .filter(|request| self.is_descendant(source, request.parent, row))
                .collect::<Vec<_>>();
            for request in cancelled {
                self.pending.remove(&request.parent);
                source.cancel_request(request);
            }
            if self
                .focused
                .is_some_and(|focused| focused != row && self.is_descendant(source, focused, row))
            {
                self.focused = Some(row);
            }
            if self
                .editor
                .as_ref()
                .is_some_and(|edit| edit.row != row && self.is_descendant(source, edit.row, row))
            {
                self.editor = None;
            }
        }
        self.clamp_scroll();
        Ok(None)
    }

    /// Returns false for stale/cancelled/foreign tokens without touching state.
    /// Response rows are cached as data keys, never as UI nodes. Existing loaded
    /// branches survive collapse; requests still pending when collapsed cancel.
    pub fn complete_children(
        &mut self,
        source: &impl TreeDataSource,
        request: RequestId,
        response: Result<Vec<RowKey>, String>,
    ) -> Result<bool, GridError> {
        if request.model != self.model_id
            || request.epoch != self.epoch
            || self.pending.get(&request.parent) != Some(&request)
        {
            return Ok(false);
        }
        match response {
            Ok(rows) => {
                let mut unique = HashSet::with_capacity(rows.len());
                for &row in &rows {
                    if !unique.insert(row) || self.is_descendant(source, request.parent, row) {
                        return Err(GridError::InvalidHierarchy(
                            "lazy response contains duplicates or an ancestor",
                        ));
                    }
                    if let Some((parent, _)) = index::location(source, &self.locations, row)
                        && parent != Some(request.parent)
                    {
                        return Err(GridError::InvalidHierarchy(
                            "lazy row key already belongs to another parent",
                        ));
                    }
                }
                let previous_locations = rows
                    .iter()
                    .enumerate()
                    .map(|(position, &row)| {
                        (row, self.locations.insert(row, (request.parent, position)))
                    })
                    .collect::<Vec<_>>();
                let previous_rows = self.loaded.insert(request.parent, rows);
                if let Err(error) = self.reindex(source) {
                    if let Some(rows) = previous_rows {
                        self.loaded.insert(request.parent, rows);
                    } else {
                        self.loaded.remove(&request.parent);
                    }
                    for (row, previous) in previous_locations {
                        if let Some(location) = previous {
                            self.locations.insert(row, location);
                        } else {
                            self.locations.remove(&row);
                        }
                    }
                    return Err(error);
                }
                self.failures.remove(&request.parent);
            }
            Err(error) => {
                self.failures.insert(request.parent, error);
            }
        }
        self.pending.remove(&request.parent);
        Ok(true)
    }

    pub fn select(
        &mut self,
        source: &impl TreeDataSource,
        row: RowKey,
        modifiers: SelectionModifiers,
    ) -> Result<(), GridError> {
        let target = self
            .visible_index(source, row)
            .ok_or(GridError::UnknownRow(row))?;
        if modifiers.shift {
            let anchor = self
                .anchor
                .and_then(|key| self.visible_index(source, key))
                .unwrap_or(target);
            let mut keys = Vec::new();
            for index in anchor.min(target)..=anchor.max(target) {
                if let Some(key) = self.row(source, index)?.and_then(|row| row.key) {
                    keys.push(key);
                }
            }
            if !modifiers.control {
                self.selected.clear();
            }
            self.selected.extend(keys);
        } else {
            if modifiers.control {
                if !self.selected.remove(&row) {
                    self.selected.insert(row);
                }
            } else {
                self.selected.clear();
                self.selected.insert(row);
            }
            self.anchor = Some(row);
        }
        self.focused = Some(row);
        self.ensure_visible(target);
        Ok(())
    }

    pub fn focus_cell(
        &mut self,
        source: &impl TreeDataSource,
        row: RowKey,
        column: ColumnId,
        modifiers: SelectionModifiers,
    ) -> Result<(), GridError> {
        self.require_column(column)?;
        self.select(source, row, modifiers)?;
        self.focused_column = Some(column);
        Ok(())
    }

    pub fn clear_selection(&mut self) {
        self.selected.clear();
        self.anchor = None;
    }

    pub fn select_all(&mut self, source: &impl TreeDataSource) -> Result<(), GridError> {
        let mut selected = HashSet::new();
        for index in 0..self.row_count() {
            if let Some(key) = self.row(source, index)?.and_then(|row| row.key) {
                selected.insert(key);
            }
        }
        self.selected = selected;
        Ok(())
    }

    pub fn navigate(
        &mut self,
        source: &mut impl TreeDataSource,
        key: GridKey,
        modifiers: SelectionModifiers,
    ) -> Result<(), GridError> {
        match key {
            GridKey::Escape => {
                self.cancel_edit();
                return Ok(());
            }
            GridKey::Enter if self.editor.is_some() => {
                self.commit_edit(source)?;
                return Ok(());
            }
            GridKey::Enter | GridKey::F2 => {
                if let (Some(row), Some(column)) = (self.focused, self.focused_column) {
                    self.begin_edit(source, row, column)?;
                }
                return Ok(());
            }
            GridKey::Right => {
                if let Some(row) = self.focused
                    && self.has_children(source, row)
                {
                    if !self.is_expanded(row) {
                        self.set_expanded(source, row, true)?;
                    } else if let Some(child) = index::child(source, &self.loaded, Some(row), 0) {
                        self.select(source, child, modifiers)?;
                    }
                }
                return Ok(());
            }
            GridKey::Left => {
                if let Some(row) = self.focused {
                    if self.is_expanded(row) {
                        self.set_expanded(source, row, false)?;
                    } else if let Some(parent) =
                        index::location(source, &self.locations, row).and_then(|item| item.0)
                    {
                        self.select(source, parent, modifiers)?;
                    }
                }
                return Ok(());
            }
            _ => {}
        }
        if self.row_count() == 0 {
            return Ok(());
        }
        let current = self.focused.and_then(|row| self.visible_index(source, row));
        let page = ((self.viewport.height - self.config.header_height).max(0.0)
            / self.config.row_height)
            .floor()
            .max(1.0) as usize;
        let index = match key {
            GridKey::Home => 0,
            GridKey::End => self.row_count() - 1,
            GridKey::Up => current.unwrap_or(1).saturating_sub(1),
            GridKey::Down => current.map_or(0, |x| x.saturating_add(1)),
            GridKey::PageUp => current.unwrap_or(0).saturating_sub(page),
            GridKey::PageDown => current.unwrap_or(0).saturating_add(page),
            _ => return Ok(()),
        }
        .min(self.row_count() - 1);
        let backwards = matches!(key, GridKey::Up | GridKey::PageUp | GridKey::End);
        let mut candidate = index;
        loop {
            if let Some(row) = self.row(source, candidate)?.and_then(|row| row.key) {
                if modifiers.control && !modifiers.shift {
                    self.focused = Some(row);
                    self.ensure_visible(candidate);
                } else {
                    self.select(source, row, modifiers)?;
                }
                return Ok(());
            }
            if backwards {
                if candidate == 0 {
                    break;
                }
                candidate -= 1;
            } else {
                candidate += 1;
                if candidate >= self.row_count() {
                    break;
                }
            }
        }
        Ok(())
    }

    pub fn apply_query(
        &mut self,
        source: &mut impl TreeDataSource,
        query: GridQuery,
    ) -> Result<(), GridError> {
        for sort in &query.sort {
            self.require_column(sort.column)?;
        }
        source.apply_query(&query).map_err(GridError::Source)?;
        self.refresh(source)?;
        self.query = query;
        self.scroll_y = 0.0;
        Ok(())
    }

    pub fn toggle_sort(
        &mut self,
        source: &mut impl TreeDataSource,
        column: ColumnId,
    ) -> Result<(), GridError> {
        self.require_column(column)?;
        let direction =
            if self.query.sort.first().is_some_and(|sort| {
                sort.column == column && sort.direction == SortDirection::Ascending
            }) {
                SortDirection::Descending
            } else {
                SortDirection::Ascending
            };
        let mut query = self.query.clone();
        query.sort = vec![SortOrder { column, direction }];
        self.apply_query(source, query)
    }

    pub fn begin_edit(
        &mut self,
        source: &impl TreeDataSource,
        row: RowKey,
        column: ColumnId,
    ) -> Result<(), GridError> {
        if !self.require_column(column)?.editable || !source.editable(row, column) {
            return Err(GridError::ReadOnly);
        }
        let position = self
            .visible_index(source, row)
            .ok_or(GridError::UnknownRow(row))?;
        let original = source.cell(row, column);
        self.editor = Some(CellEdit {
            row,
            column,
            value: original.clone(),
            original,
            error: None,
        });
        self.focused = Some(row);
        self.focused_column = Some(column);
        self.ensure_visible(position);
        Ok(())
    }

    pub fn set_edit_text(&mut self, text: impl Into<String>) -> Result<(), GridError> {
        let editor = self.editor.as_mut().ok_or(GridError::NoEdit)?;
        editor.value = text.into();
        editor.error = None;
        Ok(())
    }

    pub fn cancel_edit(&mut self) {
        self.editor = None;
    }

    pub fn commit_edit(&mut self, source: &mut impl TreeDataSource) -> Result<(), GridError> {
        let editor = self.editor.as_mut().ok_or(GridError::NoEdit)?;
        if !source.editable(editor.row, editor.column) {
            return Err(GridError::ReadOnly);
        }
        if let Err(error) = source.validate_edit(editor.row, editor.column, &editor.value) {
            editor.error = Some(error.clone());
            return Err(GridError::Validation(error));
        }
        if let Err(error) = source.set_cell(editor.row, editor.column, &editor.value) {
            editor.error = Some(error.clone());
            return Err(GridError::Source(error));
        }
        self.editor = None;
        Ok(())
    }

    pub fn resize_column(&mut self, id: ColumnId, width: f32) -> Result<(), GridError> {
        if !width.is_finite() {
            return Err(GridError::InvalidColumn("width must be finite"));
        }
        let column = self
            .columns
            .iter_mut()
            .find(|column| column.id == id)
            .ok_or(GridError::UnknownColumn(id))?;
        column.width = width.clamp(column.min_width, column.max_width);
        self.index_columns();
        self.clamp_scroll();
        Ok(())
    }

    /// Move a column before another ID, or to the end. Pinning remains attached
    /// to the column; pinned and scrolling subsets retain their relative order.
    pub fn reorder_column(
        &mut self,
        id: ColumnId,
        before: Option<ColumnId>,
    ) -> Result<(), GridError> {
        self.require_column(id)?;
        if let Some(before) = before {
            self.require_column(before)?;
            if before == id {
                return Ok(());
            }
        }
        let position = self
            .columns
            .iter()
            .position(|column| column.id == id)
            .expect("validated column");
        let column = self.columns.remove(position);
        let destination = before
            .and_then(|id| self.columns.iter().position(|column| column.id == id))
            .unwrap_or(self.columns.len());
        self.columns.insert(destination, column);
        self.index_columns();
        Ok(())
    }

    pub fn pin_column(&mut self, id: ColumnId, pinned: bool) -> Result<(), GridError> {
        let column = self
            .columns
            .iter_mut()
            .find(|column| column.id == id)
            .ok_or(GridError::UnknownColumn(id))?;
        column.pinned = pinned;
        self.index_columns();
        self.clamp_scroll();
        Ok(())
    }

    /// Portable versioned text; the application chooses its persistence path.
    pub fn save_columns(&self) -> String {
        let mut result = String::from("treegrid-columns\t1\n");
        for column in &self.columns {
            result.push_str(&format!(
                "{}\t{}\t{}\n",
                column.id.0,
                column.width,
                u8::from(column.pinned)
            ));
        }
        result
    }

    /// Parse atomically. Unknown historical IDs are ignored, new columns append,
    /// and widths are clamped to the current schema's min/max limits.
    pub fn restore_columns(&mut self, encoded: &str) -> Result<(), GridError> {
        let mut lines = encoded.lines();
        if lines.next() != Some("treegrid-columns\t1") {
            return Err(GridError::InvalidPersistence("unsupported layout version"));
        }
        let mut seen = HashSet::new();
        let mut result = Vec::new();
        for line in lines {
            let fields = line.split('\t').collect::<Vec<_>>();
            if fields.len() != 3 {
                return Err(GridError::InvalidPersistence("expected id, width, pinned"));
            }
            let id = ColumnId(
                fields[0]
                    .parse()
                    .map_err(|_| GridError::InvalidPersistence("invalid column ID"))?,
            );
            let width: f32 = fields[1]
                .parse()
                .map_err(|_| GridError::InvalidPersistence("invalid width"))?;
            if !width.is_finite() || width <= 0.0 || !seen.insert(id) {
                return Err(GridError::InvalidPersistence(
                    "nonpositive width or duplicate column ID",
                ));
            }
            let pinned = match fields[2] {
                "0" => false,
                "1" => true,
                _ => return Err(GridError::InvalidPersistence("pinned must be 0 or 1")),
            };
            if let Some(column) = self.columns.iter().find(|column| column.id == id) {
                let mut column = column.clone();
                column.width = width.clamp(column.min_width, column.max_width);
                column.pinned = pinned;
                result.push(column);
            }
        }
        result.extend(
            self.columns
                .iter()
                .filter(|column| !seen.contains(&column.id))
                .cloned(),
        );
        self.columns = result;
        self.index_columns();
        self.clamp_scroll();
        Ok(())
    }

    fn require_column(&self, id: ColumnId) -> Result<&GridColumn, GridError> {
        self.columns
            .iter()
            .find(|column| column.id == id)
            .ok_or(GridError::UnknownColumn(id))
    }

    fn reindex(&mut self, source: &impl TreeDataSource) -> Result<(), GridError> {
        self.index = SparseIndex::rebuild(source, &self.expanded, &self.loaded, &self.locations)?;
        self.clamp_scroll();
        Ok(())
    }

    fn is_descendant(&self, source: &impl TreeDataSource, row: RowKey, ancestor: RowKey) -> bool {
        let mut current = Some(row);
        let mut seen = HashSet::new();
        while let Some(row) = current {
            if row == ancestor {
                return true;
            }
            if !seen.insert(row) {
                return false;
            }
            current = index::location(source, &self.locations, row).and_then(|item| item.0);
        }
        false
    }

    fn index_columns(&mut self) {
        self.regular.clear();
        self.pinned.clear();
        self.column_ends.clear();
        self.pinned_width = 0.0;
        let mut width = 0.0;
        for (index, column) in self.columns.iter().enumerate() {
            if column.pinned {
                self.pinned.push(index);
                self.pinned_width += f64::from(column.width);
            } else {
                self.regular.push(index);
                width += f64::from(column.width);
                self.column_ends.push(width);
            }
        }
    }

    fn clamp_scroll(&mut self) {
        let height = f64::from((self.viewport.height - self.config.header_height).max(0.0));
        let width = (f64::from(self.viewport.width) - self.pinned_width).max(0.0);
        self.scroll_y = self
            .scroll_y
            .min((self.row_count() as f64 * f64::from(self.config.row_height) - height).max(0.0));
        self.scroll_x = self
            .scroll_x
            .min((self.column_ends.last().copied().unwrap_or(0.0) - width).max(0.0));
    }

    fn ensure_visible(&mut self, index: usize) {
        let top = index as f64 * f64::from(self.config.row_height);
        let bottom = top + f64::from(self.config.row_height);
        let height = f64::from((self.viewport.height - self.config.header_height).max(0.0));
        if top < self.scroll_y {
            self.scroll_y = top;
        } else if bottom > self.scroll_y + height {
            self.scroll_y = (bottom - height).max(0.0);
        }
        self.clamp_scroll();
    }
}

fn validate_columns(columns: &[GridColumn]) -> Result<(), GridError> {
    if columns.is_empty() {
        return Err(GridError::InvalidColumn("at least one column is required"));
    }
    let mut ids = HashSet::new();
    for column in columns {
        column.validate()?;
        if !ids.insert(column.id) {
            return Err(GridError::InvalidColumn("duplicate column ID"));
        }
    }
    Ok(())
}
