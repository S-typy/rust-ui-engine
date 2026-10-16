use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashSet, VecDeque},
    fmt,
    rc::{Rc, Weak},
};

use crate::{CellValue, DataError, DataRow, DataTable, TableSchema};

#[path = "observable_list.rs"]
mod observable_list;
pub use observable_list::*;

/// Maximum number of operations in one atomic collection mutation.
pub const MAX_COLLECTION_BATCH_OPERATIONS: usize = 1024;
/// Incoming row/cell payload per batch, independent of retained table limits.
pub const MAX_COLLECTION_BATCH_BYTES: usize = 64 * 1024 * 1024;
/// Retained collection/source revisions. Consumers keep independent cursors.
pub const MAX_CHANGE_REVISIONS: usize = 256;
/// Retained change records, without copies of row or cell values.
pub const MAX_CHANGE_RECORDS: usize = 4096;
/// Maximum simultaneously registered wake callbacks per collection.
pub const MAX_COLLECTION_SUBSCRIPTIONS: usize = 1024;
/// Bound callback-triggered commits in one synchronous notification delivery.
pub const MAX_REENTRANT_COLLECTION_COMMITS: usize = 1024;

struct Observer {
    active: Cell<bool>,
    callback: Box<dyn Fn(u64)>,
}

/// A wake hint is separate from the journal: each consumer owns its cursor.
/// Callbacks are never invoked with a collection or registry borrow held.
#[derive(Default)]
pub(crate) struct NotificationHub {
    observers: RefCell<BTreeMap<u64, Rc<Observer>>>,
    next_id: Cell<u64>,
    dispatching: Cell<bool>,
    pending: Cell<Option<u64>>,
    reentrant_commits: Cell<usize>,
}

impl fmt::Debug for NotificationHub {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NotificationHub")
            .field("observers", &self.observers.borrow().len())
            .finish_non_exhaustive()
    }
}

impl NotificationHub {
    /// A deferred-update consumer already has a pending update once called.
    /// Reentrant wake hints can merge into that update without another callback
    /// round. Do not use this delivery mode for collection change callbacks,
    /// whose consumers may synchronously advance their own journal cursors.
    pub(crate) fn notify_coalesced(&self, revision: u64) {
        if !self.dispatching.get() {
            self.notify(revision);
        }
    }

    fn before_commit(&self) -> Result<(), DataError> {
        if self.dispatching.get() {
            let count = self.reentrant_commits.get();
            if count >= MAX_REENTRANT_COLLECTION_COMMITS {
                return Err(DataError::Capacity(
                    "reentrant collection commit count exceeded".into(),
                ));
            }
            self.reentrant_commits.set(count + 1);
        }
        Ok(())
    }
    pub(crate) fn subscribe(
        self: &Rc<Self>,
        callback: impl Fn(u64) + 'static,
    ) -> Result<CollectionSubscription, DataError> {
        let mut observers = self.observers.borrow_mut();
        if observers.len() >= MAX_COLLECTION_SUBSCRIPTIONS {
            return Err(DataError::Capacity(
                "collection subscription count exceeded".into(),
            ));
        }
        let id = self
            .next_id
            .get()
            .checked_add(1)
            .ok_or(DataError::RevisionOverflow)?;
        self.next_id.set(id);
        observers.insert(
            id,
            Rc::new(Observer {
                active: Cell::new(true),
                callback: Box::new(callback),
            }),
        );
        Ok(CollectionSubscription {
            hub: Rc::downgrade(self),
            id: Cell::new(Some(id)),
        })
    }

    fn unsubscribe(&self, id: u64) {
        // Drop the closure outside the borrow, including its captured tokens.
        let observer = self.observers.borrow_mut().remove(&id);
        if let Some(observer) = &observer {
            observer.active.set(false);
        }
        drop(observer);
    }

