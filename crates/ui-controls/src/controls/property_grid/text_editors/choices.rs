use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChoiceView {
    Radio,
    List,
    Lookup,
    Search,
    Complete,
    Suggest,
}

pub(super) struct Choices {
    owner: WidgetId,
    focus: WidgetId,
    view: ChoiceView,
    items: Vec<SelectionItem>,
    integer: bool,
    last_query: String,
    popup_was_open: bool,
}

impl Choices {
    pub(super) fn mount(
        ui: &mut Controls,
        host: WidgetId,
        label: &str,
        d: &PropertyDescriptor,
        f: &FieldSchema,
        value: &CellValue,
    ) -> Result<Box<dyn InspectorEditor>, ControlError> {
        let view = match d.metadata.editor.as_str() {
            "ListBox" if d.choices.len() <= 8 => ChoiceView::Radio,
            "ListBox" => ChoiceView::List,
            "AutoComplete" => ChoiceView::Complete,
            "AutoSuggest" => ChoiceView::Suggest,
            "SearchLookup" => ChoiceView::Search,
            _ => ChoiceView::Lookup,
        };
        let items: Vec<_> = keys(d)
            .iter()
            .zip(&d.choices)
            .map(|(key, caption)| {
                let mut item = SelectionItem::new(key, caption);
                if matches!(view, ChoiceView::Lookup | ChoiceView::Search) {
                    item.columns.push(key.clone());
                }
                item
            })
            .collect();
        let (owner, focus) = match view {
            ChoiceView::Radio => {
                let parts = ui.radio_choice_group(
                    host,
                    items.clone(),
                    as_key(value)?.as_deref(),
                    LayoutStyle {
                        kind: LayoutKind::Wrap(Axis::Horizontal),
                        gap: 2.,
                        ..Default::default()
                    },
                )?;
                ui.set_automation_name(parts.container, label)?;
                for item in &items {
                    let id = ui.radio_item_widget(parts.container, &item.key).unwrap();
                    ui.set_automation_name(id, &item.text)?;
                    let width = (ui.measure_text(&item.text, 512., false)?.width + 40.).ceil();
                    ui.edit_props(id, |n| {
                        n.style.width = Length::Px(width);
                        n.style.height = Length::Px(28.)
                    })?;
                }
                let first = ui
                    .radio_item_widget(parts.container, &items[0].key)
                    .unwrap();
                (parts.container, first)
            }
            ChoiceView::List => {
                let list = ui.list_box(
                    host,
                    label,
                    items.clone(),
                    SelectionSettings {
                        read_only: d.metadata.read_only,
                        ..Default::default()
                    },
                    LayoutStyle {
                        height: Length::Px(112.),
                        ..Default::default()
                    },
                )?;
                (list.container, list.container)
            }
            ChoiceView::Suggest => {
                let parts = ui.combo_box(
                    host,
                    label,
                    items.clone(),
                    ComboBoxSettings {
                        editable: true,
                        read_only: d.metadata.read_only,
                    },
                    layout(),
                )?;
                (parts.container, parts.editor)
            }
            _ => {
                let headings = if view == ChoiceView::Complete {
                    Vec::new()
                } else {
                    vec!["Значение".into(), "Ключ".into()]
                };
                let parts = ui.lookup_box(host, label, items.clone(), headings, layout())?;
                ui.set_lookup_search_settings(
                    parts.container,
                    LookupSearchSettings {
                        mode: if view == ChoiceView::Complete {
                            LookupSearchMode::StartsWith
                        } else {
                            LookupSearchMode::Contains
                        },
                        columns: if view == ChoiceView::Lookup {
                            vec![0]
                        } else {
                            Vec::new()
                        },
                        ..Default::default()
                    },
                )?;
                ui.set_composite_read_only(parts.container, d.metadata.read_only)?;
                (parts.container, parts.editor)
            }
        };
        let editor = Self {
            owner,
            focus,
            view,
            items,
            integer: f.kind == FieldType::Integer,
            last_query: String::new(),
            popup_was_open: false,
        };
        editor.load(ui, value)?;
        Ok(Box::new(editor))
    }

    fn caption(&self, key: &str) -> Option<&str> {
        self.items
            .iter()
            .find(|i| i.key == key)
            .map(|i| i.text.as_str())
    }
    fn typed(&self, key: Option<&str>) -> Result<CellValue, String> {
        match key {
            None => Ok(CellValue::Null),
            Some(key) if self.integer => key
                .parse::<i64>()
                .map(CellValue::Integer)
                .map_err(|_| "Недопустимый числовой ключ.".into()),
            Some(key) => Ok(CellValue::Text(key.into())),
        }
    }
}

