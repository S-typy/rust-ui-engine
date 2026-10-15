//! Docking placement, retained tab chrome and independent window geometry.
mod model;
mod persistence;
mod view;
pub use model::*;
pub use persistence::MAX_DOCK_SNAPSHOT_BYTES;
pub use view::*;