    pub(crate) fn notify(&self, revision: u64) {
        self.pending
            .set(Some(self.pending.get().unwrap_or(revision).max(revision)));
        if self.dispatching.replace(true) {
            return;
        }
        self.reentrant_commits.set(0);
        struct DispatchGuard<'a>(&'a Cell<bool>);
        impl Drop for DispatchGuard<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let _guard = DispatchGuard(&self.dispatching);
        let mut first_panic = None;
        while let Some(revision) = self.pending.take() {
            let observers: Vec<_> = self.observers.borrow().values().cloned().collect();
            for observer in observers {
                if observer.active.get() {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        (observer.callback)(revision)
                    }));
                    if let Err(payload) = result
                        && first_panic.is_none()
                    {
                        first_panic = Some(payload);
                    }
                }
            }
        }
        if let Some(payload) = first_panic {
            std::panic::resume_unwind(payload);
        }
    }
}

/// An owning registration, not an owning collection reference. Dropping this
/// token or calling `unsubscribe` immediately stops future callbacks. Keep it
/// alive for as long as its consumer needs wake notifications.
#[must_use = "dropping the subscription immediately unsubscribes its callback"]
#[derive(Debug)]
pub struct CollectionSubscription {
    hub: Weak<NotificationHub>,
    id: Cell<Option<u64>>,
}

impl CollectionSubscription {
    pub fn unsubscribe(&self) {
        if let (Some(id), Some(hub)) = (self.id.take(), self.hub.upgrade()) {
            hub.unsubscribe(id);
        }
    }
    pub fn is_active(&self) -> bool {
        self.id.get().is_some_and(|id| {
            self.hub
                .upgrade()
                .is_some_and(|hub| hub.observers.borrow().contains_key(&id))
        })
    }
}

impl Drop for CollectionSubscription {
    fn drop(&mut self) {
        self.unsubscribe();
    }
}

/// Metadata describing a committed change. Collection indices describe the
/// sequential operations in that batch. Source indices describe its query view.
/// `Reset` means the consumer must read a current snapshot rather than replay
/// any records in that set. `Query` is emitted only by a source changing its
/// local query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataChange {
    Add { key: i64, index: usize },
    Remove { key: i64, index: usize },
    Move { key: i64, from: usize, to: usize },
    Replace { key: i64, index: usize },
    Cell { key: i64, field: String },
    Reset,
    Query,
}

/// Changes after a consumer's cursor through the current revision. An expired
/// or future cursor yields a single `Reset`; an up-to-date cursor yields none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataChangeSet {
    pub from_revision: u64,
    pub revision: u64,
    pub changes: Vec<DataChange>,
}

#[derive(Debug, Default)]
pub(crate) struct ChangeJournal {
    entries: VecDeque<(u64, Vec<DataChange>)>,
    records: usize,
}

impl ChangeJournal {
    pub(crate) fn push(&mut self, revision: u64, mut changes: Vec<DataChange>) {
        if changes.len() > MAX_CHANGE_RECORDS {
            changes = vec![DataChange::Reset];
        }
        self.records += changes.len();
        self.entries.push_back((revision, changes));
        while self.entries.len() > MAX_CHANGE_REVISIONS || self.records > MAX_CHANGE_RECORDS {
            if let Some((_, changes)) = self.entries.pop_front() {
                self.records -= changes.len();
            }
        }
    }

    pub(crate) fn since(&self, cursor: u64, revision: u64) -> DataChangeSet {
        let changes = if cursor == revision {
            Vec::new()
        } else if cursor > revision
            || self
                .entries
                .front()
                .is_none_or(|(first, _)| cursor < first - 1)
        {
            vec![DataChange::Reset]
        } else {
            self.entries
                .iter()
                .filter(|(at, _)| *at > cursor)
                .flat_map(|(_, changes)| changes.iter().cloned())
                .collect()
        };
        DataChangeSet {
            from_revision: cursor,
            revision,
            changes,
        }
    }
}

/// Operations are applied in order to a private candidate and published once.
/// `Move.index` is the final zero-based position after removing that row.
/// Row replacement keeps its identity; field edits enforce `read_only`.
/// App-owned row replacement/reset can populate read-only display fields.
#[derive(Clone, Debug, PartialEq)]
pub enum CollectionMutation {
    Add(DataRow),
    Insert {
        index: usize,
        row: DataRow,
    },
    Remove {
        key: i64,
    },
    Move {
        key: i64,
        index: usize,
    },
    Replace {
        key: i64,
        row: DataRow,
    },
    UpdateCell {
        key: i64,
        field: String,
        value: CellValue,
    },
    UpdateCellExpected {
        key: i64,
        field: String,
        expected: CellValue,
        value: CellValue,
    },
    Clear,
    Reset(Vec<DataRow>),
}

