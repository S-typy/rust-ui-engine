//! Docking placement, retained tab chrome and independent window geometry.
mod mdi;
mod model;
pub use mdi::*;
mod persistence;
mod view;
pub use model::*;
pub use persistence::MAX_DOCK_SNAPSHOT_BYTES;
pub use view::*;
