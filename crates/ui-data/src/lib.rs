//! Typed, bounded tabular data with stable signed integer keys.
//!
//! Adapters perform explicit synchronous reads and transactional writes. A
//! source keeps the last accepted snapshot, executes local typed queries and
//! never performs I/O while reading a cell. Large or remote data requires an
//! application-owned worker or a separate paged source.

mod model;
mod observable;
mod scalar;
mod source;
#[cfg(feature = "sqlite")]
mod sqlite;

pub use model::*;
pub use observable::*;
pub use scalar::*;
pub use source::*;
#[cfg(feature = "sqlite")]
pub use sqlite::*;

#[cfg(test)]
mod observable_tests;
#[cfg(test)]
mod tests;
