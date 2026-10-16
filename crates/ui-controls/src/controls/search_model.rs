//! Bounded query, request identity and history independent of provider execution.
use super::*;
use rust_desktop_ui_data::{
    Criteria, QueryExpression, QueryRequest, TextComparison, TextMatchOperator,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub const MAX_SEARCH_QUERY_BYTES: usize = 65_536;
pub const MAX_SEARCH_RESULT_BYTES: usize = 1_048_576;
const MAX_EVENTS: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchChoicePolicy {
    /// Choosing fills Query and publishes SuggestionChosen. Search remains explicit.
    #[default]
    FillQuery,
    /// Choosing also publishes a distinct SearchRequested event after SuggestionChosen.
    FillAndSearch,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SearchBoxSettings {
    pub min_query_length: usize,
    pub max_query_length: usize,
    pub max_suggestions: usize,
    pub history_limit: usize,
    pub popup_size: Size,
    pub choice_policy: SearchChoicePolicy,
}
impl Default for SearchBoxSettings {
    fn default() -> Self {
        Self {
            min_query_length: 1,
            max_query_length: 256,
            max_suggestions: 100,
            history_limit: 20,
            popup_size: Size::new(360.0, 280.0),
            choice_policy: SearchChoicePolicy::FillQuery,
        }
    }
}
impl SearchBoxSettings {
    pub fn validate(self) -> Result<(), ControlError> {
        if self.max_query_length == 0
            || self.max_query_length > 4096
            || self.min_query_length > self.max_query_length
            || !(1..=1000).contains(&self.max_suggestions)
            || self.history_limit > 100
            || !self.popup_size.is_valid()
            || !(160.0..=4096.0).contains(&self.popup_size.width)
            || !(80.0..=4096.0).contains(&self.popup_size.height)
        {
            return Err(invalid(
                "Invalid SearchBox query, result, history or popup limits",
            ));
        }
        Ok(())
    }
    pub(super) fn validate_query(self, query: &str) -> Result<(), ControlError> {
        TextLimits {
            max_graphemes: Some(self.max_query_length),
            max_bytes: MAX_SEARCH_QUERY_BYTES,
        }
        .validate(query)
        .map_err(edit_error)?;
        if query.contains(['\n', '\r', '\0']) {
            return Err(invalid("Search Query must be a single line without NUL"));
        }
        Ok(())
    }
    fn eligible(self, query: &str) -> bool {
        self.min_query_length == 0
            || TextLimits {
                max_graphemes: Some(self.min_query_length - 1),
                max_bytes: MAX_SEARCH_QUERY_BYTES,
            }
            .validate(query)
            .is_err()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchSuggestion {
    pub key: String,
    pub text: String,
    pub query: String,
    pub description: Option<String>,
    pub enabled: bool,
}
impl SearchSuggestion {
    pub fn new(key: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            key: key.into(),
            query: text.clone(),
            text,
            description: None,
            enabled: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchStatus {
    Idle,
    Pending,
    Ready,
    Failed(String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchCompletion {
    Applied,
    Rejected,
    Stale,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchEventKind {
    SuggestionChosen { key: String, query: String },
    SearchRequested { query: String },
    SuggestionsFailed { message: String },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchEvent {
    pub owner: WidgetId,
    pub kind: SearchEventKind,
}

#[derive(Debug)]
struct RequestIdentity {
    owner: WidgetId,
    generation: u64,
    query: String,
    cancelled: AtomicBool,
}
/// An unforgeable request token. Providers may clone it and move it to a worker;
/// results must be completed on the UI thread. Cancellation is cooperative.
#[derive(Clone, Debug)]
pub struct SearchSuggestionRequest(Arc<RequestIdentity>);
impl SearchSuggestionRequest {
    pub fn owner(&self) -> WidgetId {
        self.0.owner
    }
    pub fn query(&self) -> &str {
        &self.0.query
    }
    pub fn generation(&self) -> u64 {
        self.0.generation
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }
    fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Release);
    }
}

pub(super) struct SearchSession {
    pub settings: SearchBoxSettings,
    pub query: String,
    pub suggestions: Vec<SearchSuggestion>,
    pub status: SearchStatus,
    pub history: Vec<String>,
    pub events: Vec<SearchEventKind>,
    generation: u64,
    pending: Option<SearchSuggestionRequest>,
    undrained: bool,
}
impl Drop for SearchSession {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl SearchSession {
    pub fn new(settings: SearchBoxSettings, query: &str) -> Result<Self, ControlError> {
        settings.validate()?;
        settings.validate_query(query)?;
        Ok(Self {
            settings,
            query: query.into(),
            suggestions: Vec::new(),
            status: SearchStatus::Idle,
            history: Vec::new(),
            events: Vec::new(),
            generation: 0,
            pending: None,
            undrained: false,
        })
    }
    pub fn cancel(&mut self) {
        if let Some(request) = self.pending.take() {
            request.cancel();
        }
        self.undrained = false;
        if self.status == SearchStatus::Pending {
            self.status = SearchStatus::Idle;
        }
    }
    pub fn matches(&self, request: &SearchSuggestionRequest) -> bool {
        !request.is_cancelled()
            && self
                .pending
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(&current.0, &request.0))
    }
    pub fn start(&mut self, owner: WidgetId, query: &str) -> Result<(), ControlError> {
        self.settings.validate_query(query)?;
        let generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| invalid("Search generation exhausted"))?;
        self.cancel();
        self.generation = generation;
        self.query = query.into();
        self.status = SearchStatus::Idle;
        if self.settings.eligible(query) {
            self.pending = Some(SearchSuggestionRequest(Arc::new(RequestIdentity {
                owner,
                generation,
                query: query.into(),
                cancelled: AtomicBool::new(false),
            })));
            self.undrained = true;
            self.status = SearchStatus::Pending;
        }
        Ok(())
    }
    pub fn take_request(&mut self) -> Option<SearchSuggestionRequest> {
        if !std::mem::take(&mut self.undrained) {
            return None;
        }
        self.pending.clone()
    }
    pub fn validate_results(&self, values: &[SearchSuggestion]) -> Result<(), ControlError> {
        if values.len() > self.settings.max_suggestions {
            return Err(invalid("Too many search suggestions"));
        }
        let mut bytes = 0usize;
        let mut keys = std::collections::HashSet::new();
        for item in values {
            self.settings.validate_query(&item.query)?;
            if item.key.is_empty()
                || item.key.len() > 128
                || !keys.insert(&item.key)
                || item.text.len() > 4096
                || item.description.as_ref().is_some_and(|s| s.len() > 4096)
                || item.text.contains('\0')
                || item.description.as_ref().is_some_and(|s| s.contains('\0'))
            {
                return Err(invalid("Invalid or duplicate search suggestion"));
            }
            bytes = bytes
                .checked_add(
                    item.key.len()
                        + item.text.len()
                        + item.query.len()
                        + item.description.as_ref().map_or(0, String::len),
                )
                .ok_or_else(|| invalid("Search result byte overflow"))?;
            if bytes > MAX_SEARCH_RESULT_BYTES {
                return Err(invalid("Search suggestions exceed one MiB"));
            }
        }
        Ok(())
    }
    pub fn complete(
        &mut self,
        request: &SearchSuggestionRequest,
        result: Result<Vec<SearchSuggestion>, String>,
    ) -> Result<SearchCompletion, ControlError> {
        if !self.matches(request) {
            return Ok(SearchCompletion::Stale);
        }
        let result = result.and_then(|values| {
            self.validate_results(&values)
                .map(|_| values)
                .map_err(|e| e.to_string())
        });
        let error = result.as_ref().err().map(|message| {
            if message.len() > 4096 {
                "Suggestion provider error exceeds 4096 UTF-8 bytes".into()
            } else {
                message.clone()
            }
        });
        if error.is_some() {
            self.preflight_events(1)?;
        }
        self.cancel();
        if let Some(message) = error {
            self.status = SearchStatus::Failed(message.clone());
            self.events
                .push(SearchEventKind::SuggestionsFailed { message });
            return Ok(SearchCompletion::Rejected);
        }
        self.suggestions = result.unwrap();
        self.status = SearchStatus::Ready;
        Ok(SearchCompletion::Applied)
    }
    pub fn preflight_events(&self, count: usize) -> Result<(), ControlError> {
        if self.events.len().saturating_add(count) > MAX_EVENTS {
            return Err(invalid(
                "Drain SearchBox events before requesting further actions",
            ));
        }
        Ok(())
    }
    pub fn candidate(&self, key: &str) -> Result<SearchSuggestion, ControlError> {
        if self.status != SearchStatus::Ready {
            return Err(invalid("Suggestions are not current"));
        }
        self.preflight_events(
            if self.settings.choice_policy == SearchChoicePolicy::FillAndSearch {
                2
            } else {
                1
            },
        )?;
        self.suggestions
            .iter()
            .find(|s| s.key == key && s.enabled)
            .cloned()
            .ok_or_else(|| invalid("Unknown or disabled search suggestion"))
    }
    pub fn choose(&mut self, item: SearchSuggestion) {
        self.cancel();
        self.query = item.query.clone();
        self.status = SearchStatus::Idle;
        self.events.push(SearchEventKind::SuggestionChosen {
            key: item.key,
            query: item.query,
        });
        if self.settings.choice_policy == SearchChoicePolicy::FillAndSearch {
            self.emit_search();
        }
    }
    pub fn search(&mut self, query: &str) -> Result<(), ControlError> {
        self.settings.validate_query(query)?;
        self.preflight_events(1)?;
        self.cancel();
        self.query = query.into();
        self.status = SearchStatus::Idle;
        self.emit_search();
        Ok(())
    }
    fn emit_search(&mut self) {
        if !self.query.is_empty() && self.settings.history_limit > 0 {
            self.history.retain(|value| value != &self.query);
            self.history.insert(0, self.query.clone());
            self.history.truncate(self.settings.history_limit);
        }
        self.events.push(SearchEventKind::SearchRequested {
            query: self.query.clone(),
        });
    }
    pub fn set_history(&mut self, values: Vec<String>) -> Result<(), ControlError> {
        if values.len() > self.settings.history_limit {
            return Err(invalid("Search history exceeds its limit"));
        }
        let mut unique = std::collections::HashSet::new();
        for value in &values {
            self.settings.validate_query(value)?;
            if value.is_empty() || !unique.insert(value) {
                return Err(invalid("History requires nonempty unique queries"));
            }
        }
        self.history = values;
        Ok(())
    }
}

/// Explicit bridge to the shared query engine. Field types, null behavior and
/// matching semantics are compiled/evaluated by QueryEngine, not by SearchBox.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchTarget {
    pub fields: Vec<String>,
    pub operator: TextMatchOperator,
    pub comparison: TextComparison,
}
impl SearchTarget {
    pub fn query_request(&self, query: &str) -> Result<QueryRequest, ControlError> {
        let mut unique = std::collections::HashSet::new();
        if self.fields.is_empty()
            || self.fields.len() > 32
            || self
                .fields
                .iter()
                .any(|field| field.is_empty() || field.len() > 256 || !unique.insert(field))
        {
            return Err(invalid(
                "Search target requires 1..32 unique bounded field names",
            ));
        }
        SearchBoxSettings {
            max_query_length: 4096,
            ..Default::default()
        }
        .validate_query(query)?;
        Ok(QueryRequest {
            filter: (!query.is_empty()).then(|| {
                Criteria::Any(
                    self.fields
                        .iter()
                        .map(|field| Criteria::TextMatch {
                            text: QueryExpression::field(field),
                            pattern: QueryExpression::literal(
                                rust_desktop_ui_data::CellValue::Text(query.into()),
                            ),
                            operator: self.operator,
                            comparison: self.comparison,
                        })
                        .collect(),
                )
            }),
            ..Default::default()
        })
    }
}
fn invalid(message: &str) -> ControlError {
    ControlError::Invalid(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn owner() -> WidgetId {
        UiTree::new().root()
    }
    #[test]
    fn requests_coalesce_cancel_and_reject_other_session_and_old_generation() {
        let owner = owner();
        let mut first = SearchSession::new(Default::default(), "").unwrap();
        first.start(owner, "a").unwrap();
        let old = first.take_request().unwrap();
        first.start(owner, "ab").unwrap();
        first.start(owner, "abc").unwrap();
        assert!(old.is_cancelled());
        let current = first.take_request().unwrap();
        assert_eq!(current.query(), "abc");
        assert!(first.take_request().is_none());
        let mut other = SearchSession::new(Default::default(), "").unwrap();
        other.start(owner, "abc").unwrap();
        assert_eq!(
            other.complete(&current, Ok(Vec::new())).unwrap(),
            SearchCompletion::Stale
        );
        assert_eq!(
            first.complete(&old, Ok(Vec::new())).unwrap(),
            SearchCompletion::Stale
        );
        assert_eq!(
            first
                .complete(&current, Ok(vec![SearchSuggestion::new("k", "abc")]))
                .unwrap(),
            SearchCompletion::Applied
        );
        assert_eq!(
            first.complete(&current, Ok(Vec::new())).unwrap(),
            SearchCompletion::Stale
        );
        assert_eq!(first.suggestions.len(), 1);
    }
    #[test]
    fn malformed_results_fail_atomically_and_publish_separate_provider_error() {
        let mut state = SearchSession::new(Default::default(), "").unwrap();
        let owner = owner();
        state.start(owner, "a").unwrap();
        let request = state.take_request().unwrap();
        state
            .complete(&request, Ok(vec![SearchSuggestion::new("a", "Alpha")]))
            .unwrap();
        let previous = state.suggestions.clone();
        state.start(owner, "b").unwrap();
        let request = state.take_request().unwrap();
        assert_eq!(
            state
                .complete(
                    &request,
                    Ok(vec![
                        SearchSuggestion::new("x", "X"),
                        SearchSuggestion::new("x", "Y")
                    ])
                )
                .unwrap(),
            SearchCompletion::Rejected
        );
        assert_eq!(state.suggestions, previous);
        assert_eq!(state.query, "b");
        assert!(matches!(state.status, SearchStatus::Failed(_)));
        assert!(matches!(
            &state.events[..],
            [SearchEventKind::SuggestionsFailed { .. }]
        ));
        assert!(request.is_cancelled());
    }
    #[test]
    fn choice_and_search_are_distinct_and_history_is_bounded_deduplicated() {
        let mut state = SearchSession::new(
            SearchBoxSettings {
                history_limit: 2,
                ..Default::default()
            },
            "",
        )
        .unwrap();
        state.start(owner(), "a").unwrap();
        let request = state.take_request().unwrap();
        state
            .complete(&request, Ok(vec![SearchSuggestion::new("a", "Alpha")]))
            .unwrap();
        let chosen = state.candidate("a").unwrap();
        state.choose(chosen);
        assert!(matches!(
            &state.events[..],
            [SearchEventKind::SuggestionChosen { .. }]
        ));
        assert!(state.history.is_empty());
        for query in ["Alpha", "Beta", "Gamma", "Beta"] {
            state.search(query).unwrap();
        }
        assert_eq!(state.history, ["Beta", "Gamma"]);
        let old = state.history.clone();
        assert!(
            state
                .set_history(vec!["duplicate".into(), "duplicate".into()])
                .is_err()
        );
        assert_eq!(state.history, old);
        state.set_history(Vec::new()).unwrap();
        assert!(state.history.is_empty());
    }
    #[test]
    fn minimum_counts_graphemes_and_drop_cancels_inflight_provider() {
        let mut state = SearchSession::new(
            SearchBoxSettings {
                min_query_length: 2,
                ..Default::default()
            },
            "",
        )
        .unwrap();
        state.start(owner(), "a\u{301}").unwrap();
        assert!(state.take_request().is_none());
        state.start(owner(), "a\u{301}b").unwrap();
        let request = state.take_request().unwrap();
        drop(state);
        assert!(request.is_cancelled());
    }
}
