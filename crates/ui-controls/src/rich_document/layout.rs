use super::*;
use rust_desktop_ui_core::{
    Color, FontDescriptor, FontSlant, ImagePrimitive, Rect, Scene, TextAlign, TextRun, TextSpan,
};
use rust_desktop_ui_images::{DecodeOptions, ImageFormat, ImageSource};
use rust_desktop_ui_text::{PreparedText, TextEngine};

#[derive(Clone, Debug)]
pub struct DocumentPlacement {
    pub block: usize,
    pub bounds: Rect,
    /// Byte range in the original paragraph; None for an image or table row.
    pub text_range: Option<Range<usize>>,
}
#[derive(Clone, Debug)]
pub struct DocumentPage {
    pub scene: Scene,
    pub placements: Vec<DocumentPlacement>,
}
#[derive(Clone, Debug)]
pub struct DocumentLayout {
    pub pages: Vec<DocumentPage>,
    pub missing_glyphs: usize,
    pub actual_fonts: Vec<String>,
}
struct LayoutBuilder<'a> {
    doc: &'a RichDocument,
    engine: &'a mut TextEngine,
    result: DocumentLayout,
    y: f32,
}

impl RichDocument {
    /// Shapes paragraphs with the shared system-font engine and paginates at
    /// measured line boundaries. Table rows and images are indivisible.
    /// Oversized objects are diagnosed; content is never silently clipped away.
    pub fn paginate(&self, engine: &mut TextEngine) -> Result<DocumentLayout, DocumentError> {
        self.validate()?;
        let mut builder = LayoutBuilder {
            doc: self,
            engine,
            result: DocumentLayout {
                pages: Vec::new(),
                missing_glyphs: 0,
                actual_fonts: Vec::new(),
            },
            y: self.page.margins()[0],
        };
        builder.new_page()?;
        for (index, block) in self.blocks.iter().enumerate() {
            match block {
                RichBlock::Paragraph(p) => builder.paragraph(index, p)?,
                RichBlock::Table(t) => builder.table(index, t)?,
                RichBlock::Image(i) => builder.image(index, i)?,
            }
        }
        Ok(builder.result)
    }
}
impl LayoutBuilder<'_> {
    fn bottom(&self) -> f32 {
        self.doc.page.height - self.doc.page.margins()[2]
    }
    fn width(&self) -> f32 {
        self.doc.page.content_width()
    }
    fn page(&mut self) -> &mut DocumentPage {
        self.result.pages.last_mut().expect("page created")
    }
    fn new_page(&mut self) -> Result<(), DocumentError> {
        if self.result.pages.len() >= 512 {
            return Err(DocumentError::Limit("512 pages"));
        }
        let mut scene = Scene::default();
        scene.fill(
            Rect::new(0.0, 0.0, self.doc.page.width, self.doc.page.height),
            Color::rgb(255, 255, 255),
        );
        self.result.pages.push(DocumentPage {
            scene,
            placements: Vec::new(),
        });
        self.y = self.doc.page.margins()[0];
        Ok(())
    }
    fn prepare(
        &mut self,
        p: &Paragraph,
        width: f32,
    ) -> Result<(TextRun, Vec<TextSpan>, PreparedText), DocumentError> {
        let (run, spans) = paragraph_run(p, width);
        let prepared = self
            .engine
            .prepare_styled(&run, &spans, 1.0)
            .map_err(|e| DocumentError::Invalid(e.to_string()))?;
        if prepared
            .line_metrics()
            .iter()
            .any(|line| line.bounds.width > width + 0.5)
        {
            return Err(DocumentError::Unsupported(
                "unbreakable text exceeds paragraph or cell width".into(),
            ));
        }
        self.result.missing_glyphs += prepared.missing_glyphs;
        for font in prepared.font_runs() {
            if !self.result.actual_fonts.contains(&font.font_name) {
                self.result.actual_fonts.push(font.font_name);
            }
        }
        Ok((run, spans, prepared))
    }
    fn paragraph(&mut self, index: usize, p: &Paragraph) -> Result<(), DocumentError> {
        if p.page_break_before && self.y > self.doc.page.margins()[0] {
            self.new_page()?;
        }
        let (run, spans, prepared) = self.prepare(p, self.width())?;
        let lines = prepared.line_metrics();
        if lines.is_empty() {
            self.ensure_space(20.0)?;
            self.y += 20.0 + p.space_after;
            return Ok(());
        }
        let mut start = 0;
        while start < lines.len() {
            let top = lines[start].bounds.y;
            let first_height = lines[start].bounds.height;
            self.ensure_space(first_height)?;
            let mut end = start + 1;
            while end < lines.len()
                && lines[end].bounds.y + lines[end].bounds.height - top <= self.bottom() - self.y
            {
                end += 1;
            }
            let height = lines[end - 1].bounds.y + lines[end - 1].bounds.height - top;
            let clip = Rect::new(self.doc.page.margins()[3], self.y, self.width(), height);
            self.paint_paragraph(
                &run,
                &spans,
                &prepared,
                self.doc.page.margins()[3],
                self.y - top,
                clip,
            )?;
            self.underlines(p, &prepared, self.doc.page.margins()[3], self.y - top, clip);
            self.page().placements.push(DocumentPlacement {
                block: index,
                bounds: clip,
                text_range: Some(lines[start].text_range.start..lines[end - 1].text_range.end),
            });
            self.y += height;
            start = end;
            if start < lines.len() {
                self.new_page()?;
            }
        }
        self.y += p.space_after;
        Ok(())
    }
    fn ensure_space(&mut self, height: f32) -> Result<(), DocumentError> {
        if height > self.doc.page.content_height() {
            return Err(DocumentError::Unsupported(
                "object or line exceeds page height".into(),
            ));
        }
        if self.y + height > self.bottom() {
            self.new_page()?;
        }
        Ok(())
    }
    fn paint_paragraph(
        &mut self,
        original: &TextRun,
        spans: &[TextSpan],
        prepared: &PreparedText,
        x: f32,
        y: f32,
        clip: Rect,
    ) -> Result<(), DocumentError> {
        let mut run = original.clone();
        run.bounds.x = x;
        run.bounds.y = y;
        run.clip = clip;
        self.page()
            .scene
            .styled_text(run, spans.to_vec())
            .map_err(|e| DocumentError::Invalid(e.into()))?;
        let _ = prepared;
        Ok(())
    }
    fn underlines(&mut self, p: &Paragraph, prepared: &PreparedText, x: f32, y: f32, clip: Rect) {
        let mut offset = 0;
        for run in &p.runs {
            let end = offset + run.text.len();
            if run.style.underline {
                for bounds in prepared.selection_rects(offset..end) {
                    let line = Rect::new(
                        x + bounds.x,
                        y + bounds.y + bounds.height - 1.0,
                        bounds.width,
                        1.0,
                    );
                    if let Some(visible) = line.intersection(clip) {
                        self.page().scene.fill(
                            visible,
                            Color::rgb(run.style.color[0], run.style.color[1], run.style.color[2]),
                        );
                    }
                }
            }
            offset = end;
        }
    }
    fn table(&mut self, index: usize, t: &RichTable) -> Result<(), DocumentError> {
        for row in &t.rows {
            let mut cells = Vec::new();
            let mut height = 24.0f32;
            for (p, width) in row.iter().zip(&t.columns) {
                let prepared = self.prepare(p, *width - 12.0)?;
                height = height.max(prepared.2.height + 12.0);
                cells.push(prepared);
            }
            self.ensure_space(height)?;
            let mut x = self.doc.page.margins()[3];
            for (((run, spans, prepared), width), paragraph) in
                cells.iter().zip(&t.columns).zip(row)
            {
                let bounds = Rect::new(x, self.y, *width, height);
                self.page().scene.rounded_stroke(
                    bounds,
                    bounds,
                    0.0,
                    1.0,
                    Color::rgb(180, 180, 180),
                );
                self.paint_paragraph(
                    run,
                    spans,
                    prepared,
                    x + 6.0,
                    bounds.y + 6.0,
                    Rect::new(x + 6.0, bounds.y + 6.0, *width - 12.0, height - 12.0),
                )?;
                self.underlines(
                    paragraph,
                    prepared,
                    x + 6.0,
                    bounds.y + 6.0,
                    Rect::new(x + 6.0, bounds.y + 6.0, *width - 12.0, height - 12.0),
                );
                x += width;
            }
            let bounds = Rect::new(
                self.doc.page.margins()[3],
                self.y,
                t.columns.iter().sum(),
                height,
            );
            self.page().placements.push(DocumentPlacement {
                block: index,
                bounds,
                text_range: None,
            });
            self.y += height;
        }
        self.y += 8.0;
        Ok(())
    }
    fn image(&mut self, index: usize, i: &RichImage) -> Result<(), DocumentError> {
        self.ensure_space(i.height)?;
        let media = &self.doc.media[&i.media];
        let format = if media.content_type == "image/png" {
            ImageFormat::Png
        } else {
            ImageFormat::Jpeg
        };
        let source = ImageSource::from_bytes(media.bytes.clone(), format)
            .map_err(|e| DocumentError::Invalid(e.to_string()))?;
        let image = source
            .decode(DecodeOptions::default())
            .map_err(|e| DocumentError::Invalid(e.to_string()))?;
        let bounds = Rect::new(self.doc.page.margins()[3], self.y, i.width, i.height);
        self.page()
            .scene
            .image(ImagePrimitive::new(image.bitmap().clone(), bounds));
        self.page().placements.push(DocumentPlacement {
            block: index,
            bounds,
            text_range: None,
        });
        self.y += i.height + 8.0;
        Ok(())
    }
}
fn paragraph_run(p: &Paragraph, width: f32) -> (TextRun, Vec<TextSpan>) {
    let mut run = TextRun::new(
        p.plain_text(),
        Rect::new(0.0, 0.0, width, 1_000_000.0),
        Color::rgb(24, 24, 24),
        16.0,
    );
    run.wrap = true;
    run.align = match p.alignment {
        ParagraphAlignment::Start => TextAlign::Start,
        ParagraphAlignment::Center => TextAlign::Center,
        ParagraphAlignment::End => TextAlign::End,
    };
    let mut offset = 0;
    let mut spans = Vec::new();
    for rich in &p.runs {
        let end = offset + rich.text.len();
        if end > offset {
            spans.push(TextSpan {
                range: offset..end,
                font: Some(FontDescriptor {
                    family: rich.style.family.clone(),
                    size: rich.style.size,
                    weight: if rich.style.bold { 700 } else { 400 },
                    slant: if rich.style.italic {
                        FontSlant::Italic
                    } else {
                        FontSlant::Normal
                    },
                    ..Default::default()
                }),
                font_size: None,
                weight: None,
                color: Some(Color::rgb(
                    rich.style.color[0],
                    rich.style.color[1],
                    rich.style.color[2],
                )),
            });
        }
        offset = end;
    }
    (run, spans)
}
