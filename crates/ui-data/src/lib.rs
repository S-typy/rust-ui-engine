//! Typed, bounded tabular data with stable signed integer keys.
//!
//! Adapters perform explicit synchronous reads and transactional writes. A
//! source keeps the last accepted snapshot, executes local typed queries and
//! never performs I/O while reading a cell. Large or remote data requires an
//! application-owned worker or a separate paged source.

#[cfg(feature = "file-adapters")]
mod files;
mod model;
mod observable;
mod query;
mod scalar;
mod source;
#[cfg(feature = "sqlite")]
mod sqlite;
mod view;
mod write;

#[cfg(feature = "file-adapters")]
pub use files::*;
pub use model::*;
pub use observable::*;
pub use query::*;
pub use scalar::*;
pub use source::*;
#[cfg(feature = "sqlite")]
pub use sqlite::*;
pub use view::*;
pub use write::*;

#[cfg(test)]
mod observable_tests;
#[cfg(test)]
mod query_tests;
#[cfg(all(test, feature = "sqlite"))]
mod sqlite_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod write_tests;
