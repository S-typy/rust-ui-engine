//! Chart page snapshots share the control painter and immutable source model.
use super::*;
use crate::{ChartModel, Theme};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartPrintOptions {
    pub page_size: Size,
    pub margin: f32,
    pub theme: Theme,
    /// Include the selected observation's marker and description.
    pub include_selection: bool,
}

impl Default for ChartPrintOptions {
    fn default() -> Self {
        Self {
            page_size: Size::new(1024.0, 640.0),
            margin: 24.0,
            theme: Theme::FluentLight,
            include_selection: false,
        }
    }
}

/// Snapshot the current model/viewport as one page without mounting a window.
/// A size that would show the control's compact fallback is rejected.
/// The title is document metadata; axis titles and legend remain in the page.
/// File writing and printer submission remain explicit host operations.
pub fn from_chart(
    model: &ChartModel,
    engine: &mut TextEngine,
    title: &str,
    options: ChartPrintOptions,
    mut cancelled: impl FnMut() -> bool,
) -> Result<PrintDocument, PdfError> {
    if cancelled() {
        return Err(PdfError::Cancelled);
    }
    if title.len() > 1024
        || title.contains('\0')
        || !options.page_size.width.is_finite()
        || !options.page_size.height.is_finite()
        || !(1.0..=19200.0).contains(&options.page_size.width)
        || !(1.0..=19200.0).contains(&options.page_size.height)
        || !options.margin.is_finite()
        || options.margin < 0.0
    {
        return Err(invalid("chart print page options"));
    }
    let body = Rect::new(
        options.margin,
        options.margin,
        options.page_size.width - 2.0 * options.margin,
        options.page_size.height - 2.0 * options.margin,
    );
    if !body.is_valid() || crate::controls::chart_paint::plot(model, body).is_none() {
        return Err(invalid(
            "chart print area is too small for full presentation",
        ));
    }
    let mut snapshot = model.clone();
    if !options.include_selection {
        snapshot.select(None).map_err(invalid)?;
    }
    let summary = snapshot
        .selected()
        .and_then(|key| snapshot.describe(key))
        .unwrap_or_default();
    let mut scene = Scene::default();
    scene.fill(
        Rect::new(0.0, 0.0, options.page_size.width, options.page_size.height),
        options.theme.tokens().surface,
    );
    crate::controls::chart_paint::paint_snapshot(
        &mut scene,
        engine,
        options.theme,
        &snapshot,
        body,
        &summary,
    )
    .map_err(invalid)?;
    if cancelled() {
        return Err(PdfError::Cancelled);
    }
    PrintDocument::new(
        title,
        vec![PrintPage {
            size: options.page_size,
            scene,
        }],
    )
    .map_err(invalid)
}
