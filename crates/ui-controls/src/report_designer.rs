//! Transactional report editing. Evaluation and pagination belong to ReportEngine.
use crate::reports::*;
use rust_desktop_ui_core::*;
use rust_desktop_ui_data::*;
use rust_desktop_ui_text::TextEngine;
use std::collections::{BTreeMap, VecDeque};
mod view;
pub use view::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportBandKind {
    ReportHeader,
    PageHeader,
    GroupHeader,
    Detail,
    GroupFooter,
    ReportFooter,
    PageFooter,
}
impl ReportBandKind {
    pub const ALL: [Self; 7] = [
        Self::ReportHeader,
        Self::PageHeader,
        Self::GroupHeader,
        Self::Detail,
        Self::GroupFooter,
        Self::ReportFooter,
        Self::PageFooter,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::ReportHeader => "Report header",
            Self::PageHeader => "Page header",
            Self::GroupHeader => "Group header",
            Self::Detail => "Detail",
            Self::GroupFooter => "Group footer",
            Self::ReportFooter => "Report footer",
            Self::PageFooter => "Page footer",
        }
    }
    pub fn band(self, d: &ReportDefinition) -> &ReportBand {
        match self {
            Self::ReportHeader => &d.report_header,
            Self::PageHeader => &d.page_header,
            Self::GroupHeader => &d.group_header,
            Self::Detail => &d.detail,
            Self::GroupFooter => &d.group_footer,
            Self::ReportFooter => &d.report_footer,
            Self::PageFooter => &d.page_footer,
        }
    }
    fn band_mut(self, d: &mut ReportDefinition) -> &mut ReportBand {
        match self {
            Self::ReportHeader => &mut d.report_header,
            Self::PageHeader => &mut d.page_header,
            Self::GroupHeader => &mut d.group_header,
            Self::Detail => &mut d.detail,
            Self::GroupFooter => &mut d.group_footer,
            Self::ReportFooter => &mut d.report_footer,
            Self::PageFooter => &mut d.page_footer,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReportSelection {
    pub band: ReportBandKind,
    pub item: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReportRevision(u64);
#[derive(Clone)]
struct Snapshot {
    definition: ReportDefinition,
    selection: Option<ReportSelection>,
    bytes: usize,
}

pub struct ReportDesignerSession {
    current: Snapshot,
    table: DataTable,
    parameters: BTreeMap<String, CellValue>,
    undo: VecDeque<Snapshot>,
    redo: VecDeque<Snapshot>,
    revision: u64,
    preview: ReportResult,
}
fn error(e: impl ToString) -> ReportError {
    ReportError {
        band: "designer".into(),
        item: None,
        message: e.to_string(),
    }
}
impl ReportDesignerSession {
    pub fn new(
        definition: ReportDefinition,
        table: DataTable,
        parameters: BTreeMap<String, CellValue>,
    ) -> Result<Self, ReportError> {
        let bytes = definition.to_json()?.len();
        let preview = ReportEngine::build(
            &definition,
            &table,
            &parameters,
            &mut TextEngine::new(),
            || false,
        )?;
        Ok(Self {
            current: Snapshot {
                definition,
                selection: None,
                bytes,
            },
            table,
            parameters,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            revision: 0,
            preview,
        })
    }
    pub fn definition(&self) -> &ReportDefinition {
        &self.current.definition
    }
    pub fn table(&self) -> &DataTable {
        &self.table
    }
    pub fn selection(&self) -> Option<ReportSelection> {
        self.current.selection
    }
    pub fn revision(&self) -> ReportRevision {
        ReportRevision(self.revision)
    }
    /// Always evaluated from the accepted definition and matching data.
    pub fn preview(&self) -> &ReportResult {
        &self.preview
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn select(&mut self, selection: Option<ReportSelection>) -> Result<(), ReportError> {
        if selection.is_some_and(|s| s.item >= s.band.band(self.definition()).items.len()) {
            return Err(error("Unknown report item"));
        }
        self.current.selection = selection;
        Ok(())
    }
    /// Validates the entire candidate (including bindings) before changing history.
    pub fn replace(
        &mut self,
        expected: ReportRevision,
        definition: ReportDefinition,
    ) -> Result<(), ReportError> {
        self.accept(expected, definition, self.selection())
    }
    fn accept(
        &mut self,
        expected: ReportRevision,
        definition: ReportDefinition,
        selection: Option<ReportSelection>,
    ) -> Result<(), ReportError> {
        if expected != self.revision() {
            return Err(error("Stale report edit"));
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| error("Revision exhausted"))?;
        let bytes = definition.to_json()?.len();
        let preview = ReportEngine::build(
            &definition,
            &self.table,
            &self.parameters,
            &mut TextEngine::new(),
            || false,
        )?;
        let selection = selection.filter(|s| s.item < s.band.band(&definition).items.len());
        self.undo.push_back(std::mem::replace(
            &mut self.current,
            Snapshot {
                definition,
                selection,
                bytes,
            },
        ));
        trim(&mut self.undo);
        self.redo.clear();
        self.preview = preview;
        self.revision = revision;
        Ok(())
    }
    pub fn add(&mut self, band: ReportBandKind, item: ReportItem) -> Result<(), ReportError> {
        let mut d = self.definition().clone();
        let b = band.band_mut(&mut d);
        let index = b.items.len();
        b.items.push(item);
        self.accept(
            self.revision(),
            d,
            Some(ReportSelection { band, item: index }),
        )
    }
    pub fn replace_selected(&mut self, item: ReportItem) -> Result<(), ReportError> {
        let s = self.selection().ok_or_else(|| error("Select an item"))?;
        let mut d = self.definition().clone();
        s.band.band_mut(&mut d).items[s.item] = item;
        self.accept(self.revision(), d, Some(s))
    }
    pub fn delete_selected(&mut self) -> Result<(), ReportError> {
        let s = self.selection().ok_or_else(|| error("Select an item"))?;
        let mut d = self.definition().clone();
        s.band.band_mut(&mut d).items.remove(s.item);
        self.accept(self.revision(), d, None)
    }
    /// Grid snapping is applied to position, never to text/font values.
    pub fn move_selected(&mut self, x: f32, y: f32, grid: Option<f32>) -> Result<(), ReportError> {
        let s = self.selection().ok_or_else(|| error("Select an item"))?;
        if !x.is_finite()
            || !y.is_finite()
            || grid.is_some_and(|g| !g.is_finite() || g <= 0.0 || g > 256.0)
        {
            return Err(error("Invalid position/grid"));
        }
        let snap = |v: f32| grid.map_or(v, |g| (v / g).round() * g);
        let mut item = s.band.band(self.definition()).items[s.item].clone();
        match &mut item {
            ReportItem::Text { x: a, y: b, .. } => {
                *a = snap(x);
                *b = snap(y)
            }
            ReportItem::Rectangle { bounds, .. } | ReportItem::Image { bounds, .. } => {
                bounds.x = snap(x);
                bounds.y = snap(y)
            }
        }
        self.replace_selected(item)
    }
    pub fn undo(&mut self) -> Result<bool, ReportError> {
        self.history(false)
    }
    pub fn redo(&mut self) -> Result<bool, ReportError> {
        self.history(true)
    }
    fn history(&mut self, redo: bool) -> Result<bool, ReportError> {
        let stack = if redo { &self.redo } else { &self.undo };
        let Some(candidate) = stack.back() else {
            return Ok(false);
        };
        let revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| error("Revision exhausted"))?;
        let preview = ReportEngine::build(
            &candidate.definition,
            &self.table,
            &self.parameters,
            &mut TextEngine::new(),
            || false,
        )?;
        let candidate = if redo {
            self.redo.pop_back().unwrap()
        } else {
            self.undo.pop_back().unwrap()
        };
        let old = std::mem::replace(&mut self.current, candidate);
        if redo {
            self.undo.push_back(old);
            trim(&mut self.undo)
        } else {
            self.redo.push_back(old);
            trim(&mut self.redo)
        }
        self.preview = preview;
        self.revision = revision;
        Ok(true)
    }
    pub fn save(&self) -> Result<String, ReportError> {
        self.definition().to_json()
    }
    pub fn restore(&mut self, expected: ReportRevision, json: &str) -> Result<(), ReportError> {
        let d = ReportDefinition::from_json(json, self.table.schema())?;
        self.accept(expected, d, None)
    }
    /// A failed parameter/data update leaves both preview and editable definition intact.
    pub fn set_data(
        &mut self,
        table: DataTable,
        parameters: BTreeMap<String, CellValue>,
    ) -> Result<(), ReportError> {
        let revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| error("Revision exhausted"))?;
        let preview = ReportEngine::build(
            self.definition(),
            &table,
            &parameters,
            &mut TextEngine::new(),
            || false,
        )?;
        self.table = table;
        self.parameters = parameters;
        self.preview = preview;
        self.revision = revision;
        // History may contain bindings absent from the new schema.
        self.undo.clear();
        self.redo.clear();
        Ok(())
    }
}
fn trim(stack: &mut VecDeque<Snapshot>) {
    while stack.len() > 64 || stack.iter().map(|s| s.bytes).sum::<usize>() > 16 * 1024 * 1024 {
        stack.pop_front();
    }
}
