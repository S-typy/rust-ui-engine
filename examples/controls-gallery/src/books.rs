//! Deterministic application data for the gallery, not part of the grid crate.
//! Records are synthesized only when requested. One worker and bounded channels
//! handle query indexes and lazy children; frames read only viewport cells.

use std::{
    cmp::Ordering as CmpOrdering,
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread,
};

use rust_desktop_ui_treegrid::{
    ChildrenLoad, ColumnId, GridColumn, GridError, GridQuery, RequestId, RowKey, SortDirection,
    TreeDataSource, TreeGrid,
};

pub const MAX_DEMO_ROWS: usize = 1_000_000;
const QUEUE_CAPACITY: usize = 16;
const LAZY_EVERY: usize = 10_000;

type Edits = HashMap<(RowKey, ColumnId), String>;

struct QueryIndex {
    order: Vec<usize>,
    positions: Vec<usize>,
}

enum Job {
    Query {
        epoch: u64,
        count: usize,
        query: GridQuery,
        edits: Edits,
        cancel: Arc<AtomicBool>,
    },
    Children {
        request: RequestId,
        rows: Vec<RowKey>,
        cancel: Arc<AtomicBool>,
    },
}

enum Completion {
    Query {
        epoch: u64,
        index: QueryIndex,
    },
    Children {
        request: RequestId,
        rows: Vec<RowKey>,
    },
}

/// IDs are in 1..=4*row_count (maximum four million). Roots use index+1;
/// a lazy child uses row_count + parent_index*3 + child_index+1. This namespace
/// is stable across query order, filtering and sparse cell edits.
pub struct BookSource {
    count: usize,
    index: Option<QueryIndex>,
    edits: Edits,
    query: GridQuery,
    query_epoch: u64,
    query_cancel: Option<Arc<AtomicBool>>,
    pending_query: Option<Job>,
    pending_children: HashMap<RequestId, Arc<AtomicBool>>,
    jobs: SyncSender<Job>,
    completions: Receiver<Completion>,
}

