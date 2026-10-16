//! Reference-only selection and explicit column search policy.
use super::selection::CompositeKind;
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LookupSearchMode {
    #[default]
    Contains,
    StartsWith,
}

/// UnicodeLowercase applies Rust Unicode lowercase mapping to both operands.
/// It is independent of display culture and is not linguistic collation,
/// normalization, accent folding, or full Unicode case folding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LookupSearchComparison {
    Ordinal,
    #[default]
    UnicodeLowercase,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LookupSearchSettings {
    /// Zero is the display caption, 1..31 are additional columns. Empty means
    /// all columns. A missing cell in a headerless source does not match.
    pub columns: Vec<usize>,
    pub mode: LookupSearchMode,
    pub comparison: LookupSearchComparison,
}
impl LookupSearchSettings {
    pub fn validate(&self) -> Result<(), ControlError> {
        let mut seen = HashSet::new();
        if self.columns.len() > 32 || self.columns.iter().any(|&i| i >= 32 || !seen.insert(i)) {
            return Err(ControlError::Invalid(
                "Lookup search columns must be unique indices 0..31".into(),
            ));
        }
        Ok(())
    }
    fn validate_columns(&self, count: usize) -> Result<(), ControlError> {
        self.validate()?;
        if count > 0 && self.columns.iter().any(|&index| index >= count) {
            return Err(ControlError::Invalid(
                "Lookup search column is outside the declared headers".into(),
            ));
        }
        Ok(())
    }
    fn filter(&self, items: &[SelectionItem], query: &str) -> Vec<SelectionItem> {
        let query = match self.comparison {
            LookupSearchComparison::Ordinal => Cow::Borrowed(query),
            LookupSearchComparison::UnicodeLowercase => Cow::Owned(query.to_lowercase()),
        };
        items
            .iter()
            .filter(|item| {
                if query.is_empty() {
                    return true;
                }
                let matches = |value: &str| {
                    let value = match self.comparison {
                        LookupSearchComparison::Ordinal => Cow::Borrowed(value),
                        LookupSearchComparison::UnicodeLowercase => {
                            Cow::Owned(value.to_lowercase())
                        }
                    };
                    match self.mode {
                        LookupSearchMode::Contains => value.contains(query.as_ref()),
                        LookupSearchMode::StartsWith => value.starts_with(query.as_ref()),
                    }
                };
                if self.columns.is_empty() {
                    std::iter::once(&item.text)
                        .chain(item.columns.iter())
                        .any(|text| matches(text))
                } else {
                    self.columns.iter().any(|&index| {
                        let value = if index == 0 {
                            Some(&item.text)
                        } else {
                            item.columns.get(index - 1)
                        };
                        value.is_some_and(|value| matches(value))
                    })
                }
            })
            .cloned()
            .collect()
    }
}

#[derive(Default)]
pub(super) struct LookupState {
    settings: HashMap<WidgetId, LookupSearchSettings>,
}

