//! Retained controls, themes, commands, and an adaptive Ribbon.

pub mod barcode;
mod composition_ui;
pub mod dashboard;
mod definition_codec;
pub mod media;
pub mod page_viewer;
pub mod pdf;
pub mod planning;
pub mod printing;
pub mod report_designer;
pub mod reports;
pub mod rich_document;
pub mod rich_editor;
pub mod spreadsheet;
pub use planning::*;
mod binding;
mod button;
mod commands;
mod control_bindings;
mod controls;
mod docking;
pub mod document_workspace;
mod editor;
mod localization;
mod primitives;
mod ribbon;
pub mod spatial;
mod theme;
pub use localization::*;
mod editor_async;
pub use editor_async::*;
mod binding_expression;
pub use binding_expression::*;

pub use binding::*;
pub use button::*;
pub use commands::*;
pub use control_bindings::*;
pub use controls::*;
pub use docking::*;
pub use editor::*;
pub use primitives::*;
pub use ribbon::*;
pub use theme::*;

#[cfg(test)]
mod button_tests;
#[cfg(test)]
mod command_activation_tests;
#[cfg(test)]
mod edit_session_tests;
#[cfg(test)]
mod popup_tests;
#[cfg(test)]
mod primitive_tests;
#[cfg(test)]
mod radio_tests;
#[cfg(test)]
mod tests;

pub mod workspace;
