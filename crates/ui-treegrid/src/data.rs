use std::{error::Error, fmt};

/// Stable application-defined identity. It must survive sort/filter/reload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RowKey(pub u64);

/// Stable column identity independent of the visible column order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColumnId(pub u64);

/// Opaque model-scoped token for one lazy load. Responses from another model,
/// an older query epoch or a cancelled request cannot be installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestId {
    pub(crate) model: u64,
    pub(crate) epoch: u64,
    pub(crate) sequence: u64,
    pub(crate) parent: RowKey,
}

impl RequestId {
    pub fn parent(self) -> RowKey {
        self.parent
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChildrenLoad {
    Ready(Vec<RowKey>),
    /// The host eventually calls `TreeGrid::complete_children` with this token.
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortOrder {
    pub column: ColumnId,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GridQuery {
    pub sort: Vec<SortOrder>,
    pub filter: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GridError {
    InvalidGeometry(&'static str),
    InvalidColumn(&'static str),
    UnknownColumn(ColumnId),
    UnknownRow(RowKey),
    Source(String),
    InvalidHierarchy(&'static str),
    NoEdit,
    ReadOnly,
    Validation(String),
    InvalidPersistence(&'static str),
    Capacity,
}

impl fmt::Display for GridError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidGeometry(x) => write!(f, "invalid grid geometry: {x}"),
            Self::InvalidColumn(x) => write!(f, "invalid column: {x}"),
            Self::UnknownColumn(x) => write!(f, "unknown column: {x:?}"),
            Self::UnknownRow(x) => write!(f, "unknown row: {x:?}"),
            Self::Source(x) => write!(f, "data source: {x}"),
            Self::InvalidHierarchy(x) => write!(f, "invalid hierarchy: {x}"),
            Self::NoEdit => f.write_str("no active cell editor"),
            Self::ReadOnly => f.write_str("cell is read-only"),
            Self::Validation(x) => write!(f, "cell validation: {x}"),
            Self::InvalidPersistence(x) => write!(f, "invalid column layout: {x}"),
            Self::Capacity => f.write_str("grid index or request identity capacity exceeded"),
        }
    }
}
impl Error for GridError {}

/// Indexed access to application-owned data; no database or worker is imposed.
///
/// `child_at`, `position` and `parent` must be consistent and inexpensive: a
/// flat source implements these arithmetically or through its own index. `None`
/// from `child_count(Some(row))` means unloaded children, while `Some(0)` is a
/// leaf. Root count must always be known. Hidden/filter-excluded keys may return
/// `None` from `position`; cell identity must not be derived from visible order.
pub trait TreeDataSource {
    fn child_count(&self, parent: Option<RowKey>) -> Option<usize>;
    fn child_at(&self, parent: Option<RowKey>, index: usize) -> Option<RowKey>;
    fn parent(&self, row: RowKey) -> Option<RowKey>;
    fn position(&self, row: RowKey) -> Option<usize>;
    fn cell(&self, row: RowKey, column: ColumnId) -> String;

    fn editable(&self, _row: RowKey, _column: ColumnId) -> bool {
        false
    }
    fn validate_edit(&self, _row: RowKey, _column: ColumnId, _value: &str) -> Result<(), String> {
        Ok(())
    }
    fn set_cell(&mut self, _row: RowKey, _column: ColumnId, _value: &str) -> Result<(), String> {
        Err("data source does not support editing".into())
    }
    /// The source owns sort/filter execution, including any server/database work.
    /// On error it must leave its visible index unchanged.
    fn apply_query(&mut self, query: &GridQuery) -> Result<(), String> {
        if query == &GridQuery::default() {
            Ok(())
        } else {
            Err("data source does not support this query".into())
        }
    }
    fn request_children(
        &mut self,
        _parent: RowKey,
        _request: RequestId,
    ) -> Result<ChildrenLoad, String> {
        Ok(ChildrenLoad::Pending)
    }
    fn cancel_request(&mut self, _request: RequestId) {}
}