#[derive(Debug)]
struct CollectionState {
    table: DataTable,
    revision: u64,
    journal: ChangeJournal,
}

/// Cloneable application-owned collection with atomic, bounded notifications.
/// Mutations, cursors and callbacks run on the owning thread. Reads do not run
/// callbacks, workers or I/O. Capacity limits are fixed at construction; schema
/// changes require the explicit atomic `replace_schema` operation.
#[derive(Clone, Debug)]
pub struct ObservableCollection {
    state: Rc<RefCell<CollectionState>>,
    notifications: Rc<NotificationHub>,
}

/// Non-owning handle suitable for capture by a collection's own callback.
#[derive(Clone, Debug)]
pub struct WeakObservableCollection {
    state: Weak<RefCell<CollectionState>>,
    notifications: Weak<NotificationHub>,
}

impl WeakObservableCollection {
    pub fn upgrade(&self) -> Option<ObservableCollection> {
        Some(ObservableCollection {
            state: self.state.upgrade()?,
            notifications: self.notifications.upgrade()?,
        })
    }
}

/// Compatibility name for the application-owned observable collection handle.
pub type CollectionHandle = ObservableCollection;

impl ObservableCollection {
    pub fn new(table: DataTable) -> Self {
        Self {
            state: Rc::new(RefCell::new(CollectionState {
                table,
                revision: 0,
                journal: ChangeJournal::default(),
            })),
            notifications: Rc::new(NotificationHub::default()),
        }
    }
    pub fn downgrade(&self) -> WeakObservableCollection {
        WeakObservableCollection {
            state: Rc::downgrade(&self.state),
            notifications: Rc::downgrade(&self.notifications),
        }
    }
    /// Subscribe to committed revisions. There is no initial callback. The
    /// callback is a wake hint; use `changes_since` with your own cursor for
    /// changes. Reentrant mutations commit immediately and queue another round
    /// after the current observer round; multiple such commits coalesce into
    /// the latest revision. Subscriptions added during a round start with the
    /// next round, and removal takes effect immediately. Callback-triggered
    /// writes are limited to `MAX_REENTRANT_COLLECTION_COMMITS` per delivery;
    /// further writes fail atomically with `DataError::Capacity`, preventing
    /// unbounded feedback loops. Handle mutation errors inside callbacks.
    /// A callback panic is resumed after notifying other observers; the data
    /// remains committed and subsequent notifications remain usable.
    pub fn subscribe(
        &self,
        callback: impl Fn(u64) + 'static,
    ) -> Result<CollectionSubscription, DataError> {
        self.notifications.subscribe(callback)
    }
    pub fn snapshot(&self) -> DataTable {
        self.state.borrow().table.clone()
    }
    pub fn revision(&self) -> u64 {
        self.state.borrow().revision
    }
    pub fn changes_since(&self, cursor: u64) -> DataChangeSet {
        let state = self.state.borrow();
        state.journal.since(cursor, state.revision)
    }
    pub fn add(&self, row: DataRow) -> Result<bool, DataError> {
        self.batch(vec![CollectionMutation::Add(row)])
    }
    pub fn insert(&self, index: usize, row: DataRow) -> Result<bool, DataError> {
        self.batch(vec![CollectionMutation::Insert { index, row }])
    }
    pub fn remove(&self, key: i64) -> Result<bool, DataError> {
        self.batch(vec![CollectionMutation::Remove { key }])
    }
    pub fn move_row(&self, key: i64, index: usize) -> Result<bool, DataError> {
        self.batch(vec![CollectionMutation::Move { key, index }])
    }
    pub fn replace_row(&self, key: i64, row: DataRow) -> Result<bool, DataError> {
        self.batch(vec![CollectionMutation::Replace { key, row }])
    }
    pub fn update_cell(&self, key: i64, field: &str, value: CellValue) -> Result<bool, DataError> {
        self.batch(vec![CollectionMutation::UpdateCell {
            key,
            field: field.into(),
            value,
        }])
    }
    pub fn update_cell_expected(
        &self,
        key: i64,
        field: &str,
        expected: &CellValue,
        value: CellValue,
    ) -> Result<bool, DataError> {
        self.batch(vec![CollectionMutation::UpdateCellExpected {
            key,
            field: field.into(),
            expected: expected.clone(),
            value,
        }])
    }
    /// Adapter writes return their own accepted snapshot/cursor even when a
    /// subscriber immediately performs another mutation during notification.
    pub(crate) fn update_cell_expected_snapshot(
        &self,
        key: i64,
        field: &str,
        expected: &CellValue,
        value: CellValue,
    ) -> Result<(DataTable, u64), DataError> {
        let (_, snapshot, revision) = self.batch_inner(
            vec![CollectionMutation::UpdateCellExpected {
                key,
                field: field.into(),
                expected: expected.clone(),
                value,
            }],
            true,
        )?;
        Ok((
            snapshot.expect("adapter write requested a snapshot"),
            revision,
        ))
    }
    /// Capture before callbacks, preserving pending reentrant changes for the
    /// adapter's next synchronize boundary.
    pub(crate) fn write_batch_snapshot(
        &self,
        operations: &[crate::DataWrite],
    ) -> Result<(DataTable, u64), DataError> {
        let mut state = self.state.borrow_mut();
        let table = crate::write::apply_writes(&state.table, operations)?;
        if table == state.table {
            return Ok((table, state.revision));
        }
        let revision = state
            .revision
            .checked_add(1)
            .ok_or(DataError::RevisionOverflow)?;
        self.notifications.before_commit()?;
        state.table = table.clone();
        state.revision = revision;
        state.journal.push(revision, vec![DataChange::Reset]);
        drop(state);
        self.notifications.notify(revision);
        Ok((table, revision))
    }
    pub fn clear(&self) -> Result<bool, DataError> {
        self.batch(vec![CollectionMutation::Clear])
    }
    pub fn reset(&self, rows: Vec<DataRow>) -> Result<bool, DataError> {
        self.batch(vec![CollectionMutation::Reset(rows)])
    }
    pub fn replace(&self, table: DataTable) -> Result<bool, DataError> {
        if self.state.borrow().table.schema() != table.schema() {
            return Err(DataError::Schema(
                "collection replacement changed its schema".into(),
            ));
        }
        // The receiving collection's limits, not the caller's, remain in force.
        self.reset(table.rows().to_vec())
    }

