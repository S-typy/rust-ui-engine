use std::{
    cell::RefCell,
    collections::HashSet,
    fmt,
    rc::{Rc, Weak},
};

use super::{
    ChangeJournal, CollectionSubscription, DataChange, DataChangeSet, MAX_COLLECTION_BATCH_BYTES,
    MAX_COLLECTION_BATCH_OPERATIONS, NotificationHub,
};
use crate::DataError;

/// Limits for a generic collection. Heap payload is measured by the function
/// supplied at construction, in addition to `size_of::<T>()` and entry metadata.
/// Allocator overhead and independently retained snapshots are not included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListLimits {
    pub max_items: usize,
    pub max_item_bytes: usize,
    pub max_bytes: usize,
}

impl Default for ListLimits {
    fn default() -> Self {
        Self {
            max_items: 100_000,
            max_item_bytes: 1024 * 1024,
            max_bytes: 64 * 1024 * 1024,
        }
    }
}

impl ListLimits {
    pub fn validate(self) -> Result<(), DataError> {
        let hard = Self::default();
        if self.max_items > hard.max_items
            || self.max_item_bytes > hard.max_item_bytes
            || self.max_bytes > hard.max_bytes
        {
            return Err(DataError::Capacity(
                "generic collection limits exceed hard bounds".into(),
            ));
        }
        Ok(())
    }
}

/// An immutable, shared snapshot item. Its identity does not depend on position.
/// Cloning the item clones its reference, not its payload.
#[derive(Debug)]
pub struct ObservableItem<T> {
    key: i64,
    value: Rc<T>,
    bytes: usize,
}

impl<T> Clone for ObservableItem<T> {
    fn clone(&self) -> Self {
        Self {
            key: self.key,
            value: self.value.clone(),
            bytes: self.bytes,
        }
    }
}

impl<T: PartialEq> PartialEq for ObservableItem<T> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.value == other.value
    }
}

impl<T> ObservableItem<T> {
    pub fn key(&self) -> i64 {
        self.key
    }
    pub fn value(&self) -> &T {
        &self.value
    }
    pub fn shared_value(&self) -> Rc<T> {
        self.value.clone()
    }
}

/// Explicit mutations over owned Rust values. Replacing a value keeps its key.
/// Use replacement to publish an object's field changes as a single revision.
#[derive(Clone, Debug, PartialEq)]
pub enum ListMutation<T> {
    Add { key: i64, value: T },
    Insert { index: usize, key: i64, value: T },
    Remove { key: i64 },
    Move { key: i64, index: usize },
    Replace { key: i64, value: T },
    Clear,
    Reset(Vec<(i64, T)>),
}

enum Prepared<T> {
    Add(ObservableItem<T>),
    Insert(usize, ObservableItem<T>),
    Remove(i64),
    Move(i64, usize),
    Replace(ObservableItem<T>),
    Clear,
    Reset(Vec<ObservableItem<T>>),
}

#[derive(Debug)]
struct ListState<T> {
    items: Vec<ObservableItem<T>>,
    revision: u64,
    journal: ChangeJournal,
}

/// Observable, bounded, application-owned Rust values without a table schema.
/// The collection owns values and only exposes shared references; all changes
/// must go through its methods. Interior mutation through `Cell`, `RefCell`,
/// atomics or external aliases is not observed and must not change the measured
/// payload. Replace the value instead. An arbitrary external `Vec<T>` cannot
/// be intercepted by this API.
pub struct ObservableList<T> {
    state: Rc<RefCell<ListState<T>>>,
    notifications: Rc<NotificationHub>,
    limits: ListLimits,
    heap_bytes: Rc<dyn Fn(&T) -> usize>,
}

impl<T> Clone for ObservableList<T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            notifications: self.notifications.clone(),
            limits: self.limits,
            heap_bytes: self.heap_bytes.clone(),
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for ObservableList<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ObservableList")
            .field("state", &self.state)
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

