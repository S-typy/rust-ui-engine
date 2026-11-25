//! A read-only GridControl backed by a bounded window of provider rows.
use super::*;
use rust_desktop_ui_data::{
    CollectionAdapter, ComparisonOperator, Criteria, PageRequest, PageStatus, PageTotal,
    PagedDataSource, QueryExpression, QueryRequest,
};
struct ProviderTotals {
    generation: u64,
    snapshot: u64,
    values: Vec<(String, CellValue)>,
}
/// The application schedules requests on its worker and calls poll on the UI thread.
/// Only an accepted current page is installed in the mounted GridControl.
pub struct GridPageView {
    pending: Rc<RefCell<Option<DataQuery>>>,
    header: DataQuery,
    id: WidgetId,
    pages: PagedDataSource,
    offset: u64,
    shown: Option<(u64, u64, u64)>,
    summary: Option<ProviderTotals>,
    base: QueryRequest,
    path: Vec<(String, CellValue)>,
}
impl GridPageView {
    pub fn mount(
        ui: &mut Controls,
        parent: WidgetId,
        pages: PagedDataSource,
        style: LayoutStyle,
    ) -> Result<Self, ControlError> {
        let base = pages.query().clone();
        let table = DataTable::new(pages.schema().clone(), vec![]).map_err(error)?;
        let handle = Self::handle(table)?;
        let id = ui.grid_control(
            parent,
            handle,
            GridControlSettings {
                is_read_only: true,
                ..Default::default()
            },
            style,
        )?;
        let pending = Rc::new(RefCell::new(None));
        ui.grids.get_mut(&id).unwrap().source.page_query = Some(pending.clone());
        Ok(Self {
            id,
            pending,
            header: DataQuery::default(),
            pages,
            offset: 0,
            shown: None,
            summary: None,
            base,
            path: vec![],
        })
    }
    fn handle(table: DataTable) -> Result<DataSourceHandle, ControlError> {
        Ok(Rc::new(RefCell::new(
            DataSource::new(CollectionAdapter::new(table)).map_err(error)?,
        )))
    }
    pub fn id(&self) -> WidgetId {
        self.id
    }
    pub fn offset(&self) -> u64 {
        self.offset
    }
    pub fn set_waker(
        &mut self,
        waker: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    ) -> Result<(), ControlError> {
        self.pages
            .set_completion_waker(waker)
            .map_err(|e| error(format!("Page wake: {e:?}")))
    }
    pub fn source(&self) -> &PagedDataSource {
        &self.pages
    }
    pub fn status(&self) -> PageStatus {
        self.pages.page_status(self.offset)
    }
    /// Publish provider totals only for an already accepted query snapshot.
    pub fn accept_totals(
        &mut self,
        token: rust_desktop_ui_data::PageToken,
        snapshot: u64,
        values: Vec<(String, CellValue)>,
    ) -> Result<bool, ControlError> {
        if !self.pages.owns_generation(token) || self.pages.snapshot_revision() != Some(snapshot) {
            return Ok(false);
        }
        if values.len() > 16
            || values.iter().any(|(t, v)| {
                t.len() > 256
                    || v.display().len() > 16384
                    || matches!(v,CellValue::Real(x) if !x.is_finite())
            })
        {
            return Err(error("Invalid page totals"));
        }
        self.summary = Some(ProviderTotals {
            generation: token.generation(),
            snapshot,
            values,
        });
        Ok(true)
    }
    pub fn totals(&self) -> &[(String, CellValue)] {
        self.summary
            .as_ref()
            .filter(|v| {
                v.generation == self.pages.generation()
                    && Some(v.snapshot) == self.pages.snapshot_revision()
            })
            .map_or(&[], |v| v.values.as_slice())
    }
    pub fn path(&self) -> &[(String, CellValue)] {
        &self.path
    }
    fn remember(&mut self, ui: &Controls) -> Result<(), ControlError> {
        if self.shown.is_some() {
            let key = ui.grid_selected_key(self.id);
            let previously_visible = self.pages.selected_key().is_some_and(|k| {
                self.pages
                    .page(self.offset)
                    .is_some_and(|p| p.table.row(k).is_some())
            });
            if key.is_some() || previously_visible {
                self.pages.set_selected_key(key).map_err(error)?;
            }
        }
        Ok(())
    }
    pub fn select(&mut self, ui: &mut Controls, key: Option<i64>) -> Result<(), ControlError> {
        let visible = key.filter(|k| {
            self.pages
                .page(self.offset)
                .is_some_and(|p| p.table.row(*k).is_some())
        });
        ui.set_grid_selected_key(self.id, visible)?;
        self.pages.set_selected_key(key).map_err(error)
    }
    pub fn navigate(
        &mut self,
        ui: &mut Controls,
        row_offset: u64,
    ) -> Result<Vec<PageRequest>, ControlError> {
        let size = self.pages.settings().page_size as u64;
        let offset = row_offset / size * size;
        if matches!(self.pages.total(),PageTotal::Known(n) if offset>=n && offset!=0) {
            return Err(error("Requested page is outside the source"));
        }
        if offset != self.offset {
            self.remember(ui)?;
            self.pages.cancel_all().map_err(error)?;
            self.offset = offset;
        }
        let requests = self.pages.request_window(offset, 1).map_err(error)?;
        self.publish(ui)?;
        Ok(requests)
    }
    pub fn query(
        &mut self,
        ui: &mut Controls,
        query: QueryRequest,
    ) -> Result<Vec<PageRequest>, ControlError> {
        self.remember(ui)?;
        self.pages.set_query(query.clone()).map_err(error)?;
        self.base = query;
        self.pending.borrow_mut().take();
        self.header = DataQuery::default();
        self.show_header(ui);
        self.path.clear();
        self.offset = 0;
        self.navigate(ui, 0)
    }
    /// Each typed branch path is pushed into the provider query. Collapsing to
    /// an ancestor invalidates outstanding descendant requests by generation.
    pub fn open_path(
        &mut self,
        ui: &mut Controls,
        path: Vec<(String, CellValue)>,
    ) -> Result<Vec<PageRequest>, ControlError> {
        if path.len() > 8 {
            return Err(error("Grid branch depth exceeds eight"));
        }
        let mut query = self.header_query();
        let mut terms: Vec<_> = query.filter.take().into_iter().collect();
        let mut fields = HashSet::new();
        for (f, value) in &path {
            if !fields.insert(f) {
                return Err(error("Repeated branch field"));
            }
            let field = self
                .pages
                .schema()
                .field(f)
                .ok_or_else(|| error("Unknown branch field"))?;
            field.validate(value).map_err(error)?;
            terms.push(if *value == CellValue::Null {
                Criteria::IsNull(QueryExpression::field(f))
            } else {
                Criteria::Compare {
                    left: QueryExpression::field(f),
                    operator: ComparisonOperator::Equal,
                    right: QueryExpression::literal(value.clone()),
                }
            });
        }
        query.filter = match terms.len() {
            0 => None,
            1 => terms.pop(),
            _ => Some(Criteria::All(terms)),
        };
        self.remember(ui)?;
        self.pages.set_query(query).map_err(error)?;
        self.path = path;
        self.offset = 0;
        self.navigate(ui, 0)
    }
    fn header_query(&self) -> QueryRequest {
        use rust_desktop_ui_data::{FieldType, QueryOrder, TextComparison, TextMatchOperator};
        let mut next = self.base.clone();
        if !self.header.sort.is_empty() {
            next.order_by = self
                .header
                .sort
                .iter()
                .map(|s| QueryOrder {
                    expression: QueryExpression::field(&s.field),
                    direction: s.direction,
                })
                .collect();
        }
        if !self.header.filter.is_empty() {
            let terms: Vec<_> = self
                .pages
                .schema()
                .fields()
                .iter()
                .filter(|f| f.kind == FieldType::Text)
                .map(|f| Criteria::TextMatch {
                    text: QueryExpression::field(&f.name),
                    pattern: QueryExpression::literal(CellValue::Text(self.header.filter.clone())),
                    operator: TextMatchOperator::Contains,
                    comparison: TextComparison::Ordinal,
                })
                .collect();
            // A schema with no text fields has no matching text.
            let search = if terms.is_empty() {
                Criteria::Compare {
                    left: QueryExpression::literal(CellValue::Integer(0)),
                    operator: ComparisonOperator::Equal,
                    right: QueryExpression::literal(CellValue::Integer(1)),
                }
            } else {
                Criteria::Any(terms)
            };
            next.filter = Some(match next.filter.take() {
                Some(base) => Criteria::All(vec![base, search]),
                None => search,
            });
        }
        next
    }
    /// Translate header gestures and FilterText into provider requests on the owner tick.
    pub fn take_input(&mut self, ui: &mut Controls) -> Result<Vec<PageRequest>, ControlError> {
        let Some(input) = self.pending.borrow_mut().take() else {
            return Ok(vec![]);
        };
        let previous = std::mem::replace(&mut self.header, input);
        let result = self.open_path(ui, self.path.clone());
        if result.is_err() {
            self.header = previous;
        }
        self.show_header(ui);
        result
    }
    fn show_header(&self, ui: &mut Controls) {
        if let Some(g) = ui.grids.get_mut(&self.id) {
            g.source.data_query = self.header.clone();
            let sorts =
                self.header
                    .sort
                    .iter()
                    .filter_map(|s| {
                        Some(SortOrder {
                            column: ColumnId(
                                g.source.fields.iter().position(|f| f == &s.field)? as u64
                            ),
                            direction: if s.direction
                                == rust_desktop_ui_data::SortDirection::Descending
                            {
                                SortDirection::Descending
                            } else {
                                SortDirection::Ascending
                            },
                        })
                    })
                    .collect();
            g.model.set_query_presentation(GridQuery {
                filter: self.header.filter.clone(),
                sort: sorts,
            });
        }
    }
    pub fn retry(&mut self) -> Result<PageRequest, ControlError> {
        self.pages.retry(self.offset).map_err(error)
    }
    pub fn poll(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        match self.pages.drain_completions(64) {
            Ok(batch) => {
                for (token, result) in batch.results {
                    if let Err(e) = result {
                        self.pages
                            .complete(token, Err(e.to_string()))
                            .map_err(error)?;
                    }
                }
            }
            Err(rust_desktop_ui_data::PageSendError::Busy) => return Ok(()),
            Err(e) => return Err(error(format!("Grid page mailbox: {e:?}"))),
        }
        self.publish(ui)
    }
    fn publish(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        let page = self.pages.page(self.offset);
        let stamp = page.map(|p| (self.pages.generation(), self.offset, p.snapshot_revision));
        if stamp == self.shown {
            return Ok(());
        }
        let table = match page {
            Some(p) => p.table.clone(),
            None => DataTable::new(self.pages.schema().clone(), vec![]).map_err(error)?,
        };
        let selected = self
            .pages
            .selected_key()
            .filter(|k| table.row(*k).is_some());
        ui.replace_grid_source(self.id, Self::handle(table)?)?;
        ui.set_grid_selected_key(self.id, selected)?;
        ui.grids.get_mut(&self.id).unwrap().source.page_query = Some(self.pending.clone());
        self.show_header(ui);
        self.shown = stamp;
        Ok(())
    }
}