    /// Replace schema and rows as one validated reset, preserving this
    /// collection's limits. No implicit conversion or partial migration occurs.
    pub fn replace_schema(
        &self,
        schema: TableSchema,
        rows: Vec<DataRow>,
    ) -> Result<bool, DataError> {
        let mut state = self.state.borrow_mut();
        let table = DataTable::with_limits(schema, rows, state.table.limits())?;
        if state.table == table {
            return Ok(false);
        }
        let revision = state
            .revision
            .checked_add(1)
            .ok_or(DataError::RevisionOverflow)?;
        self.notifications.before_commit()?;
        state.table = table;
        state.revision = revision;
        state.journal.push(revision, vec![DataChange::Reset]);
        drop(state);
        self.notifications.notify(revision);
        Ok(true)
    }

    /// Publishes all operations or none. Empty and net-unchanged batches do not
    /// increment the revision. Each intermediate row count and incoming payload
    /// are bounded; the final candidate validates types, keys and total bytes.
    pub fn batch(&self, operations: Vec<CollectionMutation>) -> Result<bool, DataError> {
        self.batch_inner(operations, false)
            .map(|(changed, _, _)| changed)
    }

    fn batch_inner(
        &self,
        operations: Vec<CollectionMutation>,
        capture: bool,
    ) -> Result<(bool, Option<DataTable>, u64), DataError> {
        if operations.len() > MAX_COLLECTION_BATCH_OPERATIONS {
            return Err(DataError::Capacity(
                "collection batch operation count exceeded".into(),
            ));
        }
        if operations.is_empty() {
            let state = self.state.borrow();
            return Ok((false, capture.then(|| state.table.clone()), state.revision));
        }
        let mut state = self.state.borrow_mut();
        validate_payload(&state.table, &operations)?;
        // A leading replacement discards the original content; do not clone it.
        let replaces_all = matches!(
            operations.first(),
            Some(CollectionMutation::Clear | CollectionMutation::Reset(_))
        );
        let mut rows = if replaces_all {
            Vec::new()
        } else {
            state.table.rows().to_vec()
        };
        let mut changes = Vec::new();
        if replaces_all && !state.table.rows().is_empty() {
            changes.push(DataChange::Reset);
        }
        for operation in operations {
            match operation {
                CollectionMutation::Add(row) => {
                    let index = rows.len();
                    insert_row(&state.table, &mut rows, index, row, &mut changes)?;
                }
                CollectionMutation::Insert { index, row } => {
                    insert_row(&state.table, &mut rows, index, row, &mut changes)?;
                }
                CollectionMutation::Remove { key } => {
                    let index = row_index(&rows, key)?;
                    rows.remove(index);
                    changes.push(DataChange::Remove { key, index });
                }
                CollectionMutation::Move { key, index } => {
                    let from = row_index(&rows, key)?;
                    if index >= rows.len() {
                        return Err(DataError::Validation(
                            "collection move index out of bounds".into(),
                        ));
                    }
                    if from != index {
                        let row = rows.remove(from);
                        rows.insert(index, row);
                        changes.push(DataChange::Move {
                            key,
                            from,
                            to: index,
                        });
                    }
                }
                CollectionMutation::Replace { key, row } => {
                    let index = row_index(&rows, key)?;
                    if row.key != key {
                        return Err(DataError::Validation(
                            "row replacement changed its stable key".into(),
                        ));
                    }
                    if rows[index] != row {
                        rows[index] = row;
                        changes.push(DataChange::Replace { key, index });
                    }
                }
                CollectionMutation::UpdateCell { key, field, value } => {
                    edit_cell(
                        &state.table,
                        &mut rows,
                        key,
                        field,
                        None,
                        value,
                        &mut changes,
                    )?;
                }
                CollectionMutation::UpdateCellExpected {
                    key,
                    field,
                    expected,
                    value,
                } => {
                    edit_cell(
                        &state.table,
                        &mut rows,
                        key,
                        field,
                        Some(expected),
                        value,
                        &mut changes,
                    )?;
                }
                CollectionMutation::Clear => {
                    if !rows.is_empty() {
                        rows.clear();
                        changes.push(DataChange::Reset);
                    }
                }
                CollectionMutation::Reset(next) => {
                    if rows != next {
                        rows = next;
                        changes.push(DataChange::Reset);
                    }
                }
            }
        }
        // No-op batches do not rebuild the table index or revalidate every cell.
        if state.table.rows() == rows {
            return Ok((false, capture.then(|| state.table.clone()), state.revision));
        }
        let table =
            DataTable::with_limits(state.table.schema().clone(), rows, state.table.limits())?;
        let revision = state
            .revision
            .checked_add(1)
            .ok_or(DataError::RevisionOverflow)?;
        self.notifications.before_commit()?;
        state.table = table;
        state.revision = revision;
        state.journal.push(revision, changes);
        let snapshot = capture.then(|| state.table.clone());
        drop(state);
        self.notifications.notify(revision);
        Ok((true, snapshot, revision))
    }
}

