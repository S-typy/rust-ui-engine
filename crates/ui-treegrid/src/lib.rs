//! Virtualized hierarchical data grid with a backend-independent model and view.
//!
//! Source data, retained state and viewport materialization are separate. Flat
//! sources can expose millions of indexed rows without allocating per-row UI
//! nodes. Sorting/filtering and edits are delegated to the data source.

mod data;
mod index;
mod model;
mod view;

pub use data::*;
pub use model::*;
pub use view::*;