/// Non-owning generic collection handle for subscription closures.
pub struct WeakObservableList<T> {
    state: Weak<RefCell<ListState<T>>>,
    notifications: Weak<NotificationHub>,
    limits: ListLimits,
    heap_bytes: Weak<dyn Fn(&T) -> usize>,
}

impl<T> Clone for WeakObservableList<T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            notifications: self.notifications.clone(),
            limits: self.limits,
            heap_bytes: self.heap_bytes.clone(),
        }
    }
}

impl<T> WeakObservableList<T> {
    pub fn upgrade(&self) -> Option<ObservableList<T>> {
        Some(ObservableList {
            state: self.state.upgrade()?,
            notifications: self.notifications.upgrade()?,
            limits: self.limits,
            heap_bytes: self.heap_bytes.upgrade()?,
        })
    }
}

impl<T: PartialEq> ObservableList<T> {
    /// `heap_bytes` must deterministically account for all heap bytes owned by
    /// a value, without mutating application state. Return zero for scalars and
    /// `String::len` for strings. Inline value bytes are counted automatically.
    pub fn new(
        items: Vec<(i64, T)>,
        heap_bytes: impl Fn(&T) -> usize + 'static,
    ) -> Result<Self, DataError> {
        Self::with_limits(items, ListLimits::default(), heap_bytes)
    }

    pub fn with_limits(
        items: Vec<(i64, T)>,
        limits: ListLimits,
        heap_bytes: impl Fn(&T) -> usize + 'static,
    ) -> Result<Self, DataError> {
        limits.validate()?;
        if items.len() > limits.max_items {
            return Err(DataError::Capacity(
                "generic collection item count exceeded".into(),
            ));
        }
        let list = Self {
            state: Rc::new(RefCell::new(ListState {
                items: Vec::new(),
                revision: 0,
                journal: ChangeJournal::default(),
            })),
            notifications: Rc::new(NotificationHub::default()),
            limits,
            heap_bytes: Rc::new(heap_bytes),
        };
        let mut bytes = 0;
        let items = items
            .into_iter()
            .map(|(key, value)| list.prepare_item(key, value, &mut bytes))
            .collect::<Result<Vec<_>, _>>()?;
        list.validate_items(&items)?;
        list.state.borrow_mut().items = items;
        Ok(list)
    }

