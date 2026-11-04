//! Explicit printer transport for immutable page snapshots. Call from a worker:
//! printer drivers and spooler calls can block. No job is submitted by enumeration.
use rust_desktop_ui_core::pages::PrintDocument;
use std::sync::atomic::AtomicBool;

/// Rendering policy for printer transport. File export is independent of this policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PrintRenderMode {
    /// Keep opaque rectangle/polygon fills native; composite other pages on GPU.
    #[default]
    Auto,
    /// Require the bounded GDI profile, including prepared text and driver alpha support.
    Native,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrintError {
    Invalid(String),
    Unsupported(String),
    Native(String),
    Cancelled,
}
impl std::fmt::Display for PrintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(s) => write!(f, "Invalid print request: {s}"),
            Self::Unsupported(s) => write!(f, "Unsupported printing: {s}"),
            Self::Native(s) => write!(f, "Printer error: {s}"),
            Self::Cancelled => f.write_str("Print job cancelled"),
        }
    }
}
impl std::error::Error for PrintError {}
#[derive(Clone, Debug)]
pub struct PrintTicket {
    pub printer: String,
    /// Zero-based page indices, in output order. Empty selects all pages.
    pub pages: Vec<usize>,
    pub copies: u16,
    pub fit_to_printable_area: bool,
    /// Explicit file destination for a virtual printer; no default is invented.
    pub output: Option<std::path::PathBuf>,
    pub render_mode: PrintRenderMode,
    /// GPU page resolution, 96..=600 DPI. Default 192; never silently reduced.
    /// Each raster page must fit the renderer's 8192px and 64MiB bitmap limits.
    pub raster_dpi: u16,
}
impl PrintTicket {
    pub fn new(printer: impl Into<String>) -> Self {
        Self {
            printer: printer.into(),
            pages: Vec::new(),
            copies: 1,
            fit_to_printable_area: true,
            output: None,
            render_mode: PrintRenderMode::Auto,
            raster_dpi: 192,
        }
    }
    pub fn validate(&self, document: &PrintDocument) -> Result<Vec<usize>, PrintError> {
        if self.printer.is_empty()
            || self.printer.len() > 1024
            || self.printer.contains('\0')
            || !(1..=99).contains(&self.copies)
            || !(96..=600).contains(&self.raster_dpi)
        {
            return Err(PrintError::Invalid(
                "printer name, copies or raster DPI".into(),
            ));
        }
        let pages = if self.pages.is_empty() {
            (0..document.pages().len()).collect()
        } else {
            self.pages.clone()
        };
        if pages.len() > 512
            || pages.iter().any(|p| *p >= document.pages().len())
            || pages.len() * usize::from(self.copies) > 4096
        {
            return Err(PrintError::Invalid(
                "page selection or output page budget".into(),
            ));
        }
        if let Some(path) = &self.output
            && (!path.is_absolute() || path.as_os_str().to_string_lossy().contains('\0'))
        {
            return Err(PrintError::Invalid(
                "output must be an absolute path without NUL".into(),
            ));
        }
        for index in &pages {
            let page = &document.pages()[*index];
            if self.uses_gpu(&page.scene) {
                self.raster_size(page)?;
                continue;
            }
            if !page.scene.ellipses.is_empty() {
                return Err(PrintError::Unsupported("native ellipse printing".into()));
            }
            if page.scene.images.iter().any(|image| {
                [
                    image.source.x,
                    image.source.y,
                    image.source.width,
                    image.source.height,
                ]
                .iter()
                .any(|value| value.fract() != 0.0)
            }) {
                return Err(PrintError::Unsupported(
                    "fractional source image pixels".into(),
                ));
            }
            if !page.scene.texts.is_empty() {
                return Err(PrintError::Invalid(
                    "prepare text with printing::prepare_for_printer before transport".into(),
                ));
            }
            if page.scene.rectangles.iter().any(|r| r.color.a != 1.0)
                || page
                    .scene
                    .rounded_rectangles
                    .iter()
                    .any(|r| r.color.a != 1.0)
                || page.scene.stroke_paths.iter().any(|r| r.color.a != 1.0)
                || page.scene.fill_paths.iter().any(|r| r.color.a != 1.0)
            {
                return Err(PrintError::Unsupported("transparent vector ink".into()));
            }
        }
        Ok(pages)
    }
    pub(crate) fn uses_gpu(&self, scene: &rust_desktop_ui_core::Scene) -> bool {
        self.render_mode == PrintRenderMode::Auto
            && (!scene.texts.is_empty()
                || !scene.images.is_empty()
                || !scene.ellipses.is_empty()
                || !scene.rounded_rectangles.is_empty()
                || !scene.stroke_paths.is_empty()
                || scene.rectangles.iter().any(|r| r.color.a != 1.0)
                || scene.fill_paths.iter().any(|r| r.color.a != 1.0))
    }
    pub(crate) fn raster_size(
        &self,
        page: &rust_desktop_ui_core::pages::PrintPage,
    ) -> Result<[u32; 2], PrintError> {
        let scale = f64::from(self.raster_dpi) / 96.0;
        let size = [
            (f64::from(page.size.width) * scale).ceil() as u32,
            (f64::from(page.size.height) * scale).ceil() as u32,
        ];
        rust_desktop_ui_render_wgpu::OffscreenRenderer::validate_size(size)
            .map_err(|e| PrintError::Invalid(e.to_string()))?;
        Ok(size)
    }
}
#[derive(Clone, Debug)]
pub struct PrintReceipt {
    pub job_id: i32,
    pub pages_submitted: usize,
}
#[cfg(target_os = "windows")]
mod windows;
pub fn printers() -> Result<Vec<String>, PrintError> {
    #[cfg(target_os = "windows")]
    {
        windows::printers()
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err(PrintError::Unsupported(
            "native printer adapter on this platform".into(),
        ))
    }
}
pub fn print(
    document: &PrintDocument,
    ticket: &PrintTicket,
    cancelled: &AtomicBool,
) -> Result<PrintReceipt, PrintError> {
    let pages = ticket.validate(document)?;
    #[cfg(target_os = "windows")]
    {
        windows::print(document, ticket, &pages, cancelled)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (pages, cancelled);
        Err(PrintError::Unsupported(
            "native printer adapter on this platform".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_core::{Color, Rect, Scene, Size, TextRun, pages::PrintPage};

    #[test]
    fn render_policy_checks_selected_pages_and_bounds_raster_allocations() {
        let mut opaque = Scene::default();
        opaque.fill(Rect::new(0.0, 0.0, 100.0, 100.0), Color::rgb(0, 0, 0));
        let mut transparent = Scene::default();
        transparent.fill(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            Color {
                a: 0.5,
                ..Color::rgb(0, 0, 0)
            },
        );
        let document = PrintDocument::new(
            "selection",
            vec![
                PrintPage {
                    size: Size::new(100.0, 100.0),
                    scene: opaque.clone(),
                },
                PrintPage {
                    size: Size::new(100.0, 100.0),
                    scene: transparent.clone(),
                },
                PrintPage {
                    size: Size::new(19200.0, 19200.0),
                    scene: transparent.clone(),
                },
            ],
        )
        .unwrap();
        let mut ticket = PrintTicket::new("test");
        assert!(!ticket.uses_gpu(&opaque));
        assert!(ticket.uses_gpu(&transparent));
        ticket.pages = vec![1, 0, 1];
        assert_eq!(ticket.validate(&document).unwrap(), vec![1, 0, 1]);
        assert_eq!(
            ticket.raster_size(&document.pages()[1]).unwrap(),
            [200, 200]
        );
        ticket.pages = vec![2];
        assert!(ticket.validate(&document).is_err());
        ticket.pages = vec![0];
        for dpi in [0, 95, 601, u16::MAX] {
            ticket.raster_dpi = dpi;
            assert!(ticket.validate(&document).is_err());
        }
        ticket.raster_dpi = 192;
        ticket.render_mode = PrintRenderMode::Native;
        assert!(ticket.validate(&document).is_ok()); // Unselected alpha page is irrelevant.
        ticket.pages = vec![1];
        assert!(ticket.validate(&document).is_err());
    }

    #[test]
    fn auto_accepts_ellipse_and_fractional_image_crop_native_rejects_before_io() {
        use rust_desktop_ui_core::{Bitmap, ImagePrimitive};
        let mut ellipse = Scene::default();
        let bounds = Rect::new(0.0, 0.0, 20.0, 20.0);
        ellipse.ellipse(bounds, bounds, None, Color::rgb(0, 0, 0));
        let mut image =
            ImagePrimitive::new(Bitmap::new_rgba8(2, 2, vec![255; 16]).unwrap(), bounds);
        image.source = Rect::new(0.25, 0.25, 1.0, 1.0);
        let mut crop = Scene::default();
        crop.image(image);
        for scene in [ellipse, crop] {
            let document = PrintDocument::new(
                "scene",
                vec![PrintPage {
                    size: Size::new(100.0, 100.0),
                    scene,
                }],
            )
            .unwrap();
            let mut ticket = PrintTicket::new("test");
            assert!(ticket.validate(&document).is_ok());
            ticket.render_mode = PrintRenderMode::Native;
            assert!(ticket.validate(&document).is_err());
        }
    }

    #[test]
    fn default_uses_shared_gpu_for_inset_borders_and_stroke_caps() {
        let ticket = PrintTicket::new("test");
        let bounds = Rect::new(0.0, 0.0, 40.0, 40.0);
        let mut scene = Scene::default();
        scene.rounded_stroke(bounds, bounds, 8.0, 4.0, Color::rgb(0, 0, 0));
        assert!(ticket.uses_gpu(&scene));
        scene.clear();
        scene.stroke_path(
            &[rust_desktop_ui_core::StrokeSegment {
                start: rust_desktop_ui_core::Point::new(4.0, 4.0),
                end: rust_desktop_ui_core::Point::new(30.0, 30.0),
            }],
            bounds,
            3.0,
            Color::rgb(0, 0, 0),
        );
        assert!(ticket.uses_gpu(&scene));
    }

    #[test]
    fn ticket_validates_selection_copies_and_prepared_text_without_io() {
        let mut scene = Scene::default();
        scene.fill(Rect::new(0.0, 0.0, 100.0, 100.0), Color::rgb(255, 255, 255));
        let document = PrintDocument::new(
            "Test",
            vec![PrintPage {
                size: Size::new(100.0, 100.0),
                scene: scene.clone(),
            }],
        )
        .unwrap();
        let mut ticket = PrintTicket::new("Not an actual printer");
        assert_eq!(ticket.validate(&document).unwrap(), vec![0]);
        ticket.pages = vec![1];
        assert!(ticket.validate(&document).is_err());
        ticket.pages.clear();
        ticket.copies = 0;
        assert!(ticket.validate(&document).is_err());
        ticket.copies = 1;
        ticket.output = Some("relative.pdf".into());
        assert!(ticket.validate(&document).is_err());
        ticket.output = None;
        scene.text(TextRun::new(
            "Text",
            Rect::new(0.0, 0.0, 100.0, 20.0),
            Color::rgb(0, 0, 0),
            12.0,
        ));
        let unprepared = PrintDocument::new(
            "Text",
            vec![PrintPage {
                size: Size::new(100.0, 100.0),
                scene,
            }],
        )
        .unwrap();
        assert!(ticket.validate(&unprepared).is_ok());
        ticket.render_mode = PrintRenderMode::Native;
        assert!(ticket.validate(&unprepared).is_err());
    }
}