impl Controls {
    pub fn lookup_search_settings(&self, id: WidgetId) -> Option<LookupSearchSettings> {
        self.is_lookup(id)
            .then(|| self.lookups.settings.get(&id).cloned().unwrap_or_default())
    }
    pub fn set_lookup_search_settings(
        &mut self,
        id: WidgetId,
        settings: LookupSearchSettings,
    ) -> Result<(), ControlError> {
        let count = self
            .lookup_columns(id)
            .ok_or_else(|| ControlError::Invalid("Not a LookupEdit".into()))?
            .len();
        settings.validate_columns(count)?;
        if self.lookup_search_settings(id).as_ref() == Some(&settings) {
            return Ok(());
        }
        if let Some(list) = self.lookup_popup_list(id) {
            let filtered =
                settings.filter(self.selection_items(id).unwrap(), &self.lookup_query(id));
            self.preflight_virtual_list_items(list, &filtered)?;
        }
        self.lookups.settings.insert(id, settings);
        self.refresh_lookup_popup(id)
    }
    pub(super) fn is_lookup(&self, id: WidgetId) -> bool {
        self.rich
            .composites
            .get(&id)
            .is_some_and(|c| matches!(c.kind, CompositeKind::Combo { lookup: true, .. }))
    }
    pub(super) fn validate_lookup_search_columns(
        &self,
        id: WidgetId,
        count: usize,
    ) -> Result<(), ControlError> {
        self.lookup_search_settings(id)
            .unwrap_or_default()
            .validate_columns(count)
    }
    pub(super) fn filter_editor_items(
        &self,
        owner: WidgetId,
        items: &[SelectionItem],
        query: &str,
    ) -> Vec<SelectionItem> {
        self.lookup_search_settings(owner)
            .unwrap_or_default()
            .filter(items, query)
    }
    pub(super) fn prune_lookups(&mut self) {
        self.lookups
            .settings
            .retain(|id, _| self.rich.composites.contains_key(id));
    }
    fn lookup_query(&self, owner: WidgetId) -> String {
        let Some(editor) = self.composite_editor(owner) else {
            return String::new();
        };
        if self.is_edit_dirty(editor) == Some(true) {
            self.document(editor)
                .map_or(String::new(), |doc| doc.text().to_owned())
        } else {
            String::new()
        }
    }
    fn lookup_popup_list(&self, owner: WidgetId) -> Option<WidgetId> {
        self.rich.popup.as_ref().filter(|p| p.owner == owner)?.list
    }
    pub(super) fn preflight_lookup_source(
        &mut self,
        owner: WidgetId,
        items: &[SelectionItem],
        kept: &[String],
    ) -> Result<(), ControlError> {
        if !self.is_lookup(owner) {
            return Ok(());
        }
        let editor = self.composite_editor(owner).unwrap();
        let caption = items
            .iter()
            .find(|item| kept.contains(&item.key))
            .map_or("", |item| item.text.as_str());
        let caption = normalize_editor_text(caption, true).map_err(edit_error)?;
        editor_limits(self.controls[&editor].editor.max_length)
            .validate(&caption)
            .map_err(edit_error)?;
        self.preflight_editor_text(editor, &caption)?;
        if let Some(list) = self.lookup_popup_list(owner) {
            let filtered = self.filter_editor_items(owner, items, &self.lookup_query(owner));
            self.preflight_virtual_list_items(list, &filtered)?;
        }
        Ok(())
    }
    pub(super) fn refresh_lookup_popup(&mut self, owner: WidgetId) -> Result<(), ControlError> {
        let Some(list) = self.lookup_popup_list(owner) else {
            return Ok(());
        };
        let query = self.lookup_query(owner);
        let filtered =
            self.filter_editor_items(owner, self.selection_items(owner).unwrap(), &query);
        let keys = self
            .combo_selection_draft(owner)
            .unwrap_or_default()
            .to_vec();
        self.set_selection_items(list, filtered)?;
        let state = self.rich.lists.get_mut(&list).unwrap();
        state.keys = keys
            .into_iter()
            .filter(|key| state.items.iter().any(|i| i.key == *key && i.enabled))
            .collect();
        if state
            .cursor
            .is_none_or(|i| !state.items.get(i).is_some_and(|item| item.enabled))
        {
            state.cursor = state.items.iter().position(|i| i.enabled);
        }
        self.rich.popup.as_mut().unwrap().query = query;
        self.materialize_list_rows(list)
    }
    /// Resolve displayed text without arbitrarily choosing among equal captions.
    /// Existing accepted identity wins when its unchanged caption still matches.
    pub(super) fn resolve_lookup_text(
        &self,
        owner: WidgetId,
        text: &str,
    ) -> Result<Vec<String>, String> {
        let items = self.selection_items(owner).ok_or("Unknown LookupEdit")?;
        let current = self.selected_keys(owner).unwrap_or_default();
        let matches = |item: &&SelectionItem| {
            item.enabled
                && normalize_editor_text(&item.text, true).is_ok_and(|caption| caption == text)
        };
        if let Some(item) = items
            .iter()
            .filter(matches)
            .find(|item| current.contains(&item.key))
        {
            return Ok(vec![item.key.clone()]);
        }
        if text.is_empty() {
            return Ok(Vec::new());
        }
        let mut candidates = items.iter().filter(matches);
        let Some(item) = candidates.next() else {
            return Err("Выберите значение из справочника.".into());
        };
        if self.rich.suppress_combo_commit != Some(owner) && candidates.next().is_some() {
            return Err(
                "Несколько записей имеют одинаковую подпись. Выберите запись из списка.".into(),
            );
        }
        Ok(vec![item.key.clone()])
    }
}