fn row_index(rows: &[DataRow], key: i64) -> Result<usize, DataError> {
    rows.iter()
        .position(|row| row.key == key)
        .ok_or(DataError::UnknownRow(key))
}

fn insert_row(
    table: &DataTable,
    rows: &mut Vec<DataRow>,
    index: usize,
    row: DataRow,
    changes: &mut Vec<DataChange>,
) -> Result<(), DataError> {
    if index > rows.len() {
        return Err(DataError::Validation(
            "collection insert index out of bounds".into(),
        ));
    }
    if rows.len() >= table.limits().max_rows {
        return Err(DataError::Capacity("collection row count exceeded".into()));
    }
    if rows.iter().any(|current| current.key == row.key) {
        return Err(DataError::Validation("duplicate collection row key".into()));
    }
    changes.push(DataChange::Add {
        key: row.key,
        index,
    });
    rows.insert(index, row);
    Ok(())
}

fn edit_cell(
    table: &DataTable,
    rows: &mut [DataRow],
    key: i64,
    field: String,
    expected: Option<CellValue>,
    value: CellValue,
    changes: &mut Vec<DataChange>,
) -> Result<(), DataError> {
    let column = table
        .schema()
        .field_index(&field)
        .ok_or_else(|| DataError::UnknownField(field.clone()))?;
    let schema = &table.schema().fields()[column];
    if schema.read_only {
        return Err(DataError::ReadOnly(field));
    }
    schema.validate(&value)?;
    let index = row_index(rows, key)?;
    let current = &mut rows[index].values[column];
    if expected.is_some_and(|expected| *current != expected) {
        return Err(DataError::Conflict(
            "collection value changed since editing began".into(),
        ));
    }
    if *current != value {
        *current = value;
        changes.push(DataChange::Cell { key, field });
    }
    Ok(())
}

