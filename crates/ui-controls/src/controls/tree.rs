//! Retained hosts for the shared virtual TreeGrid model.
use super::*;
use rust_desktop_ui_treegrid::{
    ChildrenLoad, ColumnId, GridColumn, GridConfig, GridFrame, GridPalette, GridQuery, RequestId,
    RowKey, SelectionModifiers, TreeDataSource, TreeGrid, TreeMoveTarget,
};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone)]
pub struct TreeSourceHandle(Rc<RefCell<dyn TreeDataSource>>);
impl TreeSourceHandle {
    pub fn new(source: impl TreeDataSource + 'static) -> Self {
        Self(Rc::new(RefCell::new(source)))
    }
    pub fn from_shared<T: TreeDataSource + 'static>(source: Rc<RefCell<T>>) -> Self {
        Self(source)
    }
    pub fn same_source(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
    /// Host operations require an unborrowed provider, including reads that can
    /// trigger lazy requests. This check performs no storage access or mutation.
    pub fn check_available(&self) -> Result<(), String> {
        self.0
            .try_borrow_mut()
            .map(|_| ())
            .map_err(|_| "Tree source is already borrowed".into())
    }
}
impl TreeDataSource for TreeSourceHandle {
    fn child_count(&self, parent: Option<RowKey>) -> Option<usize> {
        self.0.borrow().child_count(parent)
    }
    fn child_at(&self, parent: Option<RowKey>, index: usize) -> Option<RowKey> {
        self.0.borrow().child_at(parent, index)
    }
    fn parent(&self, row: RowKey) -> Option<RowKey> {
        self.0.borrow().parent(row)
    }
    fn position(&self, row: RowKey) -> Option<usize> {
        self.0.borrow().position(row)
    }
    fn cell(&self, row: RowKey, col: ColumnId) -> String {
        self.0.borrow().cell(row, col)
    }
    fn revision(&self) -> u64 {
        self.0.borrow().revision()
    }
    fn request_children(
        &mut self,
        parent: RowKey,
        request: RequestId,
    ) -> Result<ChildrenLoad, String> {
        self.0
            .try_borrow_mut()
            .map_err(|_| "Tree source is already borrowed".to_owned())?
            .request_children(parent, request)
    }
    fn cancel_request(&mut self, request: RequestId) {
        if let Ok(mut source) = self.0.try_borrow_mut() {
            source.cancel_request(request);
        }
    }
    fn apply_query(&mut self, query: &GridQuery) -> Result<(), String> {
        self.0
            .try_borrow_mut()
            .map_err(|_| "Tree source is already borrowed".to_owned())?
            .apply_query(query)
    }
    fn validate_move(&self, rows: &[RowKey], target: TreeMoveTarget) -> Result<(), String> {
        self.0
            .try_borrow()
            .map_err(|_| "Tree source is already borrowed".to_owned())?
            .validate_move(rows, target)
    }
    fn move_nodes(&mut self, rows: &[RowKey], target: TreeMoveTarget) -> Result<(), String> {
        self.0
            .try_borrow_mut()
            .map_err(|_| "Tree source is already borrowed".to_owned())?
            .move_nodes(rows, target)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TreeControlSettings {
    pub columns: Vec<GridColumn>,
    pub show_header: bool,
    pub selection_mode: SelectionMode,
    pub allow_node_drag: bool,
    /// Restricts user node moves. Selection/expansion and application writes
    /// remain available; this host never opens a cell editor.
    pub read_only: bool,
}
impl Default for TreeControlSettings {
    fn default() -> Self {
        Self {
            columns: vec![GridColumn::new(ColumnId(0), "Название", 240.0)],
            show_header: true,
            selection_mode: SelectionMode::Single,
            allow_node_drag: false,
            read_only: true,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeSelectionChanged {
    pub id: WidgetId,
    pub keys: Vec<RowKey>,
}

#[derive(Default)]
pub(super) struct Trees {
    pub(super) hosts: HashMap<WidgetId, TreeState>,
    sources: HashMap<String, TreeSourceHandle>,
    changes: Vec<TreeSelectionChanged>,
}
pub(super) struct TreeState {
    pub(super) source: TreeSourceHandle,
    pub(super) model: TreeGrid,
    pub(super) frame: GridFrame,
    pub(super) settings: TreeControlSettings,
    pub(super) revision: u64,
    pub(super) bounds: Option<Rect>,
    pub(super) drag: Option<super::tree_input::NodeDrag>,
    pub(super) scroll: super::tree_scroll::TreeScroll,
    pub(super) error: Option<String>,
    pub(super) selection_validator: Option<SelectionValidator>,
}
pub(super) fn tree_error(error: impl ToString) -> ControlError {
    ControlError::Invalid(error.to_string())
}
impl Controls {
    fn prepare_tree_operation(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let state = self
            .trees
            .hosts
            .get(&id)
            .ok_or_else(|| tree_error("Unknown tree"))?;
        state.source.check_available().map_err(tree_error)?;
        if state.source.revision() != state.revision {
            self.tree_refresh(id)?;
        }
        Ok(())
    }
    pub fn register_tree_source(
        &mut self,
        name: impl Into<String>,
        source: TreeSourceHandle,
    ) -> Result<(), ControlError> {
        let name = name.into();
        if name.is_empty() || name.len() > 256 || name.contains('\0') {
            return Err(tree_error("Invalid tree source name"));
        }
        if self.trees.sources.contains_key(&name) {
            return Err(tree_error("Tree source name already registered"));
        }
        self.trees.sources.insert(name, source);
        Ok(())
    }
    pub fn tree_source(&self, name: &str) -> Option<TreeSourceHandle> {
        self.trees.sources.get(name).cloned()
    }
    pub fn tree_view(
        &mut self,
        parent: WidgetId,
        source: TreeSourceHandle,
        display_column: ColumnId,
        style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        self.tree_grid(
            parent,
            source,
            TreeControlSettings {
                columns: vec![GridColumn::new(display_column, "", 240.0)],
                show_header: false,
                ..Default::default()
            },
            style,
        )
    }
    pub fn tree_grid(
        &mut self,
        parent: WidgetId,
        mut source: TreeSourceHandle,
        mut settings: TreeControlSettings,
        mut style: LayoutStyle,
    ) -> Result<WidgetId, ControlError> {
        source.check_available().map_err(tree_error)?;
        if settings.columns.is_empty()
            || settings.columns.len() > 64
            || (!settings.show_header && settings.columns.len() != 1)
        {
            return Err(tree_error(
                "Tree requires 1..64 columns; a headerless tree has one column",
            ));
        }
        for column in &mut settings.columns {
            if column.title.len() > 4096
                || column.title.contains('\0')
                || column.min_width < 32.0
                || column.max_width > 4096.0
            {
                return Err(tree_error("Invalid tree column limits"));
            }
            self.measure_text(&column.title, 4096.0, false)?;
            column.editable = false;
        }
        let config = GridConfig {
            header_height: if settings.show_header { 32.0 } else { 0.0 },
            ..Default::default()
        };
        let mut model =
            TreeGrid::with_config(settings.columns.clone(), config).map_err(tree_error)?;
        model.refresh(&mut source).map_err(tree_error)?;
        // Validate initial visible graph and text before creating a retained host.
        let frame = model
            .paint(&source, Rect::new(0.0, 0.0, 320.0, 320.0))
            .map_err(tree_error)?;
        self.preflight_tree_frame(&frame)?;
        style.kind = LayoutKind::Overlay;
        if style.height == Length::Auto && style.flex_grow == 0.0 {
            style.height = Length::Px(320.0);
        }
        let id = self.panel(parent, style)?;
        self.controls.get_mut(&id).unwrap().role = Some(if settings.show_header {
            Role::Grid
        } else {
            Role::Tree
        });
        self.edit_props(id, |props| {
            props.focusable = true;
            props.tab_stop = true;
        })?;
        let revision = source.revision();
        self.trees.hosts.insert(
            id,
            TreeState {
                source,
                model,
                frame,
                settings,
                revision,
                bounds: None,
                drag: None,
                scroll: Default::default(),
                error: None,
                selection_validator: None,
            },
        );
        Ok(id)
    }
    fn preflight_tree_frame(&mut self, frame: &GridFrame) -> Result<(), ControlError> {
        for run in &frame.scene.texts {
            self.measure_text(&run.text, 4096.0, false)?;
        }
        Ok(())
    }
    pub fn tree_frame(&self, id: WidgetId) -> Option<&GridFrame> {
        self.trees.hosts.get(&id).map(|s| &s.frame)
    }
    pub fn tree_model(&self, id: WidgetId) -> Option<&TreeGrid> {
        self.trees.hosts.get(&id).map(|s| &s.model)
    }
    pub fn tree_settings(&self, id: WidgetId) -> Option<&TreeControlSettings> {
        self.trees.hosts.get(&id).map(|s| &s.settings)
    }
    pub fn tree_error(&self, id: WidgetId) -> Option<&str> {
        self.trees.hosts.get(&id)?.error.as_deref()
    }
    pub fn take_tree_selection_changes(&mut self) -> Vec<TreeSelectionChanged> {
        std::mem::take(&mut self.trees.changes)
    }
    pub fn tree_selected_keys(&self, id: WidgetId) -> Option<Vec<RowKey>> {
        let mut keys: Vec<_> = self
            .trees
            .hosts
            .get(&id)?
            .model
            .selected()
            .iter()
            .copied()
            .collect();
        keys.sort();
        Some(keys)
    }
    pub fn set_tree_selected_key(
        &mut self,
        id: WidgetId,
        key: Option<RowKey>,
    ) -> Result<(), ControlError> {
        self.prepare_tree_operation(id)?;
        let before = self
            .tree_selected_keys(id)
            .ok_or_else(|| tree_error("Unknown tree"))?;
        let state = self.trees.hosts.get_mut(&id).unwrap();
        state.source.check_available().map_err(tree_error)?;
        if let Some(validator) = &state.selection_validator {
            validator
                .check(&key.map(|key| vec![key.0.to_string()]).unwrap_or_default())
                .map_err(tree_error)?;
        }
        state
            .model
            .assign_selected_key(&state.source, key)
            .map_err(tree_error)?;
        state.error = None;
        self.record_tree_selection(id, &before);
        self.scene_dirty = true;
        Ok(())
    }
    pub fn set_tree_selection_validator(
        &mut self,
        id: WidgetId,
        validator: Option<SelectionValidator>,
    ) -> Result<(), ControlError> {
        let keys = self
            .tree_selected_keys(id)
            .ok_or_else(|| tree_error("Unknown tree"))?;
        if let Some(validator) = &validator {
            validator
                .check(&keys.iter().map(|key| key.0.to_string()).collect::<Vec<_>>())
                .map_err(tree_error)?;
        }
        self.trees.hosts.get_mut(&id).unwrap().selection_validator = validator;
        Ok(())
    }
    pub(crate) fn chain_tree_selection_validator(
        &mut self,
        id: WidgetId,
        validator: SelectionValidator,
    ) -> Result<(), ControlError> {
        let state = self
            .trees
            .hosts
            .get_mut(&id)
            .ok_or_else(|| tree_error("Unknown tree"))?;
        let previous = state.selection_validator.take();
        state.selection_validator = Some(SelectionValidator::new(move |keys| {
            if let Some(previous) = &previous {
                previous.check(keys)?;
            }
            validator.check(keys)
        }));
        Ok(())
    }
    pub(super) fn validate_tree_selection(
        &mut self,
        id: WidgetId,
        snapshot: rust_desktop_ui_treegrid::TreeSelectionSnapshot,
    ) -> Result<(), ControlError> {
        let keys = self.tree_selected_keys(id).unwrap();
        let state = self.trees.hosts.get_mut(&id).unwrap();
        if let Some(validator) = &state.selection_validator
            && let Err(error) =
                validator.check(&keys.iter().map(|key| key.0.to_string()).collect::<Vec<_>>())
        {
            state
                .model
                .restore_selection_snapshot(snapshot)
                .map_err(tree_error)?;
            state.drag = None;
            state.model.cancel_pointer();
            state.error = Some(error.clone());
            return Err(tree_error(error));
        }
        Ok(())
    }
    pub(super) fn record_tree_selection(&mut self, id: WidgetId, before: &[RowKey]) {
        if let Some(keys) = self.tree_selected_keys(id)
            && keys != before
        {
            self.trees.changes.push(TreeSelectionChanged { id, keys });
        }
    }
    pub fn set_tree_interaction(
        &mut self,
        id: WidgetId,
        allow_node_drag: bool,
        read_only: bool,
    ) -> Result<(), ControlError> {
        let state = self
            .trees
            .hosts
            .get_mut(&id)
            .ok_or_else(|| tree_error("Unknown tree"))?;
        state.settings.allow_node_drag = allow_node_drag;
        state.settings.read_only = read_only;
        state.drag = None;
        state.scroll.cancel();
        state.model.cancel_pointer();
        if self.runtime.captured() == Some(id) {
            self.runtime.apply(Mutation::CapturePointer(None))?;
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub fn tree_set_expanded(
        &mut self,
        id: WidgetId,
        key: RowKey,
        expanded: bool,
    ) -> Result<Option<RequestId>, ControlError> {
        self.prepare_tree_operation(id)?;
        let state = self
            .trees
            .hosts
            .get_mut(&id)
            .ok_or_else(|| tree_error("Unknown tree"))?;
        state.source.check_available().map_err(tree_error)?;
        let request = state
            .model
            .set_expanded(&mut state.source, key, expanded)
            .map_err(tree_error)?;
        self.scene_dirty = true;
        Ok(request)
    }
    pub fn tree_complete_children(
        &mut self,
        id: WidgetId,
        request: RequestId,
        result: Result<Vec<RowKey>, String>,
    ) -> Result<bool, ControlError> {
        let state = self
            .trees
            .hosts
            .get_mut(&id)
            .ok_or_else(|| tree_error("Unknown tree"))?;
        state.source.check_available().map_err(tree_error)?;
        let changed = state
            .model
            .complete_children(&state.source, request, result)
            .map_err(tree_error)?;
        if changed {
            state.revision = state.source.revision();
            self.scene_dirty = true;
        }
        Ok(changed)
    }
    /// Built-in source completion validates the token before changing records.
    /// The provided shared source must be the exact source mounted in this tree.
    pub fn tree_complete_indexed_children(
        &mut self,
        id: WidgetId,
        source: &Rc<RefCell<rust_desktop_ui_treegrid::IndexedTreeSource>>,
        request: RequestId,
        children: Vec<rust_desktop_ui_treegrid::TreeNode>,
    ) -> Result<bool, ControlError> {
        let state = self
            .trees
            .hosts
            .get_mut(&id)
            .ok_or_else(|| tree_error("Unknown tree"))?;
        if !state
            .source
            .same_source(&TreeSourceHandle::from_shared(Rc::clone(source)))
        {
            return Err(tree_error("Tree completion source identity does not match"));
        }
        state.source.check_available().map_err(tree_error)?;
        let changed = state
            .model
            .complete_indexed_children(&mut source.borrow_mut(), request, children)
            .map_err(tree_error)?;
        if changed {
            state.revision = state.source.revision();
            self.scene_dirty = true;
        }
        Ok(changed)
    }
    pub fn tree_refresh(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let state = self
            .trees
            .hosts
            .get_mut(&id)
            .ok_or_else(|| tree_error("Unknown tree"))?;
        state.source.check_available().map_err(tree_error)?;
        state.model.refresh(&mut state.source).map_err(tree_error)?;
        state.revision = state.source.revision();
        state.drag = None;
        state.scroll.cancel();
        state.model.cancel_pointer();
        state.error = None;
        if self.runtime.captured() == Some(id) {
            self.runtime.apply(Mutation::CapturePointer(None))?;
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub fn tree_move_nodes(
        &mut self,
        id: WidgetId,
        keys: &[RowKey],
        target: TreeMoveTarget,
    ) -> Result<(), ControlError> {
        self.prepare_tree_operation(id)?;
        let state = self
            .trees
            .hosts
            .get_mut(&id)
            .ok_or_else(|| tree_error("Unknown tree"))?;
        state.source.check_available().map_err(tree_error)?;
        state
            .model
            .move_nodes(&mut state.source, keys, target)
            .map_err(tree_error)?;
        state.revision = state.source.revision();
        state.drag = None;
        state.scroll.cancel();
        state.error = None;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn tree_scroll_to(&mut self, id: WidgetId, x: f64, y: f64) -> Result<(), ControlError> {
        self.trees
            .hosts
            .get_mut(&id)
            .ok_or_else(|| tree_error("Unknown tree"))?
            .model
            .scroll_to(x, y)
            .map_err(tree_error)?;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn tree_resize_column(
        &mut self,
        id: WidgetId,
        column: ColumnId,
        width: f32,
    ) -> Result<(), ControlError> {
        self.trees
            .hosts
            .get_mut(&id)
            .ok_or_else(|| tree_error("Unknown tree"))?
            .model
            .resize_column(column, width)
            .map_err(tree_error)?;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn tree_reorder_column(
        &mut self,
        id: WidgetId,
        column: ColumnId,
        before: Option<ColumnId>,
    ) -> Result<(), ControlError> {
        self.trees
            .hosts
            .get_mut(&id)
            .ok_or_else(|| tree_error("Unknown tree"))?
            .model
            .reorder_column(column, before)
            .map_err(tree_error)?;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn tree_pin_column(
        &mut self,
        id: WidgetId,
        column: ColumnId,
        pinned: bool,
    ) -> Result<(), ControlError> {
        self.trees
            .hosts
            .get_mut(&id)
            .ok_or_else(|| tree_error("Unknown tree"))?
            .model
            .pin_column(column, pinned)
            .map_err(tree_error)?;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn tree_perform_action(
        &mut self,
        id: WidgetId,
        row: RowKey,
        column: Option<ColumnId>,
        action: SemanticAction,
    ) -> Result<(), ControlError> {
        if !self.is_enabled(id) {
            return Ok(());
        }
        if let Some(state) = self.trees.hosts.get(&id) {
            state.source.check_available().map_err(tree_error)?;
            if state.source.revision() != state.revision {
                self.tree_refresh(id)?;
                self.sync_trees()?;
                // The caller's virtual action refers to the old projection.
                return Ok(());
            }
        }
        let state = self
            .trees
            .hosts
            .get_mut(&id)
            .ok_or_else(|| tree_error("Unknown tree"))?;
        state.source.check_available().map_err(tree_error)?;
        match action {
            SemanticAction::Expand | SemanticAction::Collapse => {
                state
                    .model
                    .set_expanded(&mut state.source, row, action == SemanticAction::Expand)
                    .map_err(tree_error)?;
            }
            SemanticAction::Focus | SemanticAction::Activate => {
                let before = self.tree_selected_keys(id).unwrap();
                let state = self.trees.hosts.get_mut(&id).unwrap();
                let snapshot = state.model.selection_snapshot();
                let column = column.unwrap_or(state.model.tree_column());
                state
                    .model
                    .focus_cell(&state.source, row, column, SelectionModifiers::default())
                    .map_err(tree_error)?;
                self.validate_tree_selection(id, snapshot)?;
                self.record_tree_selection(id, &before);
                self.runtime.apply(Mutation::Focus(Some(id)))?;
            }
            _ => return Err(tree_error("Unsupported tree action")),
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub(super) fn prune_trees(&mut self) {
        let ids: Vec<_> = self
            .trees
            .hosts
            .keys()
            .filter(|id| !self.controls.contains_key(id))
            .copied()
            .collect();
        for id in ids {
            if let Some(mut state) = self.trees.hosts.remove(&id) {
                state.model.cancel_pending_requests(&mut state.source);
            }
        }
        self.trees
            .changes
            .retain(|event| self.controls.contains_key(&event.id));
    }
    pub(super) fn cancel_tree_pointers(&mut self) {
        for state in self.trees.hosts.values_mut() {
            state.drag = None;
            state.scroll.cancel();
            state.model.cancel_pointer();
        }
    }
    pub(super) fn sync_trees(&mut self) -> Result<(), ControlError> {
        let ids: Vec<_> = self.trees.hosts.keys().copied().collect();
        let theme = self.theme.tokens();
        let chrome = self.theme.chrome();
        for id in ids {
            let available = self.is_enabled(id);
            let bounds = self.runtime.bounds(id);
            let state = self.trees.hosts.get_mut(&id).unwrap();
            state.source.check_available().map_err(tree_error)?;
            if !available
                || state.bounds.is_some_and(|old| Some(old) != bounds)
                || state.source.revision() != state.revision
            {
                state.drag = None;
                state.scroll.cancel();
                state.model.cancel_pointer();
                if self.runtime.captured() == Some(id) {
                    self.runtime.apply(Mutation::CapturePointer(None))?;
                }
            }
            if state.source.revision() != state.revision {
                state.model.refresh(&mut state.source).map_err(tree_error)?;
                state.revision = state.source.revision();
            }
            state.bounds = bounds;
            let Some(bounds) = bounds else {
                continue;
            };
            let viewport = state
                .scroll
                .geometry(&state.model, bounds, !state.settings.show_header);
            if !state.settings.show_header {
                let column = state.model.columns()[0].clone();
                let width = viewport.width.clamp(column.min_width, column.max_width);
                if column.width != width {
                    state
                        .model
                        .resize_column(column.id, width)
                        .map_err(tree_error)?;
                }
            }
            state.model.palette = GridPalette {
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
            let candidate = state
                .model
                .paint(&state.source, viewport)
                .map_err(tree_error)?;
            state
                .scroll
                .geometry(&state.model, bounds, !state.settings.show_header);
            self.preflight_tree_frame(&candidate)?;
            let state = self.trees.hosts.get_mut(&id).unwrap();
            state.frame = candidate;
            let mut scene = state.frame.scene.clone();
            for bar in &state.scroll.bars {
                scene.fill(bar.track, chrome.card_surface);
                scene.fill(bar.thumb, theme.muted);
            }
            if let Some(drag) = &state.drag
                && drag.active
                && let Some((_, marker)) = drag.target
            {
                scene.fill(marker, theme.focus);
            }
            self.set_custom_scene(id, scene)?;
        }
        Ok(())
    }
}
