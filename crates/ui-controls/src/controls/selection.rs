//! Stable-key selection and retained editor compositions.
use super::*;
use rust_desktop_ui_core::Modifiers;
use std::rc::Rc;
use std::time::{Duration, Instant};

type SelectionValidationCallback = dyn Fn(&[String]) -> Result<(), String>;
/// Validates stable keys before either selection or its display text changes.
#[derive(Clone)]
pub struct SelectionValidator(Rc<SelectionValidationCallback>);
impl SelectionValidator {
    pub fn new(validator: impl Fn(&[String]) -> Result<(), String> + 'static) -> Self {
        Self(Rc::new(validator))
    }
    pub(super) fn check(&self, keys: &[String]) -> Result<(), String> {
        (self.0)(keys)
    }
}

/// A selection identity is independent of its caption and current row index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionItem {
    pub key: String,
    pub text: String,
    pub columns: Vec<String>,
    pub enabled: bool,
}
impl SelectionItem {
    pub fn new(key: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            text: text.into(),
            columns: Vec::new(),
            enabled: true,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionMode {
    #[default]
    Single,
    Multiple,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectionSettings {
    pub mode: SelectionMode,
    pub read_only: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComboBoxSettings {
    pub editable: bool,
    pub read_only: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct LookupColumn {
    pub header: String,
    /// Logical pixels; None shares the remaining width with other flexible columns.
    pub width: Option<f32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionChanged {
    pub id: WidgetId,
    pub keys: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListBox {
    pub container: WidgetId,
    pub viewport: WidgetId,
    pub content: WidgetId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorControl {
    pub container: WidgetId,
    pub editor: WidgetId,
    pub buttons: Vec<WidgetId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditorButtonAction {
    Command(CommandId),
    Clear,
}
#[derive(Clone, Debug, PartialEq)]
pub struct EditorButton {
    pub label: String,
    pub icon: Option<IconSymbol>,
    pub action: EditorButtonAction,
}

#[derive(Clone)]
pub(super) struct SelectionState {
    pub items: Vec<SelectionItem>,
    pub keys: Vec<String>,
    pub settings: SelectionSettings,
    pub rows: Vec<WidgetId>,
    pub row_start: usize,
    pub column_widths: Vec<Option<f32>>,
    pub parts: ListBox,
    pub cursor: Option<usize>,
    pub anchor: Option<usize>,
    pub typeahead: String,
    pub typeahead_at: Option<Instant>,
}
#[derive(Clone)]
pub(super) enum CompositeKind {
    Buttons,
    Combo {
        items: Vec<SelectionItem>,
        keys: Vec<String>,
        editable: bool,
        columns: Vec<LookupColumn>,
        lookup: bool,
    },
    Spin(SpinBoxSettings),
    Date(DateEditSettings),
    Time(TimeEditSettings),
}
#[derive(Clone)]
pub(super) struct Composite {
    pub parts: EditorControl,
    pub kind: CompositeKind,
    pub read_only: bool,
}
#[derive(Clone)]
pub(super) enum RichAction {
    Select {
        owner: WidgetId,
        key: String,
    },
    Button {
        owner: WidgetId,
        action: EditorButtonAction,
    },
    Popup(WidgetId),
    Step {
        owner: WidgetId,
        direction: i32,
    },
    Month {
        owner: WidgetId,
        direction: i32,
    },
    Date {
        owner: WidgetId,
        value: DateValue,
    },
}
pub(super) struct RichPopup {
    pub owner: WidgetId,
    pub surface: WidgetId,
    pub list: Option<WidgetId>,
    pub desired: Size,
    pub query: String,
    pub month: Option<DateValue>,
}
#[derive(Default)]
pub(super) struct RichControls {
    pub lists: HashMap<WidgetId, SelectionState>,
    pub composites: HashMap<WidgetId, Composite>,
    pub actions: HashMap<WidgetId, RichAction>,
    pub popup: Option<RichPopup>,
    pub changes: Vec<SelectionChanged>,
    pub values: Vec<ValueChanged>,
    pub modifiers: Modifiers,
    pub selection_validators: HashMap<WidgetId, SelectionValidator>,
    pub suppress_combo_commit: Option<WidgetId>,
    pub value_baselines: HashMap<WidgetId, EditorValue>,
}

fn validate_items(items: &[SelectionItem]) -> Result<(), ControlError> {
    if items.len() > 100_000 {
        return Err(ControlError::Invalid(
            "Selection supports at most 100000 items".into(),
        ));
    }
    let mut keys = HashSet::new();
    let mut bytes = 0usize;
    for item in items {
        bytes = bytes
            .saturating_add(item.key.len())
            .saturating_add(item.text.len());
        for column in &item.columns {
            bytes = bytes.saturating_add(column.len());
        }
        if item.key.is_empty() || !keys.insert(&item.key) || bytes > 16 * 1024 * 1024 {
            return Err(ControlError::Invalid(
                "Selection keys must be nonempty and unique; total text must not exceed 16 MiB"
                    .into(),
            ));
        }
    }
    Ok(())
}

impl Controls {
    pub fn list_box(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        items: Vec<SelectionItem>,
        settings: SelectionSettings,
        style: LayoutStyle,
    ) -> Result<ListBox, ControlError> {
        validate_items(&items)?;
        let scroll = self.scroll_view(parent, style, items.len() as f32 * 32.0 + 8.0)?;
        self.set_label(scroll.viewport, label)?;
        self.set_role(scroll.viewport, Role::ListBox);
        self.controls
            .get_mut(&scroll.content)
            .unwrap()
            .transparent_panel = true;
        self.edit_props(scroll.content, |p| {
            p.style.padding = Edges::all(4.0);
            p.style.gap = 0.0;
        })?;
        let parts = ListBox {
            container: scroll.viewport,
            viewport: scroll.viewport,
            content: scroll.content,
        };
        self.rich.lists.insert(
            parts.container,
            SelectionState {
                items,
                keys: Vec::new(),
                settings,
                rows: Vec::new(),
                row_start: 0,
                column_widths: Vec::new(),
                parts,
                cursor: None,
                anchor: None,
                typeahead: String::new(),
                typeahead_at: None,
            },
        );
        self.rebuild_selection_rows(parts.container)?;
        Ok(parts)
    }
    fn rebuild_selection_rows(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let state = self.rich.lists[&id].clone();
        let (start, end) = self.selection_visible_range(id);
        if self
            .runtime
            .focused()
            .is_some_and(|id| state.rows.contains(&id))
        {
            self.runtime.apply(Mutation::Focus(Some(id)))?;
        }
        for row in state.rows {
            if self.controls.contains_key(&row) {
                self.remove(row)?;
            }
        }
        let mut rows = Vec::with_capacity(end - start);
        for item in &state.items[start..end] {
            let row = self.button(
                state.parts.content,
                if item.columns.is_empty() {
                    item.text.as_str()
                } else {
                    ""
                },
                None,
                LayoutStyle {
                    kind: LayoutKind::Flex(Axis::Horizontal),
                    height: Length::Px(32.0),
                    flex_shrink: 0.0,
                    offset: Point::new(0.0, start as f32 * 32.0),
                    ..Default::default()
                },
            )?;
            self.set_automation_name(row, format!("{} {}", item.text, item.columns.join(" ")))?;
            self.set_role(row, Role::ListItem);
            self.edit_props(row, |p| p.tab_stop = false)?;
            self.set_enabled(row, item.enabled)?;
            if !item.columns.is_empty() {
                for (column, text) in std::iter::once(&item.text)
                    .chain(item.columns.iter())
                    .enumerate()
                {
                    let width = state.column_widths.get(column).copied().flatten();
                    self.label(
                        row,
                        text,
                        LayoutStyle {
                            width: Length::Px(width.unwrap_or(0.0)),
                            flex_grow: if width.is_some() { 0.0 } else { 1.0 },
                            flex_shrink: 0.0,
                            height: Length::Px(32.0),
                            ..Default::default()
                        },
                    )?;
                }
            }
            self.controls.get_mut(&row).unwrap().checked =
                if self.rich.popup.as_ref().is_some_and(|p| p.list == Some(id)) {
                    state
                        .cursor
                        .and_then(|i| state.items.get(i))
                        .is_some_and(|i| i.key == item.key)
                } else {
                    state.keys.contains(&item.key)
                };
            self.rich.actions.insert(
                row,
                RichAction::Select {
                    owner: id,
                    key: item.key.clone(),
                },
            );
            rows.push(row);
        }
        if let Some(current) = self.rich.lists.get_mut(&id) {
            current.rows = rows;
            current.row_start = start;
        }
        self.set_scroll_extent(id, state.items.len() as f32 * 32.0 + 8.0)?;
        self.scene_dirty = true;
        Ok(())
    }
    fn selection_visible_range(&self, id: WidgetId) -> (usize, usize) {
        let state = &self.rich.lists[&id];
        let scroll = self
            .runtime
            .tree()
            .node(id)
            .map_or(0.0, |n| n.props.scroll.y);
        let height = self.runtime.bounds(id).map_or(320.0, |r| r.height);
        let start = ((scroll / 32.0).max(0.0) as usize)
            .saturating_sub(2)
            .min(state.items.len());
        let end = start
            .saturating_add((height / 32.0).ceil() as usize)
            .saturating_add(5)
            .min(state.items.len());
        (start, end)
    }
    pub fn selection_items(&self, id: WidgetId) -> Option<&[SelectionItem]> {
        if let Some(list) = self.rich.lists.get(&id) {
            return Some(&list.items);
        }
        match &self.rich.composites.get(&id)?.kind {
            CompositeKind::Combo { items, .. } => Some(items),
            _ => None,
        }
    }
    pub fn selected_keys(&self, id: WidgetId) -> Option<&[String]> {
        if let Some(list) = self.rich.lists.get(&id) {
            return Some(&list.keys);
        }
        match &self.rich.composites.get(&id)?.kind {
            CompositeKind::Combo { keys, .. } => Some(keys),
            _ => None,
        }
    }
    pub fn drain_selection_changes(&mut self) -> Vec<SelectionChanged> {
        std::mem::take(&mut self.rich.changes)
    }
    pub fn chain_selection_validator(
        &mut self,
        id: WidgetId,
        validator: SelectionValidator,
    ) -> Result<(), ControlError> {
        self.selected_keys(id)
            .ok_or_else(|| ControlError::Invalid("Not a selection control".into()))?;
        let previous = self.rich.selection_validators.get(&id).cloned();
        self.rich.selection_validators.insert(
            id,
            SelectionValidator::new(move |keys| {
                if let Some(previous) = &previous {
                    previous.check(keys)?;
                }
                validator.check(keys)
            }),
        );
        Ok(())
    }
    pub fn set_selected_keys(
        &mut self,
        id: WidgetId,
        keys: &[String],
    ) -> Result<bool, ControlError> {
        let items = self
            .selection_items(id)
            .ok_or_else(|| ControlError::Invalid("Not a selection control".into()))?;
        let multiple = self
            .rich
            .lists
            .get(&id)
            .is_some_and(|s| s.settings.mode == SelectionMode::Multiple);
        let requested: HashSet<_> = keys.iter().collect();
        let available: HashSet<_> = items.iter().map(|i| &i.key).collect();
        if (!multiple && keys.len() > 1)
            || requested.len() != keys.len()
            || keys.iter().any(|key| !available.contains(key))
        {
            return Err(ControlError::Invalid(
                "Selection requires existing unique keys and a compatible selection mode".into(),
            ));
        }
        let canonical: Vec<_> = items
            .iter()
            .filter(|i| requested.contains(&i.key))
            .map(|i| i.key.clone())
            .collect();
        let text = items
            .iter()
            .find(|i| canonical.contains(&i.key))
            .map_or(String::new(), |i| i.text.clone());
        if self.selected_keys(id) == Some(canonical.as_slice()) {
            return Ok(false);
        }
        if let Some(validator) = self.rich.selection_validators.get(&id) {
            validator.check(&canonical).map_err(ControlError::Invalid)?;
        }
        if let Some(list) = self.rich.lists.get_mut(&id) {
            list.keys.clone_from(&canonical);
            list.cursor = list.items.iter().position(|i| canonical.contains(&i.key));
            for (row, item) in list.rows.iter().zip(list.items.iter().skip(list.row_start)) {
                if let Some(c) = self.controls.get_mut(row) {
                    c.checked = canonical.contains(&item.key);
                }
            }
        } else {
            let composite = self.rich.composites[&id].clone();
            // Selection is applied only after the editor's complete validation succeeds.
            self.rich.suppress_combo_commit = Some(id);
            let result = self.set_editor_value(composite.parts.editor, &text);
            self.rich.suppress_combo_commit = None;
            if let CommitOutcome::Invalid(message) = result? {
                return Err(ControlError::Invalid(message));
            }
            if let CompositeKind::Combo { keys, .. } =
                &mut self.rich.composites.get_mut(&id).unwrap().kind
            {
                keys.clone_from(&canonical);
            }
        }
        self.rich.changes.push(SelectionChanged {
            id,
            keys: canonical,
        });
        self.scene_dirty = true;
        Ok(true)
    }
    pub fn set_selection_items(
        &mut self,
        id: WidgetId,
        items: Vec<SelectionItem>,
    ) -> Result<(), ControlError> {
        validate_items(&items)?;
        if let Some(Composite {
            kind:
                CompositeKind::Combo {
                    columns,
                    lookup: true,
                    ..
                },
            ..
        }) = self.rich.composites.get(&id)
            && !columns.is_empty()
            && items.iter().any(|i| i.columns.len() + 1 != columns.len())
        {
            return Err(ControlError::Invalid(
                "Lookup row width must equal its column count".into(),
            ));
        }
        let previous = self
            .selected_keys(id)
            .ok_or_else(|| ControlError::Invalid("Not a selection control".into()))?
            .to_vec();
        let kept: Vec<_> = previous
            .iter()
            .filter(|key| items.iter().any(|i| &i.key == *key))
            .cloned()
            .collect();
        if previous != kept
            && let Some(validator) = self.rich.selection_validators.get(&id)
        {
            validator.check(&kept).map_err(ControlError::Invalid)?;
        }
        if let Some(list) = self.rich.lists.get_mut(&id) {
            list.items = items;
            list.keys.clone_from(&kept);
            list.cursor = list.items.iter().position(|i| kept.contains(&i.key));
            list.anchor = list.cursor;
            self.rebuild_selection_rows(id)?;
        } else {
            let previous_composite = self.rich.composites[&id].clone();
            let text = items.iter().find(|i| kept.contains(&i.key)).map_or_else(
                || {
                    if previous.is_empty() && kept.is_empty() {
                        self.accepted_value(previous_composite.parts.editor)
                            .unwrap_or_default()
                            .to_owned()
                    } else {
                        String::new()
                    }
                },
                |i| i.text.clone(),
            );
            if let CompositeKind::Combo { items: target, .. } =
                &mut self.rich.composites.get_mut(&id).unwrap().kind
            {
                *target = items;
            }
            self.rich.suppress_combo_commit = Some(id);
            let editor = previous_composite.parts.editor;
            let same_selection = previous == kept;
            let same_caption = self.accepted_value(editor) == Some(text.as_str());
            let preserve_document = self.document(editor).is_some_and(|d| {
                self.is_edit_dirty(editor) == Some(true) || d.composition().is_some()
            });
            let result = if same_selection && same_caption {
                Ok(CommitOutcome::Unchanged)
            } else if preserve_document {
                let limits = editor_limits(self.controls[&editor].editor.max_length);
                let preflight = normalize_editor_text(&text, true)
                    .map_err(edit_error)
                    .and_then(|value| limits.validate(&value).map_err(edit_error));
                match preflight {
                    Err(error) => Err(error),
                    Ok(()) => match self.validate_editor_value(editor, &text) {
                        Err(message) => Ok(CommitOutcome::Invalid(message)),
                        Ok(()) => {
                            self.controls
                                .get_mut(&editor)
                                .unwrap()
                                .accepted_value
                                .clone_from(&text);
                            self.edit_events.push(EditEvent::Committed {
                                id: editor,
                                value: text.clone(),
                            });
                            self.scene_dirty = true;
                            Ok(CommitOutcome::Committed)
                        }
                    },
                }
            } else {
                self.set_editor_value(editor, &text)
            };
            self.rich.suppress_combo_commit = None;
            match result {
                Ok(CommitOutcome::Invalid(message)) => {
                    self.rich.composites.insert(id, previous_composite);
                    return Err(ControlError::Invalid(message));
                }
                Err(error) => {
                    self.rich.composites.insert(id, previous_composite);
                    return Err(error);
                }
                _ => {}
            }
            if let CompositeKind::Combo { keys, .. } =
                &mut self.rich.composites.get_mut(&id).unwrap().kind
            {
                keys.clone_from(&kept);
            }
            if self
                .rich
                .popup
                .as_ref()
                .is_some_and(|popup| popup.owner == id)
            {
                if same_selection {
                    self.open_editor_popup(id)?;
                } else {
                    self.close_rich_popup(false)?;
                }
            }
        }
        if previous != kept {
            self.rich.changes.push(SelectionChanged { id, keys: kept });
        }
        Ok(())
    }
    pub fn selection_settings(&self, id: WidgetId) -> Option<&SelectionSettings> {
        self.rich.lists.get(&id).map(|s| &s.settings)
    }
    pub fn set_selection_settings(
        &mut self,
        id: WidgetId,
        settings: SelectionSettings,
    ) -> Result<(), ControlError> {
        let list = self
            .rich
            .lists
            .get_mut(&id)
            .ok_or_else(|| ControlError::Invalid("Not a ListBox".into()))?;
        if settings.mode == SelectionMode::Single && list.keys.len() > 1 {
            return Err(ControlError::Invalid(
                "Clear multiple selection before switching to Single".into(),
            ));
        }
        list.settings = settings;
        self.scene_dirty = true;
        Ok(())
    }
    pub fn composite_editor(&self, id: WidgetId) -> Option<WidgetId> {
        self.rich.composites.get(&id).map(|c| c.parts.editor)
    }
    pub fn editor_control_parts(&self, id: WidgetId) -> Option<&EditorControl> {
        self.rich.composites.get(&id).map(|c| &c.parts)
    }
    pub fn set_composite_read_only(
        &mut self,
        id: WidgetId,
        value: bool,
    ) -> Result<(), ControlError> {
        let c = self
            .rich
            .composites
            .get_mut(&id)
            .ok_or_else(|| ControlError::Invalid("Not a composite editor".into()))?;
        c.read_only = value;
        let editor = c.parts.editor;
        let text_read_only = value
            || matches!(
                c.kind,
                CompositeKind::Combo {
                    editable: false,
                    ..
                }
            );
        self.set_read_only(editor, text_read_only)?;
        if value && self.rich.popup.as_ref().is_some_and(|p| p.owner == id) {
            self.close_rich_popup(false)?;
        }
        self.scene_dirty = true;
        Ok(())
    }
    pub fn combo_box(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        items: Vec<SelectionItem>,
        settings: ComboBoxSettings,
        style: LayoutStyle,
    ) -> Result<EditorControl, ControlError> {
        self.create_combo(parent, label.into(), items, settings, Vec::new(), style)
    }
    pub fn lookup_box(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        items: Vec<SelectionItem>,
        columns: Vec<String>,
        style: LayoutStyle,
    ) -> Result<EditorControl, ControlError> {
        if columns.len() > 32 || columns.iter().any(|c| c.len() > 4096) {
            return Err(ControlError::Invalid(
                "Lookup supports at most 32 bounded column captions".into(),
            ));
        }
        let parts = self.create_combo(
            parent,
            label.into(),
            items,
            ComboBoxSettings {
                editable: true,
                read_only: false,
            },
            columns,
            style,
        )?;
        if let CompositeKind::Combo { lookup, .. } =
            &mut self.rich.composites.get_mut(&parts.container).unwrap().kind
        {
            *lookup = true;
        }
        Ok(parts)
    }
    fn create_combo(
        &mut self,
        parent: WidgetId,
        label: String,
        items: Vec<SelectionItem>,
        settings: ComboBoxSettings,
        columns: Vec<String>,
        style: LayoutStyle,
    ) -> Result<EditorControl, ControlError> {
        validate_items(&items)?;
        if !columns.is_empty() && items.iter().any(|i| i.columns.len() + 1 != columns.len()) {
            return Err(ControlError::Invalid(
                "Lookup row width must equal its column count".into(),
            ));
        }
        let mut parts = self.create_editor_parts(parent, label, "", style)?;
        let button =
            self.add_editor_button(&parts, "Открыть список", Some(IconSymbol::ChevronDown))?;
        parts.buttons.push(button);
        self.rich
            .actions
            .insert(button, RichAction::Popup(parts.container));
        self.set_role(parts.container, Role::ComboBox);
        self.rich.composites.insert(
            parts.container,
            Composite {
                parts: parts.clone(),
                kind: CompositeKind::Combo {
                    items,
                    keys: Vec::new(),
                    editable: settings.editable,
                    columns: columns
                        .into_iter()
                        .map(|header| LookupColumn {
                            header,
                            width: None,
                        })
                        .collect(),
                    lookup: false,
                },
                read_only: settings.read_only,
            },
        );
        self.set_composite_read_only(parts.container, settings.read_only)?;
        Ok(parts)
    }
    pub fn combo_settings(&self, id: WidgetId) -> Option<ComboBoxSettings> {
        let c = self.rich.composites.get(&id)?;
        match &c.kind {
            CompositeKind::Combo { editable, .. } => Some(ComboBoxSettings {
                editable: *editable,
                read_only: c.read_only,
            }),
            _ => None,
        }
    }
    pub fn set_combo_settings(
        &mut self,
        id: WidgetId,
        settings: ComboBoxSettings,
    ) -> Result<(), ControlError> {
        let c = self
            .rich
            .composites
            .get(&id)
            .ok_or_else(|| ControlError::Invalid("Not a ComboBox or LookupEdit".into()))?;
        let CompositeKind::Combo { items, .. } = &c.kind else {
            return Err(ControlError::Invalid("Not a ComboBox or LookupEdit".into()));
        };
        if !settings.editable
            && self
                .accepted_value(c.parts.editor)
                .is_some_and(|text| !text.is_empty() && !items.iter().any(|i| i.text == text))
        {
            return Err(ControlError::Invalid(
                "Select a listed value before disabling text editing".into(),
            ));
        }
        if let CompositeKind::Combo { editable, .. } =
            &mut self.rich.composites.get_mut(&id).unwrap().kind
        {
            *editable = settings.editable;
        }
        self.set_composite_read_only(id, settings.read_only)?;
        self.close_rich_popup(false)?;
        Ok(())
    }
    pub fn lookup_columns(&self, id: WidgetId) -> Option<&[LookupColumn]> {
        match &self.rich.composites.get(&id)?.kind {
            CompositeKind::Combo {
                columns,
                lookup: true,
                ..
            } => Some(columns),
            _ => None,
        }
    }
    pub fn set_lookup_columns(
        &mut self,
        id: WidgetId,
        columns: Vec<LookupColumn>,
    ) -> Result<(), ControlError> {
        let c = self
            .rich
            .composites
            .get(&id)
            .ok_or_else(|| ControlError::Invalid("Not a LookupEdit".into()))?;
        let CompositeKind::Combo {
            items,
            lookup: true,
            ..
        } = &c.kind
        else {
            return Err(ControlError::Invalid("Not a LookupEdit".into()));
        };
        if columns.len() > 32
            || columns.iter().any(|c| {
                c.header.len() > 4096
                    || c.width
                        .is_some_and(|w| !w.is_finite() || w <= 0.0 || w > 16_384.0)
            })
            || (!columns.is_empty() && items.iter().any(|i| i.columns.len() + 1 != columns.len()))
        {
            return Err(ControlError::Invalid("Lookup columns require matching row widths, bounded headers and finite positive widths <= 16384".into()));
        }
        if let CompositeKind::Combo {
            columns: target, ..
        } = &mut self.rich.composites.get_mut(&id).unwrap().kind
        {
            *target = columns;
        }
        self.close_rich_popup(false)?;
        Ok(())
    }
    pub fn button_edit(
        &mut self,
        parent: WidgetId,
        label: impl Into<String>,
        text: &str,
        buttons: Vec<EditorButton>,
        style: LayoutStyle,
    ) -> Result<EditorControl, ControlError> {
        if buttons.len() > 8 || buttons.iter().any(|b| b.label.len() > 4096 || matches!(&b.action, EditorButtonAction::Command(c) if self.commands.label(c).is_none())) { return Err(ControlError::Invalid("ButtonEdit requires at most 8 bounded buttons with registered commands".into())); }
        let mut parts = self.create_editor_parts(parent, label.into(), text, style)?;
        for button in buttons {
            let id = self.add_editor_button(&parts, &button.label, button.icon)?;
            if let EditorButtonAction::Command(command) = &button.action {
                self.controls.get_mut(&id).unwrap().command = Some(command.clone());
            }
            self.rich.actions.insert(
                id,
                RichAction::Button {
                    owner: parts.container,
                    action: button.action,
                },
            );
            parts.buttons.push(id);
        }
        self.rich.composites.insert(
            parts.container,
            Composite {
                parts: parts.clone(),
                kind: CompositeKind::Buttons,
                read_only: false,
            },
        );
        Ok(parts)
    }
    pub(super) fn create_editor_parts(
        &mut self,
        parent: WidgetId,
        label: String,
        text: &str,
        mut style: LayoutStyle,
    ) -> Result<EditorControl, ControlError> {
        style.kind = LayoutKind::Flex(Axis::Horizontal);
        style.gap = 0.0;
        if style.height == Length::Auto {
            style.height = Length::Px(self.theme.tokens().control_height);
        }
        let container = self.transparent_panel(parent, style)?;
        self.set_automation_name(container, &label)?;
        let editor = self.edit_box(
            container,
            label,
            text,
            LayoutStyle {
                width: Length::Px(0.0),
                flex_grow: 1.0,
                height: Length::Percent(1.0),
                ..Default::default()
            },
        )?;
        Ok(EditorControl {
            container,
            editor,
            buttons: Vec::new(),
        })
    }
    pub(super) fn add_editor_button(
        &mut self,
        parts: &EditorControl,
        label: &str,
        icon: Option<IconSymbol>,
    ) -> Result<WidgetId, ControlError> {
        let id = self.button(
            parts.container,
            if icon.is_some() { "" } else { label },
            None,
            LayoutStyle {
                width: Length::Px(30.0),
                height: Length::Percent(1.0),
                flex_shrink: 0.0,
                ..Default::default()
            },
        )?;
        self.set_automation_name(id, label)?;
        self.edit_props(id, |p| p.tab_stop = false)?;
        self.set_button_icon(id, icon.map(ButtonIcon::new))?;
        self.set_button_chrome(id, ButtonChrome::Flat)?;
        Ok(id)
    }
    pub(super) fn preflight_rich_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.rich.composites.values().any(|c| c.parts.editor == id)
            || self
                .rich
                .lists
                .values()
                .any(|list| list.parts.content == id)
        {
            return Err(ControlError::Invalid("Required composite parts cannot be removed individually; remove their container or ancestor".into()));
        }
        Ok(())
    }
    pub(super) fn retain_rich_controls(&mut self) {
        self.rich.lists.retain(|id, list| {
            self.controls.contains_key(id) && self.controls.contains_key(&list.parts.content)
        });
        self.rich.composites.retain(|id, composite| {
            self.controls.contains_key(id) && self.controls.contains_key(&composite.parts.editor)
        });
        for composite in self.rich.composites.values_mut() {
            composite
                .parts
                .buttons
                .retain(|id| self.controls.contains_key(id));
        }
        for list in self.rich.lists.values_mut() {
            list.rows.retain(|id| self.controls.contains_key(id));
        }
        self.rich.actions.retain(|id, action| {
            self.controls.contains_key(id)
                && match action {
                    RichAction::Select { owner, .. } => self.rich.lists.contains_key(owner),
                    RichAction::Button { owner, .. }
                    | RichAction::Popup(owner)
                    | RichAction::Step { owner, .. }
                    | RichAction::Month { owner, .. }
                    | RichAction::Date { owner, .. } => self.rich.composites.contains_key(owner),
                }
        });
        self.rich
            .changes
            .retain(|e| self.controls.contains_key(&e.id));
        self.rich
            .values
            .retain(|e| self.controls.contains_key(&e.id));
        self.rich
            .selection_validators
            .retain(|id, _| self.controls.contains_key(id));
        self.rich
            .value_baselines
            .retain(|id, _| self.controls.contains_key(id));
    }
    pub(super) fn rich_owner(&self, id: WidgetId) -> Option<WidgetId> {
        if self.rich.composites.contains_key(&id) {
            return Some(id);
        }
        self.rich.composites.iter().find_map(|(&owner, c)| {
            (c.parts.editor == id || c.parts.buttons.contains(&id)).then_some(owner)
        })
    }
    pub(super) fn rich_focus_editor(&self, id: WidgetId) -> Option<WidgetId> {
        if let Some(owner) = self.rich_owner(id) {
            return self.composite_editor(owner);
        }
        if let Some(popup) = &self.rich.popup {
            let mut current = Some(id);
            while let Some(node) = current {
                if node == popup.surface {
                    return self.composite_editor(popup.owner);
                }
                current = self.runtime.tree().parent(node);
            }
        }
        None
    }
    pub(super) fn rich_read_only(&self, id: WidgetId) -> bool {
        if let Some(list) = self.rich.lists.get(&id) {
            return list.settings.read_only;
        }
        if let Some(owner) = self.rich_owner(id) {
            return self.rich.composites[&owner].read_only;
        }
        if let Some(RichAction::Select { owner, .. }) = self.rich.actions.get(&id) {
            return self
                .rich
                .lists
                .get(owner)
                .is_some_and(|l| l.settings.read_only);
        }
        false
    }
    pub(super) fn rich_editable(&self, owner: WidgetId) -> bool {
        self.is_enabled(owner)
            && self
                .rich
                .composites
                .get(&owner)
                .is_some_and(|c| !c.read_only)
    }
    pub(super) fn activate_rich_control(&mut self, id: WidgetId) -> Result<bool, ControlError> {
        let Some(action) = self.rich.actions.get(&id).cloned() else {
            return Ok(false);
        };
        match action {
            RichAction::Select { owner, key } => {
                if let Some(popup_owner) = self
                    .rich
                    .popup
                    .as_ref()
                    .filter(|p| p.list == Some(owner))
                    .map(|p| p.owner)
                {
                    if self.rich_editable(popup_owner)
                        && self.user_set_selection(popup_owner, &[key])?
                    {
                        self.close_rich_popup(true)?;
                    }
                } else {
                    self.user_select(owner, &key, self.rich.modifiers)?;
                }
            }
            RichAction::Popup(owner) => {
                if self.rich.popup.as_ref().is_some_and(|p| p.owner == owner) {
                    self.close_rich_popup(true)?;
                } else {
                    self.open_editor_popup(owner)?;
                }
            }
            RichAction::Button { owner, action } => {
                if self.rich_editable(owner) {
                    match action {
                        EditorButtonAction::Command(command) => {
                            if self.commands.activate(&command) {
                                self.activated.push(id);
                            }
                        }
                        EditorButtonAction::Clear => {
                            let editor = self.rich.composites[&owner].parts.editor;
                            self.set_text(editor, "")?;
                            self.commit_edit(editor)?;
                        }
                    }
                }
            }
            RichAction::Step { owner, direction } => {
                self.step_value(owner, direction)?;
            }
            RichAction::Month { owner, direction } => {
                self.change_calendar_month(owner, direction)?;
            }
            RichAction::Date { owner, value } => {
                if self.rich_editable(owner) && self.user_set_date_value(owner, value)? {
                    self.close_rich_popup(true)?;
                }
            }
        }
        self.scene_dirty = true;
        Ok(true)
    }
    fn user_select(
        &mut self,
        owner: WidgetId,
        key: &str,
        modifiers: Modifiers,
    ) -> Result<(), ControlError> {
        let state = self.rich.lists[&owner].clone();
        if state.settings.read_only || !self.is_enabled(owner) {
            return Ok(());
        }
        let Some(index) = state.items.iter().position(|i| i.key == key && i.enabled) else {
            return Ok(());
        };
        let mut keys = if state.settings.mode == SelectionMode::Multiple
            && (modifiers.control || modifiers.shift)
        {
            state.keys
        } else {
            Vec::new()
        };
        if state.settings.mode == SelectionMode::Multiple && modifiers.shift {
            let anchor = state.anchor.unwrap_or(index);
            for item in &state.items[anchor.min(index)..=anchor.max(index)] {
                if item.enabled && !keys.contains(&item.key) {
                    keys.push(item.key.clone());
                }
            }
        } else if let Some(position) = keys.iter().position(|k| k == key) {
            keys.remove(position);
        } else {
            keys.push(key.to_owned());
        }
        if !self.user_set_selection(owner, &keys)? {
            return Ok(());
        }
        let list = self.rich.lists.get_mut(&owner).unwrap();
        list.cursor = Some(index);
        if !modifiers.shift {
            list.anchor = Some(index);
        }
        self.ensure_selection_visible(owner, index)?;
        Ok(())
    }
    fn user_set_selection(
        &mut self,
        owner: WidgetId,
        keys: &[String],
    ) -> Result<bool, ControlError> {
        match self.set_selected_keys(owner, keys) {
            Ok(_) => {
                if let Some(c) = self.controls.get_mut(&owner) {
                    c.validation_error = None;
                }
                Ok(true)
            }
            Err(ControlError::Invalid(message)) => {
                if let Some(c) = self.controls.get_mut(&owner) {
                    c.validation_error = Some(message);
                }
                self.scene_dirty = true;
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }
    fn ensure_selection_visible(
        &mut self,
        owner: WidgetId,
        index: usize,
    ) -> Result<(), ControlError> {
        let Some(viewport) = self.runtime.bounds(owner) else {
            return Ok(());
        };
        let current = self.runtime.tree().node(owner).unwrap().props.scroll.y;
        let top = index as f32 * 32.0 + 4.0;
        if top < current {
            self.scroll_to(owner, top)?;
        } else if top + 32.0 > current + viewport.height {
            self.scroll_to(owner, top + 32.0 - viewport.height)?;
        }
        Ok(())
    }
    pub(super) fn rich_before_input(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        if let InputEvent::PointerDown {
            position,
            modifiers,
            ..
        } = event
        {
            self.rich.modifiers = *modifiers;
            if let Some(popup) = &self.rich.popup {
                let inside = self
                    .runtime
                    .bounds(popup.surface)
                    .is_some_and(|r| r.contains(position.x, position.y))
                    || self
                        .runtime
                        .bounds(popup.owner)
                        .is_some_and(|r| r.contains(position.x, position.y));
                if !inside {
                    self.close_rich_popup(false)?;
                }
            }
        }
        if matches!(event, InputEvent::WindowFocused(false)) {
            self.close_rich_popup(false)?;
        }
        let InputEvent::KeyDown {
            key,
            modifiers,
            repeat,
        } = event
        else {
            return Ok(false);
        };
        if *key == Key::Escape && self.rich.popup.is_some() && !repeat {
            self.close_rich_popup(true)?;
            return Ok(true);
        }
        if *key == Key::Tab && self.rich.popup.is_some() {
            self.close_rich_popup(true)?;
        }
        let Some(focused) = self.runtime.focused() else {
            return Ok(false);
        };
        if self
            .document(focused)
            .is_some_and(|d| d.composition().is_some())
        {
            return Ok(false);
        }
        if !modifiers.control
            && !modifiers.alt
            && !modifiers.super_key
            && self.rich.popup.as_ref().is_some_and(|p| p.month.is_some())
            && self.rich_focus_editor(focused).is_some()
            && self.calendar_key(key)?
        {
            return Ok(true);
        }
        let owner = self.rich_owner(focused).or_else(|| {
            self.rich_focus_editor(focused)
                .and_then(|editor| self.rich_owner(editor))
        });
        if let Some(owner) = owner {
            if let Some(popup_list) = self
                .rich
                .popup
                .as_ref()
                .filter(|p| p.owner == owner)
                .and_then(|p| p.list)
            {
                if matches!(
                    key,
                    Key::ArrowDown
                        | Key::ArrowUp
                        | Key::Home
                        | Key::End
                        | Key::PageDown
                        | Key::PageUp
                ) {
                    self.selection_key(popup_list, key, *modifiers, true)?;
                    return Ok(true);
                }
                if matches!(key, Key::Character(_))
                    && matches!(
                        self.rich.composites[&owner].kind,
                        CompositeKind::Combo {
                            editable: false,
                            ..
                        }
                    )
                {
                    return self.selection_key(popup_list, key, *modifiers, true);
                }
                if *key == Key::Enter && !repeat {
                    let s = &self.rich.lists[&popup_list];
                    if let Some(item) = s.cursor.and_then(|i| s.items.get(i)).filter(|i| i.enabled)
                    {
                        let key = item.key.clone();
                        if !self.user_set_selection(owner, &[key])? {
                            return Ok(true);
                        }
                    }
                    self.close_rich_popup(true)?;
                    return Ok(true);
                }
            }
            if matches!(key, Key::ArrowDown | Key::ArrowUp)
                && !modifiers.control
                && !modifiers.super_key
            {
                match self.rich.composites[&owner].kind {
                    CompositeKind::Combo { .. } => {
                        self.open_editor_popup(owner)?;
                        return Ok(true);
                    }
                    CompositeKind::Date(_) if modifiers.alt => {
                        self.open_editor_popup(owner)?;
                        return Ok(true);
                    }
                    CompositeKind::Spin(_) | CompositeKind::Time(_) => {
                        self.step_value(owner, if *key == Key::ArrowUp { 1 } else { -1 })?;
                        return Ok(true);
                    }
                    _ => {}
                }
            }
        }
        let list = if self.rich.lists.contains_key(&focused) {
            Some(focused)
        } else {
            self.rich.actions.get(&focused).and_then(|a| {
                if let RichAction::Select { owner, .. } = a {
                    Some(*owner)
                } else {
                    None
                }
            })
        };
        if let Some(list) = list {
            return self.selection_key(list, key, *modifiers, false);
        }
        Ok(false)
    }
    fn selection_key(
        &mut self,
        id: WidgetId,
        key: &Key,
        modifiers: Modifiers,
        preview: bool,
    ) -> Result<bool, ControlError> {
        if !self.is_enabled(id) {
            return Ok(false);
        }
        let state = self.rich.lists[&id].clone();
        let enabled: Vec<_> = state
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| item.enabled.then_some(i))
            .collect();
        if enabled.is_empty() {
            return Ok(false);
        }
        let pos = state
            .cursor
            .and_then(|i| enabled.iter().position(|n| *n == i));
        let page = self
            .runtime
            .bounds(id)
            .map_or(5, |r| (r.height / 32.0).max(1.0) as usize);
        let index = match key {
            Key::ArrowDown => enabled[pos.map_or(0, |p| (p + 1).min(enabled.len() - 1))],
            Key::ArrowUp => enabled[pos.map_or(0, |p| p.saturating_sub(1))],
            Key::Home => enabled[0],
            Key::End => *enabled.last().unwrap(),
            Key::PageDown => enabled[pos.map_or(0, |p| (p + page).min(enabled.len() - 1))],
            Key::PageUp => enabled[pos.map_or(0, |p| p.saturating_sub(page))],
            Key::Space | Key::Enter => state.cursor.unwrap_or(enabled[0]),
            Key::Character(text)
                if !modifiers.alt && !modifiers.super_key && !modifiers.control =>
            {
                let now = Instant::now();
                let list = self.rich.lists.get_mut(&id).unwrap();
                if list
                    .typeahead_at
                    .is_none_or(|previous| now.duration_since(previous) > Duration::from_secs(1))
                {
                    list.typeahead.clear();
                }
                list.typeahead.push_str(&text.to_lowercase());
                list.typeahead_at = Some(now);
                let query = list.typeahead.clone();
                let Some(index) = enabled
                    .iter()
                    .copied()
                    .find(|i| state.items[*i].text.to_lowercase().starts_with(&query))
                else {
                    return Ok(true);
                };
                index
            }
            Key::Character(text)
                if modifiers.control
                    && text.eq_ignore_ascii_case("a")
                    && state.settings.mode == SelectionMode::Multiple =>
            {
                if !state.settings.read_only {
                    let keys = state
                        .items
                        .iter()
                        .filter(|i| i.enabled)
                        .map(|i| i.key.clone())
                        .collect::<Vec<_>>();
                    self.user_set_selection(id, &keys)?;
                }
                return Ok(true);
            }
            _ => return Ok(false),
        };
        self.rich.lists.get_mut(&id).unwrap().cursor = Some(index);
        if preview || (modifiers.control && !matches!(key, Key::Space | Key::Enter)) {
            for (row, item) in state
                .rows
                .iter()
                .zip(state.items.iter().skip(state.row_start))
            {
                if let Some(c) = self.controls.get_mut(row) {
                    c.checked = if preview {
                        item.key == state.items[index].key
                    } else {
                        state.keys.contains(&item.key)
                    };
                }
            }
            self.ensure_selection_visible(id, index)?;
        } else {
            self.user_select(id, &state.items[index].key, modifiers)?;
        }
        self.scene_dirty = true;
        Ok(true)
    }
    pub fn open_editor_popup(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        let composite = self
            .rich
            .composites
            .get(&owner)
            .cloned()
            .ok_or_else(|| ControlError::Invalid("Not a composite editor".into()))?;
        if !self.rich_editable(owner) {
            return Ok(());
        }
        self.close_popup_from(0, popup::CloseReason::Restore)?;
        self.close_rich_popup(false)?;
        match composite.kind {
            CompositeKind::Combo {
                items,
                keys,
                editable,
                columns,
                ..
            } => {
                let query = if editable && self.is_edit_dirty(composite.parts.editor) == Some(true)
                {
                    self.document(composite.parts.editor)
                        .unwrap()
                        .text()
                        .to_owned()
                } else {
                    String::new()
                };
                let filtered = filter_items(&items, &query);
                let header_height = if columns.is_empty() { 0.0 } else { 32.0 };
                let desired = Size::new(
                    self.runtime
                        .bounds(owner)
                        .map_or(240.0, |r| r.width)
                        .max(if columns.len() > 1 { 420.0 } else { 180.0 })
                        .max(columns.iter().map(|c| c.width.unwrap_or(80.0)).sum::<f32>() + 8.0),
                    (filtered.len() as f32 * 32.0 + 8.0).clamp(40.0, 264.0) + header_height,
                );
                let surface = self.panel(
                    self.overlay,
                    LayoutStyle {
                        width: Length::Px(desired.width),
                        height: Length::Px(desired.height),
                        gap: 0.0,
                        ..Default::default()
                    },
                )?;
                self.controls.get_mut(&surface).unwrap().popup_surface = true;
                if !columns.is_empty() {
                    let header = self.transparent_panel(
                        surface,
                        LayoutStyle {
                            kind: LayoutKind::Flex(Axis::Horizontal),
                            height: Length::Px(32.0),
                            flex_shrink: 0.0,
                            padding: Edges {
                                left: 4.0,
                                right: 4.0,
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                    )?;
                    for column in &columns {
                        self.label(
                            header,
                            &column.header,
                            LayoutStyle {
                                width: Length::Px(column.width.unwrap_or(0.0)),
                                flex_grow: if column.width.is_some() { 0.0 } else { 1.0 },
                                flex_shrink: 0.0,
                                height: Length::Px(32.0),
                                ..Default::default()
                            },
                        )?;
                    }
                }
                let list = self.list_box(
                    surface,
                    "Варианты",
                    filtered,
                    SelectionSettings::default(),
                    LayoutStyle {
                        height: Length::Px(desired.height - header_height),
                        ..Default::default()
                    },
                )?;
                if !columns.is_empty() {
                    self.rich
                        .lists
                        .get_mut(&list.container)
                        .unwrap()
                        .column_widths = columns.iter().map(|c| c.width).collect();
                    self.rebuild_selection_rows(list.container)?;
                }
                let keys: Vec<_> = keys
                    .into_iter()
                    .filter(|key| {
                        self.rich.lists[&list.container]
                            .items
                            .iter()
                            .any(|i| &i.key == key)
                    })
                    .collect();
                self.set_selected_keys(list.container, &keys)?;
                if self.rich.lists[&list.container].cursor.is_none() {
                    let first = self.rich.lists[&list.container]
                        .items
                        .iter()
                        .position(|i| i.enabled);
                    self.rich.lists.get_mut(&list.container).unwrap().cursor = first;
                }
                self.rich.changes.retain(|e| e.id != list.container);
                self.rich.popup = Some(RichPopup {
                    owner,
                    surface,
                    list: Some(list.container),
                    desired,
                    query,
                    month: None,
                });
                let state = &self.rich.lists[&list.container];
                for (row, item) in state
                    .rows
                    .iter()
                    .zip(state.items.iter().skip(state.row_start))
                {
                    if let Some(control) = self.controls.get_mut(row) {
                        control.checked = state
                            .cursor
                            .and_then(|i| state.items.get(i))
                            .is_some_and(|i| i.key == item.key);
                    }
                }
            }
            CompositeKind::Date(settings) => {
                let month = self
                    .date_value(owner)
                    .flatten()
                    .unwrap_or_else(DateValue::today_utc)
                    .max(settings.minimum.unwrap_or(DateValue {
                        year: 1,
                        month: 1,
                        day: 1,
                    }))
                    .min(settings.maximum.unwrap_or(DateValue {
                        year: 9999,
                        month: 12,
                        day: 31,
                    }));
                self.build_calendar_popup(owner, month)?;
            }
            _ => return Err(ControlError::Invalid("This editor has no popup".into())),
        }
        self.runtime
            .apply(Mutation::Focus(Some(composite.parts.editor)))?;
        self.scene_dirty = true;
        Ok(())
    }
    pub(super) fn close_rich_popup(&mut self, restore_focus: bool) -> Result<(), ControlError> {
        if let Some(popup) = self.rich.popup.take() {
            if self.controls.contains_key(&popup.surface) {
                self.remove(popup.surface)?;
            }
            if restore_focus
                && self.is_enabled(popup.owner)
                && let Some(editor) = self.composite_editor(popup.owner)
            {
                self.runtime.apply(Mutation::Focus(Some(editor)))?;
            }
            self.scene_dirty = true;
        }
        Ok(())
    }
    pub(super) fn rich_popup_expanded(&self, id: WidgetId) -> Option<bool> {
        let owner = self.rich_owner(id)?;
        matches!(
            self.rich.composites[&owner].kind,
            CompositeKind::Combo { .. } | CompositeKind::Date(_)
        )
        .then(|| self.rich.popup.as_ref().is_some_and(|p| p.owner == owner))
    }
    pub(super) fn sync_rich_layout(&mut self) -> Result<FrameStats, ControlError> {
        let mut delta = FrameStats::default();
        let rebuild: Vec<_> = self
            .rich
            .lists
            .iter()
            .filter_map(|(&id, state)| {
                let (start, end) = self.selection_visible_range(id);
                (state.row_start != start || state.rows.len() != end - start).then_some(id)
            })
            .collect();
        for id in rebuild {
            self.rebuild_selection_rows(id)?;
        }
        if self.runtime.needs_update() {
            add_stats(
                &mut delta,
                self.runtime.update(&mut self.layout, self.viewport)?,
            );
        }
        let Some(popup) = &self.rich.popup else {
            return Ok(delta);
        };
        let owner = popup.owner;
        if !self.rich_editable(owner) || self.runtime.bounds(owner).is_none() {
            self.close_rich_popup(false)?;
            return Ok(delta);
        }
        if self
            .runtime
            .focused()
            .is_some_and(|id| self.rich_focus_editor(id) != self.composite_editor(owner))
        {
            self.close_rich_popup(false)?;
            return Ok(delta);
        }
        if let Some(c) = self.rich.composites.get(&owner)
            && matches!(c.kind, CompositeKind::Combo { editable: true, .. })
        {
            let query = if self.is_edit_dirty(c.parts.editor) == Some(true) {
                self.document(c.parts.editor).unwrap().text().to_owned()
            } else {
                String::new()
            };
            if query != self.rich.popup.as_ref().unwrap().query {
                self.open_editor_popup(owner)?;
            }
        }
        let popup = self.rich.popup.as_ref().unwrap();
        let bounds = popup::place_overlay(
            self.runtime.bounds(owner).unwrap(),
            popup.desired,
            self.viewport,
            false,
        );
        let surface = popup.surface;
        let mut style = self
            .runtime
            .tree()
            .node(surface)
            .unwrap()
            .props
            .style
            .clone();
        style.width = Length::Px(bounds.width);
        style.height = Length::Px(bounds.height);
        style.offset = Point::new(bounds.x, bounds.y);
        if self.runtime.tree().node(surface).unwrap().props.style != style {
            self.runtime.apply(Mutation::SetStyle {
                node: surface,
                style,
            })?;
        }
        if self.runtime.needs_update() {
            add_stats(
                &mut delta,
                self.runtime.update(&mut self.layout, self.viewport)?,
            );
        }
        Ok(delta)
    }
}

fn filter_items(items: &[SelectionItem], query: &str) -> Vec<SelectionItem> {
    let query = query.to_lowercase();
    items
        .iter()
        .filter(|i| {
            i.text.to_lowercase().contains(&query)
                || i.columns.iter().any(|c| c.to_lowercase().contains(&query))
        })
        .cloned()
        .collect()
}