fn validate_payload(table: &DataTable, operations: &[CollectionMutation]) -> Result<(), DataError> {
    let mut bytes = 0usize;
    for operation in operations {
        match operation {
            CollectionMutation::Add(row)
            | CollectionMutation::Insert { row, .. }
            | CollectionMutation::Replace { row, .. } => {
                validate_row(table, row)?;
                charge_payload(&mut bytes, size_of::<DataRow>())?;
                for cell in &row.values {
                    charge_value(table, &mut bytes, cell)?;
                }
            }
            CollectionMutation::Reset(rows) => {
                if rows.len() > table.limits().max_rows {
                    return Err(DataError::Capacity(
                        "collection reset row count exceeded".into(),
                    ));
                }
                let mut keys = HashSet::with_capacity(rows.len());
                for row in rows {
                    if !keys.insert(row.key) {
                        return Err(DataError::Validation(
                            "duplicate collection reset row key".into(),
                        ));
                    }
                    validate_row(table, row)?;
                    charge_payload(&mut bytes, size_of::<DataRow>())?;
                    for cell in &row.values {
                        charge_value(table, &mut bytes, cell)?;
                    }
                }
            }
            CollectionMutation::UpdateCell {
                field, value: cell, ..
            } => {
                validate_field(table, field)?;
                charge_payload(&mut bytes, field.len())?;
                charge_value(table, &mut bytes, cell)?;
            }
            CollectionMutation::UpdateCellExpected {
                field,
                expected,
                value: cell,
                ..
            } => {
                validate_field(table, field)?;
                charge_payload(&mut bytes, field.len())?;
                charge_value(table, &mut bytes, expected)?;
                charge_value(table, &mut bytes, cell)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn charge_payload(bytes: &mut usize, amount: usize) -> Result<(), DataError> {
    *bytes = bytes.saturating_add(amount);
    if *bytes > MAX_COLLECTION_BATCH_BYTES {
        Err(DataError::Capacity(
            "collection batch payload exceeded".into(),
        ))
    } else {
        Ok(())
    }
}

fn charge_value(table: &DataTable, bytes: &mut usize, value: &CellValue) -> Result<(), DataError> {
    let payload_bytes = match value {
        CellValue::Text(text) => {
            if text.len() > table.limits().max_text_bytes {
                return Err(DataError::Capacity("collection cell text exceeded".into()));
            }
            text.len()
        }
        CellValue::Binary(value) => {
            if value.len() > table.limits().max_binary_bytes {
                return Err(DataError::Capacity(
                    "collection cell binary exceeded".into(),
                ));
            }
            value.len()
        }
        _ => 0,
    };
    charge_payload(bytes, size_of::<CellValue>().saturating_add(payload_bytes))
}

fn validate_row(table: &DataTable, row: &DataRow) -> Result<(), DataError> {
    if row.values.len() != table.schema().fields().len() {
        return Err(DataError::Validation(
            "collection row width does not match schema".into(),
        ));
    }
    for (field, value) in table.schema().fields().iter().zip(&row.values) {
        field.validate(value)?;
    }
    Ok(())
}

fn validate_field(table: &DataTable, field: &str) -> Result<(), DataError> {
    if field.len() > 256 {
        return Err(DataError::Capacity("collection field name exceeded".into()));
    }
    table
        .schema()
        .field_index(field)
        .map(|_| ())
        .ok_or_else(|| DataError::UnknownField(field.into()))
}
