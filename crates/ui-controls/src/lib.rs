//! Retained controls, themes, commands, and an adaptive Ribbon.

mod button;
mod commands;
mod controls;
mod docking;
mod editor;
mod primitives;
mod ribbon;
mod theme;

pub use button::*;
pub use commands::*;
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
