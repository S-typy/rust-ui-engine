use super::*;
use rust_desktop_ui_core::{Color, Point, Rect, Scene};
use std::io::{self, Write};

pub const MAX_BARCODE_OUTPUT_SIDE: u32 = 32_768;
pub const MAX_BARCODE_RASTER_PIXELS: u64 = 16_777_216;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarcodeGeometryOptions {
    /// Integral output units per narrow module; a raster/print unit is one dot.
    pub module_size: u16,
    pub bar_height: u32,
    pub quiet_left: Option<u16>,
    pub quiet_right: Option<u16>,
}
impl Default for BarcodeGeometryOptions {
    fn default() -> Self {
        Self {
            module_size: 2,
            bar_height: 96,
            quiet_left: None,
            quiet_right: None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarcodeRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BarcodeGeometry {
    width: u32,
    height: u32,
    module_size: u16,
    bars: Vec<BarcodeRect>,
}
impl BarcodeGeometry {
    pub fn new(
        symbol: &BarcodeSymbol,
        options: BarcodeGeometryOptions,
    ) -> Result<Self, BarcodeError> {
        let left = options.quiet_left.unwrap_or(symbol.quiet_left);
        let right = options.quiet_right.unwrap_or(symbol.quiet_right);
        if options.module_size == 0
            || options.bar_height < 8
            || left < symbol.quiet_left
            || right < symbol.quiet_right
        {
            return Err(BarcodeError(
                "Barcode requires a nonzero module, at least eight height units, and complete quiet zones",
            ));
        }
        let m = u32::from(options.module_size);
        let width = (u32::from(left) + u32::from(symbol.modules) + u32::from(right))
            .checked_mul(m)
            .ok_or(BarcodeError("Barcode output size overflow"))?;
        let guard = if symbol.symbology == BarcodeSymbology::Code128B {
            0
        } else {
            5 * m
        };
        let height = options
            .bar_height
            .checked_add(guard)
            .ok_or(BarcodeError("Barcode output size overflow"))?;
        if width > MAX_BARCODE_OUTPUT_SIDE || height > MAX_BARCODE_OUTPUT_SIDE {
            return Err(BarcodeError("Barcode output exceeds 32768 units per side"));
        }
        let bars = symbol
            .bars
            .iter()
            .map(|bar| BarcodeRect {
                x: (u32::from(left) + u32::from(bar.start)) * m,
                y: 0,
                width: u32::from(bar.width) * m,
                height: options.bar_height + if bar.guard { guard } else { 0 },
            })
            .collect();
        Ok(Self {
            width,
            height,
            module_size: options.module_size,
            bars,
        })
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn module_size(&self) -> u16 {
        self.module_size
    }
    pub fn bars(&self) -> &[BarcodeRect] {
        &self.bars
    }
    /// Reusable vector/print visitor. The opaque white background is emitted
    /// first, followed by black rectangles; callers retain the complete bounds.
    pub fn visit_rectangles(&self, mut visit: impl FnMut(BarcodeRect, bool)) {
        visit(
            BarcodeRect {
                x: 0,
                y: 0,
                width: self.width,
                height: self.height,
            },
            false,
        );
        for &bar in &self.bars {
            visit(bar, true);
        }
    }
    /// Append vector geometry in logical Scene units. This preserves ratios;
    /// it does not promise device-pixel snapping after an external DPI transform.
    /// Partial clipping is rejected before any scene command is appended.
    pub fn append_scene(
        &self,
        scene: &mut Scene,
        origin: Point,
        unit_scale: f32,
        clip: Rect,
    ) -> Result<(), BarcodeError> {
        let bounds = Rect::new(
            origin.x,
            origin.y,
            self.width as f32 * unit_scale,
            self.height as f32 * unit_scale,
        );
        if !unit_scale.is_finite()
            || !(0.001..=1024.0).contains(&unit_scale)
            || !bounds.is_valid()
            || !clip.is_valid()
            || bounds.x < clip.x
            || bounds.y < clip.y
            || bounds.x + bounds.width > clip.x + clip.width
            || bounds.y + bounds.height > clip.y + clip.height
        {
            return Err(BarcodeError(
                "Barcode scene placement must be finite and wholly inside its clip",
            ));
        }
        let module = f32::from(self.module_size) * unit_scale;
        if [
            bounds.x,
            bounds.x + bounds.width,
            bounds.y,
            bounds.y + bounds.height,
        ]
        .into_iter()
        .any(|x| ((x + module) - x - module).abs() > module * 0.01)
        {
            return Err(BarcodeError(
                "Scene coordinates cannot preserve the module precision on both axes",
            ));
        }
        self.visit_rectangles(|r, dark| {
            scene.fill(
                Rect::new(
                    origin.x + r.x as f32 * unit_scale,
                    origin.y + r.y as f32 * unit_scale,
                    r.width as f32 * unit_scale,
                    r.height as f32 * unit_scale,
                ),
                if dark {
                    Color::rgb(0, 0, 0)
                } else {
                    Color::rgb(255, 255, 255)
                },
            )
        });
        Ok(())
    }
    /// Standalone vector output, with no scripts, images, external fonts or I/O.
    pub fn to_svg(&self) -> String {
        use std::fmt::Write as _;
        let mut s = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" shape-rendering=\"crispEdges\">",
            self.width, self.height, self.width, self.height
        );
        self.visit_rectangles(|r, dark| {
            write!(
                s,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>",
                r.x,
                r.y,
                r.width,
                r.height,
                if dark { "#000" } else { "#fff" }
            )
            .unwrap();
        });
        s.push_str("</svg>\n");
        s
    }
    /// A bounded grayscale image suitable for independent decoder validation.
    /// Geometry is validated before writing; writer errors can leave partial I/O.
    pub fn write_pgm(&self, writer: &mut impl Write) -> io::Result<()> {
        if u64::from(self.width) * u64::from(self.height) > MAX_BARCODE_RASTER_PIXELS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Barcode raster exceeds 16 million pixels",
            ));
        }
        write!(writer, "P5\n{} {}\n255\n", self.width, self.height)?;
        let mut row = vec![255u8; self.width as usize];
        for y in 0..self.height {
            row.fill(255);
            for bar in &self.bars {
                if y < bar.height {
                    row[bar.x as usize..(bar.x + bar.width) as usize].fill(0);
                }
            }
            writer.write_all(&row)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarcodePrintOptions {
    pub dpi: u16,
    pub module_mm: f64,
    pub bar_height_mm: f64,
}
impl Default for BarcodePrintOptions {
    fn default() -> Self {
        Self {
            dpi: 300,
            module_mm: 0.33,
            bar_height_mm: 24.0,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct BarcodePrintLayout {
    pub geometry: BarcodeGeometry,
    pub dpi: u16,
    pub actual_module_mm: f64,
}
impl BarcodePrintLayout {
    /// Round module pitch to the nearest complete printer dot (half up), then
    /// derive every bar/space/quiet-zone edge from that one pitch. Height rounds
    /// upward. The realized physical pitch is returned for application checks.
    pub fn new(symbol: &BarcodeSymbol, options: BarcodePrintOptions) -> Result<Self, BarcodeError> {
        if !(72..=2400).contains(&options.dpi)
            || !options.module_mm.is_finite()
            || !(0.05..=10.0).contains(&options.module_mm)
            || !options.bar_height_mm.is_finite()
            || !(1.0..=1000.0).contains(&options.bar_height_mm)
        {
            return Err(BarcodeError(
                "Invalid barcode printer DPI or physical dimensions",
            ));
        }
        let pitch = (options.module_mm * f64::from(options.dpi) / 25.4).round();
        if !(1.0..=f64::from(u16::MAX)).contains(&pitch) {
            return Err(BarcodeError(
                "Requested module is smaller than one printer dot",
            ));
        }
        let height = (options.bar_height_mm * f64::from(options.dpi) / 25.4).ceil();
        if height > f64::from(MAX_BARCODE_OUTPUT_SIDE) {
            return Err(BarcodeError(
                "Printed barcode height exceeds the geometry limit",
            ));
        }
        let geometry = BarcodeGeometry::new(
            symbol,
            BarcodeGeometryOptions {
                module_size: pitch as u16,
                bar_height: height as u32,
                ..Default::default()
            },
        )?;
        Ok(Self {
            geometry,
            dpi: options.dpi,
            actual_module_mm: pitch * 25.4 / f64::from(options.dpi),
        })
    }
}
