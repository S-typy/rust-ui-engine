//! Retained calendar and task-planning compositions over shared data models.
//!
//! A host routes ordinary control activation through `process`, calls `refresh`
//! after layout, then updates Controls once more when geometry changed. These
//! owners never replace the application's Controls or create native windows.
mod common;
mod gantt;
mod scheduler;
mod scheduler_geometry;
mod scheduler_model;
pub use gantt::*;
pub use scheduler::*;
pub use scheduler_geometry::*;
pub use scheduler_model::*;
#[cfg(test)]
mod tests;
