//! Explicit printer transport for immutable page snapshots. Call from a worker:
//! printer drivers and spooler calls can block. No job is submitted by enumeration.
use rust_desktop_ui_core::pages::PrintDocument;
use std::sync::atomic::AtomicBool;

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
}
impl PrintTicket {
    pub fn new(printer: impl Into<String>) -> Self {
        Self {
            printer: printer.into(),
            pages: Vec::new(),
            copies: 1,
            fit_to_printable_area: true,
            output: None,
        }
    }
    pub fn validate(&self, document: &PrintDocument) -> Result<Vec<usize>, PrintError> {
        if self.printer.is_empty()
            || self.printer.len() > 1024
            || self.printer.contains('\0')
            || !(1..=99).contains(&self.copies)
        {
            return Err(PrintError::Invalid("printer name or copies".into()));
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
        for page in document.pages() {
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
            {
                return Err(PrintError::Unsupported("transparent vector ink".into()));
            }
        }
        Ok(pages)
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
        assert!(ticket.validate(&unprepared).is_err());
    }
}
