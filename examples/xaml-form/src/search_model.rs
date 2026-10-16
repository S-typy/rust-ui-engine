//! Application-owned suggestion provider and explicit query execution.
use crate::view::MountedView;
use rust_desktop_ui_controls::{
    BindingContext, Controls, DataValue, SearchCompletion, SearchEventKind, SearchSuggestion,
    SearchSuggestionRequest, SearchTarget, SelectionItem,
};
use rust_desktop_ui_core::{Mutation, WidgetId};
use rust_desktop_ui_data::{
    CellValue, DataRow, DataTable, FieldSchema, FieldType, QueryEngine, TableSchema,
    TextComparison, TextMatchOperator,
};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

pub(crate) fn model() -> Result<Rc<RefCell<BindingContext>>, String> {
    BindingContext::new(DataValue::object([
        ("Query", "".into()),
        ("Locked", false.into()),
    ]))
    .map(|model| Rc::new(RefCell::new(model)))
    .map_err(err)
}
pub(crate) fn register(ui: &mut Controls) {
    ui.commands_mut()
        .register("SearchDemoHistory", "История запросов");
    ui.commands_mut()
        .register("SearchDemoClearHistory", "Очистить историю");
}
pub(crate) struct State {
    owner: WidgetId,
    list: WidgetId,
    title: WidgetId,
    status: WidgetId,
    table: DataTable,
    target: SearchTarget,
    pending: Vec<(SearchSuggestionRequest, Instant)>,
    searches: u64,
    choices: u64,
    stale: u64,
    failures: u64,
}
impl State {
    pub(crate) fn install(ui: &mut Controls, view: &MountedView) -> Result<Option<Self>, String> {
        let Some(owner) = view.find_name("DocumentSearch") else {
            return Ok(None);
        };
        let required = |name| {
            view.find_name(name)
                .ok_or_else(|| format!("Missing {name}"))
        };
        let schema = TableSchema::new(vec![
            FieldSchema::new("Title", FieldType::Text),
            FieldSchema::new("Category", FieldType::Text),
        ])
        .map_err(err)?;
        let rows = [
            ("План разработки", "Проекты"),
            ("План поездки", "Личное"),
            ("Годовой бюджет", "Финансы"),
            ("Отчёт за октябрь", "Финансы"),
            ("Руководство пользователя", "Документация"),
            ("Заметки по дизайну", "Проекты"),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (title, category))| {
            DataRow::new(
                index as i64 + 1,
                vec![
                    CellValue::Text(title.into()),
                    CellValue::Text(category.into()),
                ],
            )
        })
        .collect();
        let state = Self {
            owner,
            list: required("SearchResults")?,
            title: required("SearchResultTitle")?,
            status: required("SearchActivity")?,
            table: DataTable::new(schema, rows).map_err(err)?,
            target: SearchTarget {
                fields: vec!["Title".into(), "Category".into()],
                operator: TextMatchOperator::Contains,
                comparison: TextComparison::UnicodeLowercase,
            },
            pending: Vec::new(),
            searches: 0,
            choices: 0,
            stale: 0,
            failures: 0,
        };
        state.show_results(ui, "")?;
        Ok(Some(state))
    }
    fn keys(&self, query: &str) -> Result<Vec<i64>, String> {
        Ok(
            QueryEngine::execute(&self.table, &self.target.query_request(query).map_err(err)?)
                .map_err(err)?
                .row_keys,
        )
    }
    fn suggestions(&self, query: &str) -> Result<Vec<SearchSuggestion>, String> {
        if query.eq_ignore_ascii_case("ошибка") {
            return Err("Демонстрационный источник временно недоступен".into());
        }
        self.keys(query).map(|keys| {
            keys.into_iter()
                .map(|key| {
                    let row = self.table.row(key).unwrap();
                    let mut suggestion =
                        SearchSuggestion::new(key.to_string(), row.values[0].display());
                    suggestion.description = Some(row.values[1].display());
                    suggestion
                })
                .collect()
        })
    }
    fn show_results(&self, ui: &mut Controls, query: &str) -> Result<(), String> {
        let keys = self.keys(query)?;
        let count = keys.len();
        let items = keys
            .into_iter()
            .map(|key| {
                let row = self.table.row(key).unwrap();
                let mut item = SelectionItem::new(key.to_string(), row.values[0].display());
                item.columns = vec![row.values[1].display()];
                item
            })
            .collect();
        ui.set_selection_items(self.list, items).map_err(err)?;
        ui.set_label(self.title, format!("Найдено документов: {count}"))
            .map_err(err)
    }
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.pending.iter().map(|(_, at)| *at).min()
    }
    pub(crate) fn refresh(&mut self, ui: &mut Controls) -> Result<(), String> {
        let now = Instant::now();
        for request in ui.drain_search_suggestion_requests() {
            // The bounded example queue deliberately allows cancelled requests
            // to finish so stale-result handling can be observed.
            if self.pending.len() == 64 {
                self.pending.remove(0);
            }
            let delay = if request.query().eq_ignore_ascii_case("п") {
                900
            } else {
                300
            };
            self.pending
                .push((request, now + Duration::from_millis(delay)));
        }
        let pending = std::mem::take(&mut self.pending);
        for (request, at) in pending {
            if at > now {
                self.pending.push((request, at));
                continue;
            }
            let values = self.suggestions(request.query());
            if ui
                .complete_search_suggestions(&request, values)
                .map_err(err)?
                == SearchCompletion::Stale
            {
                self.stale += 1;
            }
        }
        for event in ui.drain_search_events() {
            if event.owner != self.owner {
                continue;
            }
            match event.kind {
                SearchEventKind::SuggestionChosen { .. } => self.choices += 1,
                SearchEventKind::SearchRequested { query } => {
                    self.searches += 1;
                    self.show_results(ui, &query)?;
                }
                SearchEventKind::SuggestionsFailed { .. } => self.failures += 1,
            }
        }
        ui.set_label(
            self.status,
            format!(
                "Поисков: {} · подсказок выбрано: {} · устаревших ответов: {} · ошибок: {}",
                self.searches, self.choices, self.stale, self.failures
            ),
        )
        .map_err(err)
    }
    pub(crate) fn command(&mut self, ui: &mut Controls, command: &str) -> Result<bool, String> {
        match command {
            "SearchDemoHistory" => {
                let editor = ui.search_parts(self.owner).ok_or("Search removed")?.editor;
                ui.runtime
                    .apply(Mutation::Focus(Some(editor)))
                    .map_err(err)?;
                ui.open_search_history(self.owner).map_err(err)?;
            }
            "SearchDemoClearHistory" => {
                ui.set_search_history(self.owner, Vec::new()).map_err(err)?
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}
fn err(value: impl std::fmt::Display) -> String {
    value.to_string()
}