impl InspectorEditor for Choices {
    fn owner(&self) -> WidgetId {
        self.owner
    }
    fn focus(&self) -> WidgetId {
        self.focus
    }
    fn inline(&self) -> bool {
        matches!(self.view, ChoiceView::Radio | ChoiceView::List)
    }
    fn set_active(&self, ui: &mut Controls, active: bool) -> Result<(), ControlError> {
        if !active
            && ui
                .rich
                .popup
                .as_ref()
                .is_some_and(|p| p.owner == self.owner)
        {
            ui.close_rich_popup(false)?;
        }
        Ok(())
    }
    fn load(&self, ui: &mut Controls, value: &CellValue) -> Result<(), ControlError> {
        if self.view == ChoiceView::Suggest {
            return accepted(ui.set_editor_value(self.focus, raw_text(value)?)?);
        }
        let key = as_key(value)?;
        if let Some(key) = &key
            && self.caption(key).is_none()
        {
            return Err(invalid("Unknown selection key"));
        }
        if self.view == ChoiceView::Radio {
            return ui.set_radio_selected_key(self.owner, key.as_deref());
        }
        if ui.document(self.focus).is_some() {
            ui.cancel_edit(self.focus)?;
        }
        ui.set_selected_keys(self.owner, key.as_slice())?;
        let caption = key.as_deref().and_then(|k| self.caption(k)).unwrap_or("");
        if ui
            .document(self.focus)
            .is_some_and(|doc| doc.text() != caption)
        {
            accepted(ui.set_editor_value(self.focus, caption)?)?;
        }
        Ok(())
    }
    fn read(&self, ui: &Controls) -> Result<CellValue, String> {
        if self.view == ChoiceView::Suggest {
            return ui
                .document(self.focus)
                .map(|doc| CellValue::Text(doc.text().into()))
                .ok_or_else(|| "Suggestion editor is unavailable".into());
        }
        if self.view == ChoiceView::Radio {
            return self.typed(ui.radio_selected_key(self.owner));
        }
        if let Some(doc) = ui.document(self.focus) {
            let text = doc.text();
            if self.view == ChoiceView::Complete && !text.is_empty() {
                if self.items.iter().any(|item| item.text == text) {
                    let exact = ui.resolve_lookup_text(self.owner, text)?;
                    return self.typed(exact.first().map(String::as_str));
                }
                let folded = text.to_lowercase();
                let mut matching = self
                    .items
                    .iter()
                    .filter(|item| item.text.to_lowercase().starts_with(&folded));
                if let Some(only) = matching.next()
                    && matching.next().is_none()
                {
                    return self.typed(Some(&only.key));
                }
            }
            let keys = ui.resolve_lookup_text(self.owner, text)?;
            return self.typed(keys.first().map(String::as_str));
        }
        self.typed(
            ui.selected_keys(self.owner)
                .and_then(|keys| keys.first())
                .map(String::as_str),
        )
    }
    fn update(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        if !matches!(
            self.view,
            ChoiceView::Search | ChoiceView::Complete | ChoiceView::Suggest
        ) {
            return Ok(());
        }
        let expanded = ui
            .rich
            .popup
            .as_ref()
            .is_some_and(|p| p.owner == self.owner);
        let dismissed = self.popup_was_open && !expanded;
        self.popup_was_open = expanded;
        let searching = ui.runtime.focused() == Some(self.focus)
            && ui.is_edit_dirty(self.focus) == Some(true)
            && ui
                .document(self.focus)
                .is_some_and(|doc| doc.composition().is_none());
        if !searching {
            self.last_query.clear();
            return Ok(());
        }
        let doc = ui.document(self.focus).unwrap();
        let query = doc.text().to_owned();
        if query != self.last_query {
            let caret = doc.selection();
            let extending = query.len() > self.last_query.len();
            self.last_query.clone_from(&query);
            if !query.is_empty() && !dismissed && !ui.popup_open() {
                ui.open_editor_popup(self.owner)?;
                self.popup_was_open = true;
            }
            if self.view == ChoiceView::Complete
                && extending
                && caret.anchor == query.len()
                && caret.focus == query.len()
            {
                let folded = query.to_lowercase();
                let mut matching = self
                    .items
                    .iter()
                    .map(|i| i.text.as_str())
                    .filter(|t| t.to_lowercase().starts_with(&folded));
                if let Some(caption) = matching.next()
                    && matching.next().is_none()
                    && caption.len() > query.len()
                    && caption
                        .get(..query.len())
                        .is_some_and(|prefix| prefix.to_lowercase() == folded)
                {
                    ui.set_text(self.focus, caption)?;
                    ui.set_editor_selection(self.focus, query.len(), caption.len())?;
                    self.last_query = caption.into();
                }
            }
        }
        Ok(())
    }
    fn rest_label(&self, value: &CellValue) -> Option<String> {
        self.caption(&as_key(value).ok()??).map(str::to_owned)
    }
}

