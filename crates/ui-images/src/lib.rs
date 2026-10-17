//! Static image decoding into immutable resources for GPU presentation.

mod decode;
mod svg;
pub use decode::*;
mod cache;
pub use cache::*;
#[cfg(test)]
mod svg_tests;
#[cfg(test)]
mod tests;

mod edit;
pub use edit::*;
