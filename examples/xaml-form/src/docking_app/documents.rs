//! Application-owned document contents and native save/close prompts.
use super::*;
#[cfg(test)]
mod tests;
use rust_desktop_ui_controls::document_workspace::*;
use std::time::Instant;
type Contents = BTreeMap<String, String>;
pub(super) struct DocumentHost {
    pub controller: DocumentWorkspace,
    pub mdi: DockMdiLayout,
    observed: BTreeMap<DocumentId, Contents>,
    saved: BTreeMap<DocumentId, Contents>,
    owner: DockWindowId,
    close_app: bool,
    dialog_id: u64,
    queued: Option<(u64, DocumentRequest)>,
    shown: Option<(u64, DocumentRequest)>,
    save: Option<(Instant, DocumentRequest, Contents)>,
    pub fail_next_save: bool,
}
fn contents(form: &Form) -> Contents {
    form.view
        .editors
        .iter()
        .filter_map(|id| {
            let name = form.view.names.iter().find(|(_, v)| *v == id)?.0;
            Some((name.clone(), form.ui.document(*id)?.text().to_owned()))
        })
        .collect()
}
impl DocumentHost {
    fn new(state: &Workspace) -> Result<Self, String> {
        let observed = state
            .panes
            .iter()
            .map(|(id, form)| (id.clone(), contents(form)))
            .collect();
        Ok(Self {
            controller: DocumentWorkspace::new(&state.layout).map_err(err)?,
            mdi: DockMdiLayout::new(),
            saved: BTreeMap::new(),
            observed,
            owner: DockWindowId::MAIN,
            close_app: false,
            dialog_id: 0,
            queued: None,
            shown: None,
            save: None,
            fail_next_save: false,
        })
    }
    fn progress(
        &mut self,
        state: &mut Workspace,
        progress: DocumentCloseProgress,
    ) -> Result<(), String> {
        match progress {
            DocumentCloseProgress::Pending(request) => match request.kind() {
                DocumentRequestKind::Decide => {
                    self.dialog_id = self
                        .dialog_id
                        .checked_add(1)
                        .ok_or("Document dialog identity exhausted")?;
                    state.layout_status = format!(
                        "Ожидается решение: {}. Все документы ещё открыты.",
                        request.document()
                    );
                    self.queued = Some((self.dialog_id, request));
                }
                DocumentRequestKind::Save => {
                    let value = contents(&state.panes[request.document()]);
                    state.layout_status = format!("Сохранение {}…", request.document());
                    self.save = Some((Instant::now() + Duration::from_millis(350), request, value));
                }
            },
            DocumentCloseProgress::Closed { documents, .. } => {
                state.layout_status = format!(
                    "Закрыто документов: {}. Содержимое сохранено в памяти приложения.",
                    documents.len()
                );
                if self.close_app {
                    state.alive = false;
                }
                self.close_app = false;
            }
            DocumentCloseProgress::Cancelled => {
                state.layout_status = "Закрытие отменено. Все документы остались открытыми.".into();
                self.close_app = false;
            }
            DocumentCloseProgress::Failed(message) => {
                state.layout_status =
                    format!("Ошибка сохранения: {message}. Ни один документ не закрыт.");
                self.close_app = false;
            }
        }
        Ok(())
    }
    fn begin(
        &mut self,
        state: &mut Workspace,
        window: DockWindowId,
        scope: DocumentCloseScope,
        close_app: bool,
    ) -> Result<(), String> {
        state.check_document_transfer()?;
        let progress = self
            .controller
            .begin_close(&mut state.layout, scope)
            .map_err(err)?;
        self.owner = window;
        self.close_app = close_app;
        self.progress(state, progress)
    }
    fn refresh(&mut self, state: &mut Workspace) -> Result<(), String> {
        let mut changed = false;
        for (id, form) in &state.panes {
            let current = contents(form);
            if self.observed.get(id) != Some(&current) {
                self.controller.mark_changed(id).map_err(err)?;
                self.observed.insert(id.clone(), current);
                changed = true;
            }
        }
        if changed {
            state.layout_status = format!(
                "Документ изменён. Несохранённых: {}.",
                self.controller.documents().filter(|d| d.dirty()).count()
            );
            if !self.controller.close_pending() {
                self.queued = None;
                self.save = None;
                self.close_app = false;
            }
        }
        if let Some((when, request, value)) = self.save.clone()
            && when <= Instant::now()
        {
            self.save = None;
            if self.controller.pending_request() == Some(&request) {
                let result = if std::mem::take(&mut self.fail_next_save) {
                    Err("Проверочная ошибка хранилища".into())
                } else {
                    self.saved.insert(request.document().clone(), value);
                    Ok(())
                };
                match self
                    .controller
                    .complete_save(&mut state.layout, &request, result)
                {
                    Ok(progress) => self.progress(state, progress)?,
                    Err(_) => self.cancel_changed_context(state),
                }
            }
        }
        Ok(())
    }
    fn cancel_changed_context(&mut self, state: &mut Workspace) {
        self.controller.cancel_close();
        self.queued = None;
        self.save = None;
        self.close_app = false;
        state.layout_status = "Контекст закрытия изменился. Документы остались открытыми.".into();
    }
    fn switch(&mut self, state: &mut Workspace, view: DocumentPresentation) -> Result<(), String> {
        state.check_document_transfer()?;
        if self.controller.close_pending() {
            return Err("Сначала завершите закрытие документов".into());
        }
        if self.controller.presentation() == view {
            return Ok(());
        }
        let mut candidate = state.layout.clone();
        let ids = candidate
            .panes()
            .iter()
            .filter(|p| !candidate.is_closed(&p.id))
            .map(|p| p.id.clone())
            .collect::<Vec<_>>();
        let mut snapshot = candidate.snapshot();
        snapshot.floating.clear();
        snapshot.auto_hide.clear();
        snapshot.root = ids.first().map(|first| DockNode::Tabs {
            id: DockGroupId(1),
            panes: ids.clone(),
            active: if ids.contains(&state.active) {
                state.active.clone()
            } else {
                first.clone()
            },
        });
        candidate.restore_snapshot(snapshot).map_err(err)?;
        if view == DocumentPresentation::Windowed {
            for (i, id) in ids.iter().enumerate() {
                candidate
                    .float_pane(
                        id,
                        Rect::new(100. + 36. * i as f32, 100. + 36. * i as f32, 640., 440.),
                    )
                    .map_err(err)?;
            }
        }
        let revision = state
            .placement_revision
            .checked_add(1)
            .ok_or("Placement revision exhausted")?;
        let mut mdi = self.mdi.clone();
        mdi.synchronize(&candidate, DockWindowId::MAIN)
            .map_err(err)?;
        if view == DocumentPresentation::Mdi && mdi.order().contains(&state.active) {
            mdi.activate(&state.active).map_err(err)?;
        }
        self.controller.set_presentation(view).map_err(err)?;
        self.mdi = mdi;
        state.layout = candidate;
        state.placement_revision = revision;
        state.drag = None;
        state.drop_targets.clear();
        state.layout_status = format!("Представление {view:?}; документы и редакторы сохранены.");
        Ok(())
    }
    fn command(
        &mut self,
        state: &mut Workspace,
        window: DockWindowId,
        command: &str,
    ) -> Result<bool, String> {
        match command {
            "DocTabbed" => self.switch(state, DocumentPresentation::Tabbed)?,
            "DocMdi" => self.switch(state, DocumentPresentation::Mdi)?,
            "DocWindowed" => self.switch(state, DocumentPresentation::Windowed)?,
            "DocClose" => self.begin(
                state,
                window,
                DocumentCloseScope::Document(state.active.clone()),
                false,
            )?,
            "DocCloseGroup" => {
                let group = state
                    .layout
                    .group_for(&state.active)
                    .ok_or("Нет активного документа")?;
                self.begin(state, window, DocumentCloseScope::Group(group), false)?;
            }
            "DocCloseAll" => self.begin(state, window, DocumentCloseScope::All, false)?,
            "DocSave" => {
                state.check_document_transfer()?;
                if self.controller.close_pending() {
                    return Err("Закрытие уже ожидает решения".into());
                }
                let revision = self
                    .controller
                    .document(&state.active)
                    .ok_or("Нет документа")?
                    .revision;
                if std::mem::take(&mut self.fail_next_save) {
                    state.layout_status =
                        "Проверочная ошибка сохранения; изменения остались.".into();
                } else {
                    self.saved
                        .insert(state.active.clone(), contents(&state.panes[&state.active]));
                    self.controller
                        .mark_saved(&state.active, revision)
                        .map_err(err)?;
                    state.layout_status = "Сохранено в памяти приложения.".into();
                }
            }
            "DocFailSave" => {
                self.fail_next_save = true;
                state.layout_status = "Следующее сохранение завершится проверочной ошибкой.".into();
            }
            "DocOpenAll" => {
                state.check_document_transfer()?;
                for id in state.layout.snapshot().closed {
                    let geometry = state
                        .layout
                        .arrange(DockWindowId::MAIN, Rect::new(0., 0., 1000., 600.))
                        .map_err(err)?;
                    let placement = geometry.groups.first().map_or(
                        DockPlacement::Root {
                            window: DockWindowId::MAIN,
                            edge: DockEdge::Right,
                        },
                        |g| DockPlacement::Tab {
                            group: g.id,
                            index: *g.insertion_slots.last().unwrap(),
                        },
                    );
                    self.controller
                        .open(&mut state.layout, &id, placement)
                        .map_err(err)?;
                }
                if let Some(active) = self.controller.active() {
                    state.active = active.clone();
                }
                state.layout_status = "Документы снова открыты с прежними редакторами.".into();
            }
            "DocCascade" | "DocTile" => {
                state.check_document_transfer()?;
                if self.controller.presentation() != DocumentPresentation::Mdi {
                    return Err("Сначала включите MDI".into());
                }
                let size = state.native_geometry.get(&DockWindowId::MAIN).map_or(
                    Size::new(1100., 650.),
                    |g| {
                        Size::new(
                            (g.client_size.width - 16.).max(1.),
                            (g.client_size.height - 240.).max(1.),
                        )
                    },
                );
                if command == "DocCascade" {
                    self.mdi.cascade(&state.layout, size).map_err(err)?;
                } else {
                    self.mdi.tile(&state.layout, size).map_err(err)?;
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}
impl Workspace {
    pub(super) fn mdi_mode(&self) -> bool {
        self.documents
            .as_ref()
            .is_some_and(|d| d.controller.presentation() == DocumentPresentation::Mdi)
    }
    pub(super) fn pane_visible(&self, id: &DockPaneId) -> bool {
        if self.mdi_mode() && self.layout.window_for(id) == Some(DockWindowId::MAIN) {
            self.documents
                .as_ref()
                .unwrap()
                .mdi
                .maximized()
                .is_none_or(|max| max == id)
        } else {
            self.layout.pane_is_visible(id)
        }
    }
    fn with_documents<R>(
        &mut self,
        f: impl FnOnce(&mut DocumentHost, &mut Self) -> Result<R, String>,
    ) -> Result<Option<R>, String> {
        let Some(mut d) = self.documents.take() else {
            return Ok(None);
        };
        let result = f(&mut d, self);
        self.documents = Some(d);
        if result.is_ok() {
            self.reconcile_focus()?;
        }
        result.map(Some)
    }
    pub(super) fn check_document_transfer(&self) -> Result<(), String> {
        if self
            .panes
            .values()
            .any(|p| p.ime_composition_active() == Some(true) || p.ui.runtime.captured().is_some())
        {
            return Err("Завершите ввод или перетаскивание перед переходом документа".into());
        }
        Ok(())
    }
    pub(super) fn refresh_documents(&mut self) -> Result<bool, String> {
        if self.documents.is_none() {
            return Ok(false);
        }
        let before = (
            self.alive,
            self.layout_status.clone(),
            self.layout.snapshot(),
        );
        self.with_documents(|d, s| d.refresh(s))?;
        Ok(before
            != (
                self.alive,
                self.layout_status.clone(),
                self.layout.snapshot(),
            ))
    }
    pub(super) fn document_command(
        &mut self,
        window: DockWindowId,
        command: &str,
    ) -> Result<bool, String> {
        Ok(self
            .with_documents(|d, s| d.command(s, window, command))?
            .unwrap_or(false))
    }
    pub(super) fn begin_document_close(
        &mut self,
        window: DockWindowId,
        scope: DocumentCloseScope,
        app: bool,
    ) -> Result<(), String> {
        self.refresh_documents()?;
        self.with_documents(|d, s| d.begin(s, window, scope, app))?;
        Ok(())
    }
    pub(super) fn document_diagnostic(&self) -> String {
        use crate::popup_diagnostics::string;
        let Some(d) = &self.documents else {
            return "null".into();
        };
        let items = d.controller.documents().map(|doc| format!("{{\"key\":{},\"revision\":{},\"saved_revision\":{},\"dirty\":{},\"closed\":{}}}", string(doc.id.as_str()), doc.revision, doc.saved_revision, doc.dirty(), self.layout.is_closed(&doc.id))).collect::<Vec<_>>().join(",");
        format!(
            "{{\"mode\":{},\"active\":{},\"pending\":{},\"status\":{},\"items\":[{}]}}",
            string(&format!("{:?}", d.controller.presentation())),
            string(self.active.as_str()),
            d.controller.close_pending(),
            string(&self.layout_status),
            items
        )
    }
    pub(super) fn document_deadline(&self) -> Option<Instant> {
        self.documents.as_ref().and_then(|d| {
            d.queued
                .as_ref()
                .map(|_| Instant::now())
                .or_else(|| d.save.as_ref().map(|(when, _, _)| *when))
        })
    }
}
impl DockApplication {
    pub(crate) fn enable_documents(&mut self) -> Result<(), String> {
        let mut state = self.state.borrow_mut();
        let host = DocumentHost::new(&state)?;
        state.documents = Some(host);
        state.layout_status = "Документы готовы. Сохранение выполняется только по команде.".into();
        Ok(())
    }
}
impl DockWindow {
    pub(super) fn document_dialog(&mut self) -> Option<platform::DialogRequest> {
        let mut state = self.state.borrow_mut();
        let d = state.documents.as_mut()?;
        if d.owner != self.id {
            return None;
        }
        let (id, request) = d.queued.take()?;
        let name = request.document().to_string();
        d.shown = Some((id, request));
        Some(platform::DialogRequest {
            id,
            options: platform::DialogOptions::Message(platform::MessageDialog {
                title: "Закрытие документов".into(),
                message: format!(
                    "Сохранить изменения документа «{name}»?\nДа — сохранить; Нет — закрыть без сохранения; Отмена — оставить весь набор открытым."
                ),
                buttons: platform::DialogButtons::YesNoCancel,
                default_button: platform::DialogButton::Cancel,
            }),
        })
    }
    pub(super) fn document_dialog_completed(
        &mut self,
        completion: platform::DialogCompletion,
    ) -> Result<EventResponse, String> {
        let mut state = self.state.borrow_mut();
        state.with_documents(|d, s| {
            let Some((id, request)) = d.shown.as_ref() else {
                return Ok(());
            };
            if *id != completion.id {
                return Ok(());
            }
            let request = request.clone();
            d.shown = None;
            if d.controller.pending_request() != Some(&request) {
                s.layout_status = "Устаревший ответ закрытия проигнорирован.".into();
                return Ok(());
            }
            let decision = match completion.result {
                Ok(platform::DialogOutcome::Button(platform::DialogButton::Yes)) => {
                    DocumentCloseDecision::Save
                }
                Ok(platform::DialogOutcome::Button(platform::DialogButton::No)) => {
                    DocumentCloseDecision::Discard
                }
                _ => DocumentCloseDecision::Cancel,
            };
            match d.controller.decide(&mut s.layout, &request, decision) {
                Ok(progress) => d.progress(s, progress),
                Err(_) => {
                    d.cancel_changed_context(s);
                    Ok(())
                }
            }
        })?;
        Ok(EventResponse {
            redraw: true,
            ..Default::default()
        })
    }
    pub(super) fn apply_mdi_requests(
        &mut self,
        requests: Vec<DockMdiRequest>,
    ) -> Result<(), String> {
        let mut state = self.state.borrow_mut();
        for request in requests {
            match request {
                DockMdiRequest::Activate(id) => {
                    state.request(self.id, DockRequest::Activate(id))?;
                }
                DockMdiRequest::Close(id) => {
                    state.begin_document_close(self.id, DocumentCloseScope::Document(id), false)?
                }
                DockMdiRequest::Float(id) => state.request(self.id, DockRequest::Float(id))?,
            }
        }
        Ok(())
    }
}
