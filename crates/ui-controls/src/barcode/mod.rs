//! Bounded linear barcode encoding and shared vector/print geometry.
//!
//! Supported profiles are EAN-13, EAN-8 and printable ASCII Code 128 Set B.
//! A symbol is independent of retained controls, display DPI and I/O. Geometry
//! supplies the same opaque bars and quiet zones to Scene, SVG and raster output.

mod encoding;
mod geometry;
pub use geometry::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BarcodeSymbology {
    #[default]
    Ean13,
    Ean8,
    Code128B,
}
impl BarcodeSymbology {
    pub fn name(self) -> &'static str {
        match self {
            Self::Ean13 => "EAN-13",
            Self::Ean8 => "EAN-8",
            Self::Code128B => "Code 128 B",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BarcodeError(pub &'static str);
impl std::fmt::Display for BarcodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for BarcodeError {}

/// One dark run in the standardized module grid, excluding quiet zones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarcodeBar {
    pub start: u16,
    pub width: u8,
    pub guard: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BarcodeSymbol {
    symbology: BarcodeSymbology,
    data: String,
    bars: Vec<BarcodeBar>,
    modules: u16,
    quiet_left: u16,
    quiet_right: u16,
}
impl BarcodeSymbol {
    /// EAN accepts payload-only digits or a verified complete check digit.
    /// Code128B accepts 1..128 printable ASCII bytes (32..126), without FNC.
    pub fn encode(symbology: BarcodeSymbology, data: &str) -> Result<Self, BarcodeError> {
        encoding::encode(symbology, data)
    }
    pub fn symbology(&self) -> BarcodeSymbology {
        self.symbology
    }
    /// Canonical encoded data, including the EAN check digit.
    pub fn data(&self) -> &str {
        &self.data
    }
    pub fn bars(&self) -> &[BarcodeBar] {
        &self.bars
    }
    pub fn modules(&self) -> u16 {
        self.modules
    }
    pub fn quiet_zones(&self) -> (u16, u16) {
        (self.quiet_left, self.quiet_right)
    }
    pub fn total_modules(&self) -> u16 {
        self.modules + self.quiet_left + self.quiet_right
    }
}

#[cfg(test)]
mod tests;