    pub fn downgrade(&self) -> WeakObservableList<T> {
        WeakObservableList {
            state: Rc::downgrade(&self.state),
            notifications: Rc::downgrade(&self.notifications),
            limits: self.limits,
            heap_bytes: Rc::downgrade(&self.heap_bytes),
        }
    }
    pub fn limits(&self) -> ListLimits {
        self.limits
    }
    pub fn len(&self) -> usize {
        self.state.borrow().items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn revision(&self) -> u64 {
        self.state.borrow().revision
    }
    /// Snapshot references remain valid after mutation or owner destruction.
    pub fn snapshot(&self) -> Vec<ObservableItem<T>> {
        self.state.borrow().items.clone()
    }
    pub fn get(&self, key: i64) -> Option<Rc<T>> {
        self.state
            .borrow()
            .items
            .iter()
            .find(|item| item.key == key)
            .map(|item| item.value.clone())
    }
    pub fn changes_since(&self, cursor: u64) -> DataChangeSet {
        let state = self.state.borrow();
        state.journal.since(cursor, state.revision)
    }
    /// Same wake, unsubscribe and reentrancy contract as `ObservableCollection`.
    pub fn subscribe(
        &self,
        callback: impl Fn(u64) + 'static,
    ) -> Result<CollectionSubscription, DataError> {
        self.notifications.subscribe(callback)
    }
    pub fn add(&self, key: i64, value: T) -> Result<bool, DataError> {
        self.batch(vec![ListMutation::Add { key, value }])
    }
    pub fn insert(&self, index: usize, key: i64, value: T) -> Result<bool, DataError> {
        self.batch(vec![ListMutation::Insert { index, key, value }])
    }
    pub fn remove(&self, key: i64) -> Result<bool, DataError> {
        self.batch(vec![ListMutation::Remove { key }])
    }
    pub fn move_item(&self, key: i64, index: usize) -> Result<bool, DataError> {
        self.batch(vec![ListMutation::Move { key, index }])
    }
    pub fn replace(&self, key: i64, value: T) -> Result<bool, DataError> {
        self.batch(vec![ListMutation::Replace { key, value }])
    }
    pub fn clear(&self) -> Result<bool, DataError> {
        self.batch(vec![ListMutation::Clear])
    }
    pub fn reset(&self, items: Vec<(i64, T)>) -> Result<bool, DataError> {
        self.batch(vec![ListMutation::Reset(items)])
    }

    fn prepare_item(
        &self,
        key: i64,
        value: T,
        batch_bytes: &mut usize,
    ) -> Result<ObservableItem<T>, DataError> {
        let value_bytes = size_of::<T>().saturating_add((self.heap_bytes)(&value));
        if value_bytes > self.limits.max_item_bytes {
            return Err(DataError::Capacity(
                "generic collection item bytes exceeded".into(),
            ));
        }
        let bytes = value_bytes.saturating_add(size_of::<ObservableItem<T>>());
        *batch_bytes = batch_bytes.saturating_add(bytes);
        if *batch_bytes > MAX_COLLECTION_BATCH_BYTES {
            return Err(DataError::Capacity(
                "generic collection batch bytes exceeded".into(),
            ));
        }
        Ok(ObservableItem {
            key,
            value: Rc::new(value),
            bytes,
        })
    }

    fn validate_items(&self, items: &[ObservableItem<T>]) -> Result<(), DataError> {
        if items.len() > self.limits.max_items {
            return Err(DataError::Capacity(
                "generic collection item count exceeded".into(),
            ));
        }
        let mut keys = HashSet::with_capacity(items.len());
        let mut bytes = 0usize;
        for item in items {
            if !keys.insert(item.key) {
                return Err(DataError::Validation(
                    "duplicate generic collection key".into(),
                ));
            }
            bytes = bytes.saturating_add(item.bytes);
            if bytes > self.limits.max_bytes {
                return Err(DataError::Capacity(
                    "generic collection retained bytes exceeded".into(),
                ));
            }
        }
        Ok(())
    }

    /// Publish the complete candidate once, or leave values, revision, journal
    /// and subscribers unchanged. Candidate/snapshot copying shares payloads;
    /// structural operations still take linear time in the item count.
    pub fn batch(&self, operations: Vec<ListMutation<T>>) -> Result<bool, DataError> {
        if operations.len() > MAX_COLLECTION_BATCH_OPERATIONS {
            return Err(DataError::Capacity(
                "generic collection batch count exceeded".into(),
            ));
        }
        let (base_revision, baseline) = {
            let state = self.state.borrow();
            (state.revision, state.items.clone())
        };
        let mut bytes = 0;
        // Execute application measurement without a live state borrow.
        let operations = operations
            .into_iter()
            .map(|operation| {
                Ok(match operation {
                    ListMutation::Add { key, value } => {
                        Prepared::Add(self.prepare_item(key, value, &mut bytes)?)
                    }
                    ListMutation::Insert { index, key, value } => {
                        Prepared::Insert(index, self.prepare_item(key, value, &mut bytes)?)
                    }
                    ListMutation::Remove { key } => Prepared::Remove(key),
                    ListMutation::Move { key, index } => Prepared::Move(key, index),
                    ListMutation::Replace { key, value } => {
                        Prepared::Replace(self.prepare_item(key, value, &mut bytes)?)
                    }
                    ListMutation::Clear => Prepared::Clear,
                    ListMutation::Reset(values) => {
                        if values.len() > self.limits.max_items {
                            return Err(DataError::Capacity(
                                "generic collection reset count exceeded".into(),
                            ));
                        }
                        let items = values
                            .into_iter()
                            .map(|(key, value)| self.prepare_item(key, value, &mut bytes))
                            .collect::<Result<Vec<_>, _>>()?;
                        self.validate_items(&items)?;
                        Prepared::Reset(items)
                    }
                })
            })
            .collect::<Result<Vec<_>, DataError>>()?;
        if operations.is_empty() {
            return Ok(false);
        }
        let replaces_all = matches!(
            operations.first(),
            Some(Prepared::Clear | Prepared::Reset(_))
        );
        let mut items = if replaces_all {
            Vec::new()
        } else {
            baseline.clone()
        };
        let mut changes = Vec::new();
        if replaces_all && !baseline.is_empty() {
            changes.push(DataChange::Reset);
        }
        for operation in operations {
            match operation {
                Prepared::Add(item) => {
                    let index = items.len();
                    self.insert_item(&mut items, index, item, &mut changes)?;
                }
                Prepared::Insert(index, item) => {
                    self.insert_item(&mut items, index, item, &mut changes)?;
                }
                Prepared::Remove(key) => {
                    let index = find_item(&items, key)?;
                    items.remove(index);
                    changes.push(DataChange::Remove { key, index });
                }
                Prepared::Move(key, index) => {
                    let from = find_item(&items, key)?;
                    if index >= items.len() {
                        return Err(DataError::Validation(
                            "generic collection move index out of bounds".into(),
                        ));
                    }
                    if from != index {
                        let item = items.remove(from);
                        items.insert(index, item);
                        changes.push(DataChange::Move {
                            key,
                            from,
                            to: index,
                        });
                    }
                }
                Prepared::Replace(item) => {
                    let index = find_item(&items, item.key)?;
                    if items[index] != item {
                        changes.push(DataChange::Replace {
                            key: item.key,
                            index,
                        });
                        items[index] = item;
                    }
                }
                Prepared::Clear => {
                    if !items.is_empty() {
                        items.clear();
                        changes.push(DataChange::Reset);
                    }
                }
                Prepared::Reset(next) => {
                    if items != next {
                        items = next;
                        changes.push(DataChange::Reset);
                    }
                }
            }
        }
        // T::eq and T::drop are application code too; candidate preparation,
        // comparison and rollback must not keep a live collection borrow.
        let unchanged = baseline == items;
        self.validate_items(&items)?;
        let mut state = self.state.borrow_mut();
        if state.revision != base_revision {
            return Err(DataError::Conflict(
                "generic collection changed during value comparison or measurement".into(),
            ));
        }
        if unchanged {
            return Ok(false);
        }
        let revision = state
            .revision
            .checked_add(1)
            .ok_or(DataError::RevisionOverflow)?;
        self.notifications.before_commit()?;
        let previous = std::mem::replace(&mut state.items, items);
        state.revision = revision;
        state.journal.push(revision, changes);
        drop(state);
        self.notifications.notify(revision);
        drop(previous);
        Ok(true)
    }

    fn insert_item(
        &self,
        items: &mut Vec<ObservableItem<T>>,
        index: usize,
        item: ObservableItem<T>,
        changes: &mut Vec<DataChange>,
    ) -> Result<(), DataError> {
        if index > items.len() {
            return Err(DataError::Validation(
                "generic collection insert index out of bounds".into(),
            ));
        }
        if items.len() >= self.limits.max_items {
            return Err(DataError::Capacity(
                "generic collection item count exceeded".into(),
            ));
        }
        if items.iter().any(|current| current.key == item.key) {
            return Err(DataError::Validation(
                "duplicate generic collection key".into(),
            ));
        }
        changes.push(DataChange::Add {
            key: item.key,
            index,
        });
        items.insert(index, item);
        Ok(())
    }
}

fn find_item<T>(items: &[ObservableItem<T>], key: i64) -> Result<usize, DataError> {
    items
        .iter()
        .position(|item| item.key == key)
        .ok_or(DataError::UnknownRow(key))
}
