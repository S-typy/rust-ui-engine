//! Document lifecycle coordination over the existing docking placement model.
//!
//! Applications own document contents and I/O. A close batch removes no pane
//! until every member has consented and every requested save has completed.
use crate::{
    DockError, DockGroupId, DockLayout, DockPaneId, DockPlacement, DockSnapshot, DockWindowId,
};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};
#[cfg(test)]
mod tests;

pub type DocumentId = DockPaneId;
static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DocumentPresentation {
    #[default]
    Tabbed,
    Mdi,
    Windowed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentState {
    pub id: DocumentId,
    pub revision: u64,
    pub saved_revision: u64,
}
impl DocumentState {
    pub fn dirty(&self) -> bool {
        self.revision != self.saved_revision
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentCloseScope {
    Document(DocumentId),
    Group(DockGroupId),
    Window(DockWindowId),
    All,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentRequestKind {
    Decide,
    Save,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentRequest {
    owner: u64,
    batch: u64,
    sequence: u64,
    document: DocumentId,
    revision: u64,
    kind: DocumentRequestKind,
}
impl DocumentRequest {
    pub fn document(&self) -> &DocumentId {
        &self.document
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn kind(&self) -> DocumentRequestKind {
        self.kind
    }
    pub fn batch_id(&self) -> u64 {
        self.batch
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentCloseDecision {
    Save,
    Discard,
    Cancel,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentCloseProgress {
    Pending(DocumentRequest),
    Closed {
        documents: Vec<DocumentId>,
        discarded: Vec<DocumentId>,
    },
    Cancelled,
    Failed(String),
}
#[derive(Clone, Debug)]
struct CloseBatch {
    id: u64,
    layout: DockSnapshot,
    revisions: Vec<(DocumentId, u64)>,
    index: usize,
    request: Option<DocumentRequest>,
    discarded: Vec<DocumentId>,
}
/// A bounded lifecycle service. Placement remains exclusively in `DockLayout`.
///
/// Keep all catalog identities stable for this service's lifetime. Direct layout
/// mutation during a batch is detected before a response can close any document.
/// Saved external I/O is never claimed to be rolled back by cancellation.
#[derive(Debug)]
pub struct DocumentWorkspace {
    owner: u64,
    sequence: u64,
    states: BTreeMap<DocumentId, DocumentState>,
    active: Option<DocumentId>,
    pending: Option<CloseBatch>,
    presentation: DocumentPresentation,
}
fn invalid(s: &str) -> DockError {
    DockError(s.into())
}
impl DocumentWorkspace {
    pub fn new(layout: &DockLayout) -> Result<Self, DockError> {
        let owner = NEXT_OWNER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| invalid("Document workspace identity exhausted"))?;
        let states = layout
            .panes()
            .iter()
            .map(|p| {
                (
                    p.id.clone(),
                    DocumentState {
                        id: p.id.clone(),
                        revision: 0,
                        saved_revision: 0,
                    },
                )
            })
            .collect();
        let active = layout
            .panes()
            .iter()
            .find(|p| layout.pane_is_visible(&p.id))
            .map(|p| p.id.clone());
        Ok(Self {
            owner,
            sequence: 0,
            states,
            active,
            pending: None,
            presentation: DocumentPresentation::Tabbed,
        })
    }
    /// Register an application-created document without opening it. New keys
    /// start clean and closed; the same 64-document docking bound applies.
    pub fn register(
        &mut self,
        layout: &mut DockLayout,
        pane: crate::DockPane,
    ) -> Result<(), DockError> {
        self.check_catalog(layout)?;
        if self.pending.is_some() {
            return Err(invalid("Document close is pending"));
        }
        let id = pane.id.clone();
        layout.register_closed_pane(pane)?;
        self.states.insert(
            id.clone(),
            DocumentState {
                id,
                revision: 0,
                saved_revision: 0,
            },
        );
        Ok(())
    }

    pub fn documents(&self) -> impl Iterator<Item = &DocumentState> {
        self.states.values()
    }
    pub fn document(&self, id: &DocumentId) -> Option<&DocumentState> {
        self.states.get(id)
    }
    pub fn active(&self) -> Option<&DocumentId> {
        self.active.as_ref()
    }
    pub fn presentation(&self) -> DocumentPresentation {
        self.presentation
    }
    pub fn pending_request(&self) -> Option<&DocumentRequest> {
        self.pending.as_ref()?.request.as_ref()
    }
    pub fn close_pending(&self) -> bool {
        self.pending.is_some()
    }
    fn check_catalog(&self, layout: &DockLayout) -> Result<(), DockError> {
        if layout.panes().len() != self.states.len()
            || layout
                .panes()
                .iter()
                .any(|p| !self.states.contains_key(&p.id))
        {
            return Err(invalid("Document catalog changed"));
        }
        Ok(())
    }
    fn next(&self) -> Result<u64, DockError> {
        self.sequence
            .checked_add(1)
            .ok_or_else(|| invalid("Document request identity exhausted"))
    }
    pub fn mark_changed(&mut self, id: &DocumentId) -> Result<u64, DockError> {
        let state = self
            .states
            .get_mut(id)
            .ok_or_else(|| invalid("Unknown document"))?;
        let revision = state
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("Document revision exhausted"))?;
        state.revision = revision;
        // Editing invalidates prior consent and all in-flight save completions.
        self.pending = None;
        Ok(revision)
    }
    /// Acknowledge external save only for the exact contents revision written.
    pub fn mark_saved(&mut self, id: &DocumentId, revision: u64) -> Result<(), DockError> {
        let state = self
            .states
            .get_mut(id)
            .ok_or_else(|| invalid("Unknown document"))?;
        if state.revision != revision {
            return Err(invalid("Saved document revision is stale"));
        }
        if self.pending.is_some() {
            return Err(invalid("Complete the pending save request instead"));
        }
        state.saved_revision = revision;
        Ok(())
    }
    pub fn activate(
        &mut self,
        layout: &mut DockLayout,
        id: &DocumentId,
    ) -> Result<bool, DockError> {
        self.check_catalog(layout)?;
        if self.pending.is_some() {
            return Err(invalid("Document close is pending"));
        }
        let changed = layout.activate(id)?;
        self.active = Some(id.clone());
        Ok(changed)
    }
    pub fn open(
        &mut self,
        layout: &mut DockLayout,
        id: &DocumentId,
        placement: DockPlacement,
    ) -> Result<bool, DockError> {
        self.check_catalog(layout)?;
        if self.pending.is_some() {
            return Err(invalid("Document close is pending"));
        }
        if !self.states.contains_key(id) {
            return Err(invalid("Unknown document"));
        }
        let changed = if layout.is_closed(id) {
            layout.restore(id, placement)?
        } else {
            layout.activate(id)?
        };
        self.active = Some(id.clone());
        Ok(changed)
    }
    /// Mark a successfully staged host presentation. The host must preflight IME,
    /// pointer capture and candidate placement before calling this method.
    pub fn set_presentation(&mut self, value: DocumentPresentation) -> Result<(), DockError> {
        if self.pending.is_some() {
            return Err(invalid("Document close is pending"));
        }
        self.presentation = value;
        Ok(())
    }
    pub fn begin_close(
        &mut self,
        layout: &mut DockLayout,
        scope: DocumentCloseScope,
    ) -> Result<DocumentCloseProgress, DockError> {
        self.check_catalog(layout)?;
        if self.pending.is_some() {
            return Err(invalid("Document close is already pending"));
        }
        match &scope {
            DocumentCloseScope::Group(group)
                if !layout
                    .panes()
                    .iter()
                    .any(|p| layout.group_for(&p.id) == Some(*group)) =>
            {
                return Err(invalid("Unknown document group"));
            }
            DocumentCloseScope::Window(window) => {
                layout.window_panes(*window)?;
            }
            _ => {}
        }
        let ids: Vec<_> = layout
            .panes()
            .iter()
            .filter(|p| !layout.is_closed(&p.id))
            .filter(|p| match &scope {
                DocumentCloseScope::Document(id) => &p.id == id,
                DocumentCloseScope::Group(group) => layout.group_for(&p.id) == Some(*group),
                DocumentCloseScope::Window(window) => layout.window_for(&p.id) == Some(*window),
                DocumentCloseScope::All => true,
            })
            .map(|p| p.id.clone())
            .collect();
        if let DocumentCloseScope::Document(id) = &scope
            && !self.states.contains_key(id)
        {
            return Err(invalid("Unknown document"));
        }
        if ids
            .iter()
            .any(|id| !layout.pane(id).is_some_and(|p| p.closable))
        {
            return Err(invalid("A document in the close batch cannot close"));
        }
        let next = self.next()?;
        let batch = CloseBatch {
            id: next,
            layout: layout.snapshot(),
            revisions: ids
                .iter()
                .map(|id| (id.clone(), self.states[id].revision))
                .collect(),
            index: 0,
            request: None,
            discarded: Vec::new(),
        };
        let mut staged = self.stage();
        staged.sequence = next;
        staged.pending = Some(batch);
        let progress = staged.advance(layout)?;
        *self = staged;
        Ok(progress)
    }
    fn stage(&self) -> Self {
        Self {
            owner: self.owner,
            sequence: self.sequence,
            states: self.states.clone(),
            active: self.active.clone(),
            pending: self.pending.clone(),
            presentation: self.presentation,
        }
    }
    fn check_response(
        &self,
        layout: &DockLayout,
        request: &DocumentRequest,
        kind: DocumentRequestKind,
    ) -> Result<(), DockError> {
        self.check_catalog(layout)?;
        let batch = self
            .pending
            .as_ref()
            .ok_or_else(|| invalid("No document close is pending"))?;
        if batch.request.as_ref() != Some(request)
            || request.owner != self.owner
            || request.kind != kind
        {
            return Err(invalid("Stale or foreign document completion"));
        }
        if layout.snapshot() != batch.layout
            || batch
                .revisions
                .iter()
                .any(|(id, revision)| self.states[id].revision != *revision)
        {
            return Err(invalid("Document close context changed"));
        }
        Ok(())
    }
    pub fn decide(
        &mut self,
        layout: &mut DockLayout,
        request: &DocumentRequest,
        decision: DocumentCloseDecision,
    ) -> Result<DocumentCloseProgress, DockError> {
        self.check_response(layout, request, DocumentRequestKind::Decide)?;
        if decision == DocumentCloseDecision::Cancel {
            self.pending = None;
            return Ok(DocumentCloseProgress::Cancelled);
        }
        let mut staged = self.stage();
        if decision == DocumentCloseDecision::Save {
            let sequence = staged.next()?;
            staged.sequence = sequence;
            let request = DocumentRequest {
                sequence,
                kind: DocumentRequestKind::Save,
                ..request.clone()
            };
            staged.pending.as_mut().unwrap().request = Some(request.clone());
            *self = staged;
            return Ok(DocumentCloseProgress::Pending(request));
        }
        let batch = staged.pending.as_mut().unwrap();
        batch.discarded.push(request.document.clone());
        batch.index += 1;
        batch.request = None;
        let progress = staged.advance(layout)?;
        *self = staged;
        Ok(progress)
    }
    pub fn complete_save(
        &mut self,
        layout: &mut DockLayout,
        request: &DocumentRequest,
        result: Result<(), String>,
    ) -> Result<DocumentCloseProgress, DockError> {
        self.check_response(layout, request, DocumentRequestKind::Save)?;
        if let Err(message) = result {
            if message.len() > 4096 || message.chars().any(|c| c == '\0') {
                return Err(invalid("Save error exceeds diagnostic limits"));
            }
            self.pending = None;
            return Ok(DocumentCloseProgress::Failed(message));
        }
        let mut staged = self.stage();
        staged
            .states
            .get_mut(&request.document)
            .unwrap()
            .saved_revision = request.revision;
        let batch = staged.pending.as_mut().unwrap();
        batch.index += 1;
        batch.request = None;
        let progress = staged.advance(layout)?;
        *self = staged;
        Ok(progress)
    }
    /// Explicit cancellation remains available after an external context change.
    pub fn cancel_close(&mut self) -> bool {
        self.pending.take().is_some()
    }
    fn advance(&mut self, layout: &mut DockLayout) -> Result<DocumentCloseProgress, DockError> {
        loop {
            let batch = self.pending.as_ref().unwrap();
            if batch.index == batch.revisions.len() {
                let mut candidate = layout.clone();
                for (id, _) in &batch.revisions {
                    candidate.close(id)?;
                }
                let closed = DocumentCloseProgress::Closed {
                    documents: batch.revisions.iter().map(|(id, _)| id.clone()).collect(),
                    discarded: batch.discarded.clone(),
                };
                *layout = candidate;
                if self.active.as_ref().is_some_and(|id| layout.is_closed(id)) {
                    self.active = layout
                        .panes()
                        .iter()
                        .find(|p| layout.pane_is_visible(&p.id))
                        .map(|p| p.id.clone());
                }
                self.pending = None;
                return Ok(closed);
            }
            let (id, revision) = batch.revisions[batch.index].clone();
            if self.states[&id].dirty() {
                let sequence = self.next()?;
                self.sequence = sequence;
                let request = DocumentRequest {
                    owner: self.owner,
                    batch: batch.id,
                    sequence,
                    document: id,
                    revision,
                    kind: DocumentRequestKind::Decide,
                };
                self.pending.as_mut().unwrap().request = Some(request.clone());
                return Ok(DocumentCloseProgress::Pending(request));
            }
            self.pending.as_mut().unwrap().index += 1;
        }
    }
}
