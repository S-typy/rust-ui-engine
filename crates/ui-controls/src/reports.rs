//! Deterministic, bounded band reports over an immutable typed table.
//! QueryEngine owns filtering, expression types, grouping and aggregate semantics.
use rust_desktop_ui_core::{
    pages::{PrintDocument, PrintPage},
    *,
};
use rust_desktop_ui_data::*;
use rust_desktop_ui_text::TextEngine;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportError {
    pub band: String,
    pub item: Option<usize>,
    pub message: String,
}
impl std::fmt::Display for ReportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Report {}{}: {}",
            self.band,
            self.item.map_or(String::new(), |i| format!("[{i}]")),
            self.message
        )
    }
}
impl std::error::Error for ReportError {}
fn error(band: &str, message: impl ToString) -> ReportError {
    ReportError {
        band: band.into(),
        item: None,
        message: message.to_string(),
    }
}
#[derive(Clone, Debug)]
pub enum ReportValue {
    Literal(String),
    Expression(QueryExpression),
    Parameter(String),
    GroupKey(usize),
    GroupTotal(usize),
    Total(usize),
    PageNumber,
    PageCount,
}
#[derive(Clone, Debug)]
pub enum ReportItem {
    Text {
        x: f32,
        y: f32,
        width: f32,
        font_size: f32,
        bold: bool,
        color: Color,
        value: ReportValue,
    },
    Rectangle {
        bounds: Rect,
        color: Color,
    },
    Image {
        bounds: Rect,
        bitmap: Bitmap,
    },
}
#[derive(Clone, Debug, Default)]
pub struct ReportBand {
    pub height: f32,
    pub items: Vec<ReportItem>,
}
#[derive(Clone, Debug)]
pub struct ReportParameterFilter {
    pub parameter: String,
    pub field: String,
    pub operator: ComparisonOperator,
}
#[derive(Clone, Debug)]
pub struct ReportDefinition {
    pub title: String,
    pub page_size: Size,
    /// Top, right, bottom, left in logical pixels.
    pub margins: [f32; 4],
    pub parameters: Vec<FieldSchema>,
    pub parameter_filters: Vec<ReportParameterFilter>,
    pub query: QueryRequest,
    pub report_header: ReportBand,
    pub page_header: ReportBand,
    pub group_header: ReportBand,
    pub detail: ReportBand,
    pub group_footer: ReportBand,
    pub report_footer: ReportBand,
    pub page_footer: ReportBand,
}
impl Default for ReportDefinition {
    fn default() -> Self {
        Self {
            title: "Report".into(),
            page_size: Size::new(794.0, 1123.0),
            margins: [48.0; 4],
            parameters: Vec::new(),
            parameter_filters: Vec::new(),
            query: QueryRequest::default(),
            report_header: ReportBand::default(),
            page_header: ReportBand::default(),
            group_header: ReportBand::default(),
            detail: ReportBand {
                height: 24.0,
                items: Vec::new(),
            },
            group_footer: ReportBand::default(),
            report_footer: ReportBand::default(),
            page_footer: ReportBand::default(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct ReportResult {
    pub document: PrintDocument,
    pub row_keys: Vec<i64>,
    pub totals: Vec<CellValue>,
    pub groups: Vec<QueryGroup>,
}
pub struct ReportEngine;
struct Context<'a> {
    definition: &'a ReportDefinition,
    table: &'a DataTable,
    parameters: &'a BTreeMap<String, CellValue>,
    result: &'a QueryResult,
    group: Option<&'a QueryGroup>,
    row: Option<i64>,
}
struct BandOutput {
    height: f32,
    scene: Scene,
    page_fields: Vec<(usize, bool)>,
}
struct Builder<'a> {
    context: Context<'a>,
    engine: &'a mut TextEngine,
    pages: Vec<PrintPage>,
    fields: Vec<(usize, usize, bool)>,
    y: f32,
    bottom: f32,
    commands: usize,
    text_bytes: usize,
}
impl ReportEngine {
    /// Failure or cancellation returns no partially built document.
    pub fn build(
        definition: &ReportDefinition,
        table: &DataTable,
        parameters: &BTreeMap<String, CellValue>,
        engine: &mut TextEngine,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<ReportResult, ReportError> {
        if cancelled() {
            return Err(error("build", "cancelled"));
        }
        validate(definition, table, parameters)?;
        let mut request = definition.query.clone();
        for filter in &definition.parameter_filters {
            let value = parameters
                .get(&filter.parameter)
                .ok_or_else(|| error("parameters", "filter parameter missing"))?;
            let c = Criteria::Compare {
                left: QueryExpression::field(&filter.field),
                operator: filter.operator,
                right: QueryExpression::literal(value.clone()),
            };
            request.filter = Some(match request.filter.take() {
                None => c,
                Some(old) => Criteria::All(vec![old, c]),
            });
        }
        let result = QueryEngine::execute(table, &request).map_err(|e| error("query", e))?;
        if result.row_keys.len() > 20_000 {
            return Err(error("query", "20000 report row limit exceeded"));
        }
        let context = Context {
            definition,
            table,
            parameters,
            result: &result,
            group: None,
            row: None,
        };
        let mut b = Builder {
            context,
            engine,
            pages: Vec::new(),
            fields: Vec::new(),
            y: 0.0,
            bottom: 0.0,
            commands: 0,
            text_bytes: 0,
        };
        b.new_page()?;
        b.place("report-header", &definition.report_header)?;
        if result.groups.is_empty() {
            for key in &result.row_keys {
                if cancelled() {
                    return Err(error("detail", "cancelled"));
                }
                b.context.row = Some(*key);
                b.place("detail", &definition.detail)?;
            }
        } else {
            for group in &result.groups {
                if cancelled() {
                    return Err(error("group", "cancelled"));
                }
                b.context.group = Some(group);
                b.context.row = None;
                b.place("group-header", &definition.group_header)?;
                for key in &group.row_keys {
                    if cancelled() {
                        return Err(error("detail", "cancelled"));
                    }
                    b.context.row = Some(*key);
                    b.place("detail", &definition.detail)?;
                }
                b.context.row = None;
                b.place("group-footer", &definition.group_footer)?;
            }
        }
        b.context.group = None;
        b.context.row = None;
        b.place("report-footer", &definition.report_footer)?;
        let count = b.pages.len();
        for (page, text, total) in b.fields {
            b.pages[page].scene.texts[text].text = if total { count } else { page + 1 }.to_string();
        }
        if cancelled() {
            return Err(error("build", "cancelled"));
        }
        let document =
            PrintDocument::new(&definition.title, b.pages).map_err(|e| error("pages", e))?;
        Ok(ReportResult {
            document,
            row_keys: result.row_keys,
            totals: result.aggregates,
            groups: result.groups,
        })
    }
}
fn validate(
    d: &ReportDefinition,
    t: &DataTable,
    p: &BTreeMap<String, CellValue>,
) -> Result<(), ReportError> {
    if !d.page_size.width.is_finite()
        || !d.page_size.height.is_finite()
        || !(100.0..=4000.0).contains(&d.page_size.width)
        || !(100.0..=6000.0).contains(&d.page_size.height)
        || d.margins.iter().any(|v| !v.is_finite() || *v < 0.0)
        || d.page_size.width - d.margins[1] - d.margins[3] < 20.0
        || d.page_size.height - d.margins[0] - d.margins[2] < 20.0
    {
        return Err(error("page", "invalid size or margins"));
    }
    if d.parameters.len() > 64 || d.parameter_filters.len() > 64 || p.len() != d.parameters.len() {
        return Err(error(
            "parameters",
            "unknown, missing or too many parameters",
        ));
    }
    let mut names = std::collections::BTreeSet::new();
    for field in &d.parameters {
        if !names.insert(&field.name) {
            return Err(error("parameters", "duplicate parameter"));
        }
        let value = p
            .get(&field.name)
            .ok_or_else(|| error("parameters", format!("missing {}", field.name)))?;
        field.validate(value).map_err(|e| error("parameters", e))?;
    }
    let mut item_count = 0;
    for (name, band) in [
        ("report-header", &d.report_header),
        ("page-header", &d.page_header),
        ("group-header", &d.group_header),
        ("detail", &d.detail),
        ("group-footer", &d.group_footer),
        ("report-footer", &d.report_footer),
        ("page-footer", &d.page_footer),
    ] {
        item_count += band.items.len();
        if !band.height.is_finite()
            || band.height < 0.0
            || band.height > d.page_size.height
            || item_count > 256
        {
            return Err(error(name, "band height or item budget"));
        }
        for (index, item) in band.items.iter().enumerate() {
            let check = (|| -> Result<(), String> {
                let width = d.page_size.width - d.margins[1] - d.margins[3];
                match item {
                    ReportItem::Text {
                        x,
                        y,
                        width: w,
                        font_size,
                        color,
                        value,
                        ..
                    } => {
                        if !x.is_finite()
                            || !y.is_finite()
                            || !w.is_finite()
                            || *x < 0.0
                            || *y < 0.0
                            || *w <= 0.0
                            || *x + *w > width
                            || !font_size.is_finite()
                            || !(4.0..=128.0).contains(font_size)
                            || [color.r, color.g, color.b, color.a]
                                .iter()
                                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                        {
                            return Err("invalid text geometry/color".into());
                        }
                        match value {
                            ReportValue::Expression(e) => {
                                if name != "detail" {
                                    return Err("row expression outside detail band".into());
                                }
                                QueryEngine::expression_type(t.schema(), e)
                                    .map_err(|e| e.to_string())?;
                            }
                            ReportValue::Parameter(p) if !names.iter().any(|n| *n == p) => {
                                return Err(format!("unknown parameter {p}"));
                            }
                            ReportValue::Total(i) | ReportValue::GroupTotal(i)
                                if *i >= d.query.aggregates.len() =>
                            {
                                return Err("aggregate index".into());
                            }
                            ReportValue::GroupKey(i) if *i >= d.query.group_by.len() => {
                                return Err("group key index".into());
                            }
                            ReportValue::Literal(s) if s.len() > 32_768 => {
                                return Err("literal text limit".into());
                            }
                            _ => {}
                        }
                        if matches!(value, ReportValue::GroupKey(_) | ReportValue::GroupTotal(_))
                            && !matches!(name, "group-header" | "group-footer" | "detail")
                        {
                            return Err("group expression outside grouped bands".into());
                        }
                    }
                    ReportItem::Rectangle { bounds, .. } | ReportItem::Image { bounds, .. } => {
                        if !bounds.is_valid()
                            || bounds.x < 0.0
                            || bounds.y < 0.0
                            || bounds.x + bounds.width > width
                            || bounds.y + bounds.height > band.height
                        {
                            return Err("item outside band".into());
                        }
                    }
                }
                Ok(())
            })();
            if let Err(message) = check {
                return Err(ReportError {
                    band: name.into(),
                    item: Some(index),
                    message,
                });
            }
        }
    }
    Ok(())
}
impl Builder<'_> {
    fn output(&mut self, name: &str, band: &ReportBand) -> Result<BandOutput, ReportError> {
        let mut scene = Scene::default();
        let mut height = band.height;
        let mut fields = Vec::new();
        for (index, item) in band.items.iter().enumerate() {
            let make_error = |e| ReportError {
                band: name.into(),
                item: Some(index),
                message: e,
            };
            match item {
                ReportItem::Rectangle { bounds, color } => scene.fill(*bounds, *color),
                ReportItem::Image { bounds, bitmap } => {
                    scene.image(ImagePrimitive::new(bitmap.clone(), *bounds))
                }
                ReportItem::Text {
                    x,
                    y,
                    width,
                    font_size,
                    bold,
                    color,
                    value,
                } => {
                    let c = &self.context;
                    let text = match value {
                        ReportValue::Literal(s) => s.clone(),
                        ReportValue::Parameter(p) => c
                            .parameters
                            .get(p)
                            .ok_or_else(|| make_error("unknown parameter".into()))?
                            .display(),
                        ReportValue::Expression(e) => QueryEngine::evaluate_expression(
                            c.table,
                            c.row
                                .ok_or_else(|| make_error("row context missing".into()))?,
                            e,
                        )
                        .map_err(|e| make_error(e.to_string()))?
                        .display(),
                        ReportValue::Total(i) => c
                            .result
                            .aggregates
                            .get(*i)
                            .ok_or_else(|| make_error("total missing".into()))?
                            .display(),
                        ReportValue::GroupTotal(i) => c
                            .group
                            .and_then(|g| g.aggregates.get(*i))
                            .ok_or_else(|| make_error("group total missing".into()))?
                            .display(),
                        ReportValue::GroupKey(i) => c
                            .group
                            .and_then(|g| g.key.get(*i))
                            .ok_or_else(|| make_error("group key missing".into()))?
                            .display(),
                        ReportValue::PageNumber | ReportValue::PageCount => "888".into(),
                    };
                    if text.len() > 32_768 {
                        return Err(make_error("text item limit".into()));
                    }
                    let mut run =
                        TextRun::new(text, Rect::new(*x, *y, *width, 6000.0), *color, *font_size);
                    run.wrap = true;
                    run.weight = if *bold { 700 } else { 400 };
                    let prepared = self
                        .engine
                        .prepare(&run, 1.0)
                        .map_err(|e| make_error(e.to_string()))?;
                    if prepared.width > *width + 0.5 {
                        return Err(make_error("unbreakable text exceeds item width".into()));
                    }
                    run.bounds.height = prepared.height.max(*font_size * 1.2);
                    run.clip = run.bounds;
                    height = height.max(*y + run.bounds.height + 2.0);
                    if matches!(value, ReportValue::PageNumber | ReportValue::PageCount) {
                        fields.push((scene.texts.len(), matches!(value, ReportValue::PageCount)));
                    }
                    scene.text(run);
                }
            }
        }
        Ok(BandOutput {
            height,
            scene,
            page_fields: fields,
        })
    }
    fn append(&mut self, output: BandOutput, y: f32) -> Result<(), ReportError> {
        self.commands += output.scene.commands().len();
        self.text_bytes += output
            .scene
            .texts
            .iter()
            .map(|t| t.text.len())
            .sum::<usize>();
        if self.commands > 99_000 || self.text_bytes > 16 * 1024 * 1024 {
            return Err(error("pages", "report command/text budget exceeded"));
        }
        let d = self.context.definition;
        let page = self.pages.len() - 1;
        let first = self.pages[page].scene.texts.len();
        self.pages[page]
            .scene
            .append_view(
                &output.scene,
                Point::new(d.margins[3], y),
                Rect::new(0.0, 0.0, d.page_size.width, d.page_size.height),
            )
            .map_err(|e| error("layout", e))?;
        for (text, total) in output.page_fields {
            self.fields.push((page, first + text, total));
        }
        Ok(())
    }
    fn new_page(&mut self) -> Result<(), ReportError> {
        let d = self.context.definition;
        if self.pages.len() >= 512 {
            return Err(error("pages", "512 page limit exceeded"));
        }
        let saved_row = self.context.row.take();
        let saved_group = self.context.group.take();
        let header = self.output("page-header", &d.page_header)?;
        let footer = self.output("page-footer", &d.page_footer)?;
        self.context.row = saved_row;
        self.context.group = saved_group;
        let bottom = d.page_size.height - d.margins[2] - footer.height;
        self.y = d.margins[0] + header.height;
        self.bottom = bottom;
        if self.y + 1.0 >= bottom {
            return Err(error("page", "headers and margins leave no body"));
        }
        let mut scene = Scene::default();
        scene.fill(
            Rect::new(0.0, 0.0, d.page_size.width, d.page_size.height),
            Color::rgb(255, 255, 255),
        );
        self.pages.push(PrintPage {
            size: d.page_size,
            scene,
        });
        self.append(header, d.margins[0])?;
        self.append(footer, bottom)?;
        Ok(())
    }
    fn place(&mut self, name: &str, band: &ReportBand) -> Result<(), ReportError> {
        if band.items.is_empty() && band.height == 0.0 {
            return Ok(());
        }
        let output = self.output(name, band)?;
        if self.y + output.height > self.bottom {
            self.new_page()?;
            if self.y + output.height > self.bottom {
                return Err(error(
                    name,
                    "band is taller than printable body; no partial page is returned",
                ));
            }
        }
        let height = output.height;
        self.append(output, self.y)?;
        self.y += height;
        Ok(())
    }
}