pub(super) fn token_keys(
    value: &CellValue,
    available: &[String],
) -> Result<Vec<String>, ControlError> {
    if matches!(value, CellValue::Null) {
        return Ok(vec![]);
    }
    let raw = raw_text(value)?;
    if raw.len() > 128 * 2048 {
        return Err(invalid("Token array exceeds its size limit"));
    }
    let parsed: Vec<String> = serde_json::from_str(raw)
        .map_err(|_| invalid("TokenList requires a JSON array of string keys"))?;
    if parsed.len() > available.len()
        || parsed.iter().collect::<HashSet<_>>().len() != parsed.len()
        || parsed.iter().any(|key| !available.contains(key))
    {
        return Err(invalid("TokenList contains unknown or duplicate keys"));
    }
    Ok(parsed)
}

pub(super) struct Tokens {
    parts: EditorControl,
    keys: Vec<String>,
    captions: Vec<String>,
    loaded: std::cell::RefCell<(String, Vec<String>)>,
}
impl Tokens {
    pub(super) fn mount(
        ui: &mut Controls,
        host: WidgetId,
        label: &str,
        d: &PropertyDescriptor,
        value: &CellValue,
    ) -> Result<Box<dyn InspectorEditor>, ControlError> {
        let keys = keys(d).to_vec();
        let items = keys
            .iter()
            .zip(&d.choices)
            .map(|(key, caption)| SelectionItem::new(key, caption))
            .collect();
        let parts = ui.combo_box(
            host,
            label,
            items,
            ComboBoxSettings {
                editable: true,
                read_only: d.metadata.read_only,
            },
            layout(),
        )?;
        ui.set_combo_selection_settings(
            parts.container,
            ComboSelectionSettings {
                mode: SelectionMode::Multiple,
                presentation: ComboSelectionPresentation::Tokens,
                commit_on_focus_lost: false,
            },
        )?;
        let editor = Self {
            parts,
            keys,
            captions: d.choices.clone(),
            loaded: Default::default(),
        };
        editor.load(ui, value)?;
        Ok(Box::new(editor))
    }
}
impl InspectorEditor for Tokens {
    fn owner(&self) -> WidgetId {
        self.parts.container
    }
    fn focus(&self) -> WidgetId {
        self.parts.editor
    }
    fn inline(&self) -> bool {
        true
    }
    fn set_active(&self, ui: &mut Controls, active: bool) -> Result<(), ControlError> {
        ui.reserve_combo_query_width(self.owner(), active)?;
        if !active
            && ui
                .rich
                .popup
                .as_ref()
                .is_some_and(|p| p.owner == self.owner())
        {
            ui.close_rich_popup(false)?;
        }
        Ok(())
    }
    fn load(&self, ui: &mut Controls, value: &CellValue) -> Result<(), ControlError> {
        let list = token_keys(value, &self.keys)?;
        ui.set_selected_keys(self.owner(), &list)?;
        let original = if matches!(value, CellValue::Null) {
            "[]"
        } else {
            raw_text(value)?
        };
        *self.loaded.borrow_mut() = (original.into(), list);
        Ok(())
    }
    fn read(&self, ui: &Controls) -> Result<CellValue, String> {
        let list = ui
            .combo_selection_draft(self.owner())
            .ok_or("Token editor is unavailable")?;
        let original = self.loaded.borrow();
        let raw = if list == original.1 {
            original.0.clone()
        } else {
            serde_json::to_string(list).map_err(|e| e.to_string())?
        };
        Ok(CellValue::Text(raw))
    }
    fn rest_label(&self, value: &CellValue) -> Option<String> {
        let list = token_keys(value, &self.keys).ok()?;
        Some(
            list.iter()
                .filter_map(|key| {
                    self.keys
                        .iter()
                        .position(|k| k == key)
                        .map(|i| self.captions[i].as_str())
                })
                .collect::<Vec<_>>()
                .join(", "),
        )
    }
}