impl BookSource {
    pub fn new(rows: usize) -> Result<Self, GridError> {
        if rows > MAX_DEMO_ROWS {
            return Err(GridError::Source(format!(
                "gallery supports at most {MAX_DEMO_ROWS} root rows"
            )));
        }
        let (jobs, work) = mpsc::sync_channel(QUEUE_CAPACITY);
        let (results, completions) = mpsc::sync_channel(QUEUE_CAPACITY);
        thread::Builder::new()
            .name("gallery-books".into())
            .spawn(move || {
                while let Ok(job) = work.recv() {
                    let completion = match job {
                        Job::Query {
                            epoch,
                            count,
                            query,
                            edits,
                            cancel,
                        } => build_query(count, &query, &edits, &cancel)
                            .map(|index| Completion::Query { epoch, index }),
                        Job::Children {
                            request,
                            rows,
                            cancel,
                        } => (!cancel.load(Ordering::Acquire))
                            .then_some(Completion::Children { request, rows }),
                    };
                    if let Some(completion) = completion
                        && results.send(completion).is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|error| GridError::Source(format!("cannot start books worker: {error}")))?;
        Ok(Self {
            count: rows,
            index: None,
            edits: HashMap::new(),
            query: GridQuery::default(),
            query_epoch: 0,
            query_cancel: None,
            pending_query: None,
            pending_children: HashMap::new(),
            jobs,
            completions,
        })
    }

    pub fn columns() -> Vec<GridColumn> {
        let names = [
            "Название",
            "Автор",
            "Год",
            "Жанр",
            "Язык",
            "Страниц",
            "Цена",
            "ISBN",
        ];
        (0..24)
            .map(|index| {
                let title = names
                    .get(index)
                    .map_or_else(|| format!("Признак {}", index - 7), |name| (*name).into());
                let mut column = GridColumn::new(
                    ColumnId(index as u64),
                    title,
                    match index {
                        0 => 320.0,
                        1 => 210.0,
                        7 => 180.0,
                        _ => 115.0,
                    },
                );
                column.pinned = index == 0;
                column.editable = index == 0 || index == 2;
                column
            })
            .collect()
    }

    pub fn root_rows(&self) -> usize {
        self.count
    }

    pub fn has_pending(&self) -> bool {
        self.query_cancel.is_some()
            || self.pending_query.is_some()
            || !self.pending_children.is_empty()
    }

    pub fn query_pending(&self) -> bool {
        self.query_cancel.is_some()
    }

    pub fn set_search(
        &mut self,
        grid: &mut TreeGrid,
        filter: impl Into<String>,
    ) -> Result<(), GridError> {
        let mut query = grid.query().clone();
        query.filter = filter.into();
        grid.apply_query(self, query)
    }

    /// Called from the event loop's timer only while `has_pending()` is true.
    /// A completion is accepted only for the current query/request generation.
    pub fn poll(&mut self, grid: &mut TreeGrid) -> Result<bool, GridError> {
        let mut changed = false;
        loop {
            match self.completions.try_recv() {
                Ok(Completion::Query { epoch, index }) => {
                    if epoch == self.query_epoch && self.query_cancel.is_some() {
                        self.index = Some(index);
                        self.query_cancel = None;
                        grid.refresh(self)?;
                        changed = true;
                    }
                }
                Ok(Completion::Children { request, rows }) => {
                    if let Some(cancel) = self.pending_children.remove(&request)
                        && !cancel.load(Ordering::Acquire)
                    {
                        changed |= grid.complete_children(self, request, Ok(rows))?;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if self.has_pending() {
                        return Err(GridError::Source("books worker disconnected".into()));
                    }
                    break;
                }
            }
        }
        if let Some(job) = self.pending_query.take() {
            match self.jobs.try_send(job) {
                Ok(()) => {}
                Err(TrySendError::Full(job)) => self.pending_query = Some(job),
                Err(TrySendError::Disconnected(_)) => {
                    return Err(GridError::Source("books worker disconnected".into()));
                }
            }
        }
        Ok(changed)
    }

    fn decode(&self, row: RowKey) -> Option<(usize, Option<usize>)> {
        if row.0 > 0 && row.0 <= self.count as u64 {
            return Some((row.0 as usize - 1, None));
        }
        let relative = row.0.checked_sub(self.count as u64 + 1)? as usize;
        let parent = relative / 3;
        let child = relative % 3;
        (parent < self.count && parent.is_multiple_of(LAZY_EVERY)).then_some((parent, Some(child)))
    }

    fn root_position(&self, index: usize) -> Option<usize> {
        match &self.index {
            None => (index < self.count).then_some(index),
            Some(query) => query
                .positions
                .get(index)
                .copied()
                .filter(|position| *position != usize::MAX),
        }
    }

    fn start_query(&mut self, query: GridQuery) -> Result<(), String> {
        if query.sort.iter().any(|sort| sort.column.0 >= 24) {
            return Err("unknown book column".into());
        }
        let epoch = self
            .query_epoch
            .checked_add(1)
            .ok_or("query identity capacity exceeded")?;
        if query == GridQuery::default() {
            if let Some(cancel) = self.query_cancel.take() {
                cancel.store(true, Ordering::Release);
            }
            self.pending_query = None;
            self.query_epoch = epoch;
            self.query = query;
            self.index = None;
            return Ok(());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let job = Job::Query {
            epoch,
            count: self.count,
            query: query.clone(),
            edits: self.edits.clone(),
            cancel: Arc::clone(&cancel),
        };
        let pending = match self.jobs.try_send(job) {
            Ok(()) => None,
            Err(TrySendError::Full(job)) => Some(job),
            Err(TrySendError::Disconnected(_)) => return Err("books worker disconnected".into()),
        };
        if let Some(cancel) = self.query_cancel.take() {
            cancel.store(true, Ordering::Release);
        }
        self.pending_query = pending;
        self.query_epoch = epoch;
        self.query = query;
        self.query_cancel = Some(cancel);
        Ok(())
    }
}

impl TreeDataSource for BookSource {
    fn child_count(&self, parent: Option<RowKey>) -> Option<usize> {
        match parent {
            None => Some(
                self.index
                    .as_ref()
                    .map_or(self.count, |index| index.order.len()),
            ),
            Some(row) => match self.decode(row) {
                Some((index, None)) if index.is_multiple_of(LAZY_EVERY) => None,
                _ => Some(0),
            },
        }
    }

    fn child_at(&self, parent: Option<RowKey>, index: usize) -> Option<RowKey> {
        match parent {
            None => {
                let index = match &self.index {
                    Some(query) => *query.order.get(index)?,
                    None if index < self.count => index,
                    _ => return None,
                };
                Some(RowKey(index as u64 + 1))
            }
            Some(row) => {
                let (parent, child) = self.decode(row)?;
                (child.is_none() && parent.is_multiple_of(LAZY_EVERY) && index < 3).then_some(
                    RowKey(self.count as u64 + parent as u64 * 3 + index as u64 + 1),
                )
            }
        }
    }

    fn parent(&self, row: RowKey) -> Option<RowKey> {
        let (parent, child) = self.decode(row)?;
        child.map(|_| RowKey(parent as u64 + 1))
    }

    fn position(&self, row: RowKey) -> Option<usize> {
        let (index, child) = self.decode(row)?;
        if let Some(child) = child {
            self.root_position(index).map(|_| child)
        } else {
            self.root_position(index)
        }
    }

    fn cell(&self, row: RowKey, column: ColumnId) -> String {
        if let Some(value) = self.edits.get(&(row, column)) {
            return value.clone();
        }
        let Some((index, child)) = self.decode(row) else {
            return String::new();
        };
        let value = generated_cell(index, column);
        if let Some(child) = child
            && column.0 == 0
        {
            format!("Экземпляр {} · {value}", child + 1)
        } else {
            value
        }
    }

    fn editable(&self, row: RowKey, column: ColumnId) -> bool {
        self.decode(row).is_some() && matches!(column.0, 0 | 2)
    }

    fn validate_edit(&self, row: RowKey, column: ColumnId, value: &str) -> Result<(), String> {
        if !self.editable(row, column) {
            return Err("Ячейка доступна только для чтения".into());
        }
        match column.0 {
            0 if value.trim().is_empty() => Err("Название не может быть пустым".into()),
            0 if value.chars().count() > 256 => {
                Err("Название должно быть короче 257 символов".into())
            }
            2 => match value.trim().parse::<u16>() {
                Ok(year) if (1400..=2100).contains(&year) => Ok(()),
                _ => Err("Год должен быть целым числом от 1400 до 2100".into()),
            },
            _ => Ok(()),
        }
    }

    fn set_cell(&mut self, row: RowKey, column: ColumnId, value: &str) -> Result<(), String> {
        self.validate_edit(row, column, value)?;
        let previous = self.edits.insert((row, column), value.trim().into());
        if self.query != GridQuery::default()
            && let Err(error) = self.start_query(self.query.clone())
        {
            if let Some(value) = previous {
                self.edits.insert((row, column), value);
            } else {
                self.edits.remove(&(row, column));
            }
            return Err(error);
        }
        Ok(())
    }

    fn apply_query(&mut self, query: &GridQuery) -> Result<(), String> {
        self.start_query(query.clone())
    }

    fn request_children(
        &mut self,
        parent: RowKey,
        request: RequestId,
    ) -> Result<ChildrenLoad, String> {
        let (index, child) = self.decode(parent).ok_or("unknown parent")?;
        if child.is_some() || !index.is_multiple_of(LAZY_EVERY) {
            return Ok(ChildrenLoad::Ready(Vec::new()));
        }
        let rows = (0..3)
            .map(|child| RowKey(self.count as u64 + index as u64 * 3 + child + 1))
            .collect();
        let cancel = Arc::new(AtomicBool::new(false));
        self.jobs
            .try_send(Job::Children {
                request,
                rows,
                cancel: Arc::clone(&cancel),
            })
            .map_err(|error| match error {
                TrySendError::Full(_) => "Очередь загрузки занята; повторите запрос",
                TrySendError::Disconnected(_) => "Источник данных недоступен",
            })?;
        self.pending_children.insert(request, cancel);
        Ok(ChildrenLoad::Pending)
    }

    fn cancel_request(&mut self, request: RequestId) {
        if let Some(cancel) = self.pending_children.remove(&request) {
            cancel.store(true, Ordering::Release);
        }
    }
}

impl Drop for BookSource {
    fn drop(&mut self) {
        if let Some(cancel) = &self.query_cancel {
            cancel.store(true, Ordering::Release);
        }
        for cancel in self.pending_children.values() {
            cancel.store(true, Ordering::Release);
        }
        // Dropping bounded channels wakes a blocked worker; no UI-thread join.
    }
}

fn generated_cell(index: usize, column: ColumnId) -> String {
    const TITLES: [&str; 12] = [
        "Война и мир",
        "Преступление и наказание",
        "Мастер и Маргарита",
        "Анна Каренина",
        "Записки путешественника",
        "История одного города",
        "Rust Patterns",
        "The Quiet Library",
        "Город и книги",
        "Applied Geometry",
        "Северный ветер",
        "Designing Interfaces",
    ];
    const AUTHORS: [&str; 8] = [
        "Лев Толстой",
        "Фёдор Достоевский",
        "Михаил Булгаков",
        "Анна Соколова",
        "John Reed",
        "Мария Волкова",
        "Alex Turner",
        "Иван Петров",
    ];
    const GENRES: [&str; 6] = [
        "Роман",
        "История",
        "Наука",
        "Приключения",
        "Технологии",
        "Поэзия",
    ];
    match column.0 {
        0 => format!("{} · {:07}", TITLES[index % TITLES.len()], index + 1),
        1 => AUTHORS[index % AUTHORS.len()].into(),
        2 => (1850 + index % 175).to_string(),
        3 => GENRES[index % GENRES.len()].into(),
        4 => if index.is_multiple_of(3) {
            "English"
        } else {
            "Русский"
        }
        .into(),
        5 => (80 + index.wrapping_mul(37) % 960).to_string(),
        6 => format!("{}.00", 200 + index.wrapping_mul(29) % 4800),
        7 => format!("978-5-{:07}-{}", index, index % 10),
        _ => format!("Метка {} / {}", column.0 - 7, index % 97),
    }
}

fn build_query(
    count: usize,
    query: &GridQuery,
    edits: &Edits,
    cancel: &AtomicBool,
) -> Option<QueryIndex> {
    let needle = query.filter.to_lowercase();
    let cell = |index: usize, column: ColumnId| {
        edits
            .get(&(RowKey(index as u64 + 1), column))
            .cloned()
            .unwrap_or_else(|| generated_cell(index, column))
    };
    let mut rows = Vec::new();
    for index in 0..count {
        if index.is_multiple_of(256) && cancel.load(Ordering::Acquire) {
            return None;
        }
        if needle.is_empty()
            || (0..8).any(|column| {
                cell(index, ColumnId(column))
                    .to_lowercase()
                    .contains(&needle)
            })
        {
            rows.push(index);
        }
    }
    // Cache keys once per filtered row. Sort comparisons do not generate text.
    let mut prepared = rows
        .into_iter()
        .map(|index| {
            let keys = query
                .sort
                .iter()
                .map(|sort| {
                    let value = cell(index, sort.column);
                    if matches!(sort.column.0, 2 | 5 | 6) {
                        SortValue::Number(value.parse::<f64>().unwrap_or(0.0))
                    } else {
                        SortValue::Text(value.to_lowercase())
                    }
                })
                .collect::<Vec<_>>();
            (index, keys)
        })
        .collect::<Vec<_>>();
    if cancel.load(Ordering::Acquire) {
        return None;
    }
    prepared.sort_unstable_by(|(left, left_keys), (right, right_keys)| {
        for (index, sort) in query.sort.iter().enumerate() {
            let comparison = left_keys[index].compare(&right_keys[index]);
            if comparison != CmpOrdering::Equal {
                return if sort.direction == SortDirection::Descending {
                    comparison.reverse()
                } else {
                    comparison
                };
            }
        }
        left.cmp(right)
    });
    if cancel.load(Ordering::Acquire) {
        return None;
    }
    let order = prepared
        .into_iter()
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let mut positions = vec![usize::MAX; count];
    for (position, &index) in order.iter().enumerate() {
        positions[index] = position;
    }
    Some(QueryIndex { order, positions })
}

enum SortValue {
    Number(f64),
    Text(String),
}
impl SortValue {
    fn compare(&self, other: &Self) -> CmpOrdering {
        match (self, other) {
            (Self::Number(a), Self::Number(b)) => a.total_cmp(b),
            (Self::Text(a), Self::Text(b)) => a.cmp(b),
            _ => CmpOrdering::Equal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_core::Rect;
    use rust_desktop_ui_treegrid::{SelectionModifiers, SortOrder};
    use std::time::{Duration, Instant};

    fn fixture(rows: usize) -> (BookSource, TreeGrid) {
        let mut source = BookSource::new(rows).unwrap();
        let mut grid = TreeGrid::new(BookSource::columns()).unwrap();
        grid.refresh(&mut source).unwrap();
        (source, grid)
    }
    fn settle(source: &mut BookSource, grid: &mut TreeGrid) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while source.has_pending() {
            source.poll(grid).unwrap();
            assert!(Instant::now() < deadline, "source worker timed out");
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn million_books_are_indexed_and_frames_read_only_visible_records() {
        let (source, mut grid) = fixture(1_000_000);
        assert_eq!(source.root_rows(), 1_000_000);
        assert!(source.index.is_none() && source.edits.is_empty());
        let frame = grid
            .paint(&source, Rect::new(0.0, 0.0, 1000.0, 600.0))
            .unwrap();
        assert!(frame.stats.materialized_rows <= 24);
        assert!(frame.stats.cells_read < 200);
        assert_eq!(source.cell(RowKey(1), ColumnId(0)), "Война и мир · 0000001");
        assert_eq!(
            source.cell(RowKey(7), ColumnId(0)),
            "Rust Patterns · 0000007"
        );
        assert_eq!(BookSource::columns().len(), 24);
    }

    #[test]
    fn background_filter_sort_and_edits_produce_a_consistent_index() {
        let (mut source, mut grid) = fixture(1_000);
        grid.select(&source, RowKey(1), SelectionModifiers::default())
            .unwrap();
        grid.apply_query(
            &mut source,
            GridQuery {
                filter: "война".into(),
                sort: vec![SortOrder {
                    column: ColumnId(2),
                    direction: SortDirection::Descending,
                }],
            },
        )
        .unwrap();
        settle(&mut source, &mut grid);
        let roots = source.child_count(None).unwrap();
        assert!(roots > 0 && roots < 1_000);
        let mut previous = u16::MAX;
        for index in 0..roots {
            let row = source.child_at(None, index).unwrap();
            assert_eq!(source.position(row), Some(index));
            assert!(
                source
                    .cell(row, ColumnId(0))
                    .to_lowercase()
                    .contains("война")
            );
            let year = source.cell(row, ColumnId(2)).parse::<u16>().unwrap();
            assert!(year <= previous);
            previous = year;
        }
        assert!(grid.is_selected(RowKey(1)));
        assert!(source.set_cell(RowKey(1), ColumnId(0), " ").is_err());
        assert!(source.set_cell(RowKey(1), ColumnId(2), "1399").is_err());
        source
            .set_cell(RowKey(1), ColumnId(0), "Новая книга")
            .unwrap();
        source.set_cell(RowKey(1), ColumnId(2), "2026").unwrap();
        settle(&mut source, &mut grid);
        assert_eq!(source.cell(RowKey(1), ColumnId(0)), "Новая книга");
        assert_eq!(source.cell(RowKey(1), ColumnId(2)), "2026");
        assert_eq!(source.position(RowKey(1)), None); // changed title no longer matches
        source.set_search(&mut grid, "новая").unwrap();
        settle(&mut source, &mut grid);
        assert_eq!(source.child_count(None), Some(1));
        assert_eq!(source.child_at(None, 0), Some(RowKey(1)));
    }

    #[test]
    fn failed_query_and_edit_leave_the_accepted_source_unchanged() {
        let (mut source, mut grid) = fixture(100);
        source.set_search(&mut grid, "война").unwrap();
        settle(&mut source, &mut grid);
        let previous_query = source.query.clone();
        let previous_cell = source.cell(RowKey(1), ColumnId(0));
        let (jobs, receiver) = mpsc::sync_channel(1);
        drop(receiver);
        source.jobs = jobs;
        assert!(source.set_search(&mut grid, "rust").is_err());
        assert_eq!(source.query, previous_query);
        assert!(
            source
                .set_cell(RowKey(1), ColumnId(0), "Other book")
                .is_err()
        );
        assert_eq!(source.cell(RowKey(1), ColumnId(0)), previous_cell);
        assert!(source.edits.is_empty());
        assert!(!source.has_pending());
    }

    #[test]
    fn stale_queries_and_cancelled_lazy_children_cannot_reappear() {
        let (mut source, mut grid) = fixture(100_000);
        source.set_search(&mut grid, "Война").unwrap();
        source.set_search(&mut grid, "never-a-book").unwrap();
        settle(&mut source, &mut grid);
        assert_eq!(source.child_count(None), Some(0));
        grid.apply_query(&mut source, GridQuery::default()).unwrap();
        let first = grid
            .set_expanded(&mut source, RowKey(1), true)
            .unwrap()
            .unwrap();
        grid.set_expanded(&mut source, RowKey(1), false).unwrap();
        assert!(
            !grid
                .complete_children(&source, first, Ok(vec![RowKey(100_001)]))
                .unwrap()
        );
        source.poll(&mut grid).unwrap();
        assert_eq!(grid.row_count(), 100_000);
        grid.set_expanded(&mut source, RowKey(1), true).unwrap();
        settle(&mut source, &mut grid);
        assert_eq!(grid.row_count(), 100_003);
        assert_eq!(
            grid.row(&source, 1).unwrap().unwrap().key,
            Some(RowKey(100_001))
        );
        assert_eq!(source.parent(RowKey(100_003)), Some(RowKey(1)));
        assert_eq!(source.position(RowKey(100_003)), Some(2));
    }
}
