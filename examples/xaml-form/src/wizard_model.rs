//! Application-owned delayed guards for a retained project setup workflow.
use crate::view::MountedView;
use rust_desktop_ui_controls::{
    BindingContext, Controls, DataValue, WizardCompletion, WizardDecision, WizardEventKind,
    WizardIntent, WizardRequest,
};
use rust_desktop_ui_core::WidgetId;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

pub(crate) fn model() -> Result<Rc<RefCell<BindingContext>>, String> {
    BindingContext::new(DataValue::object([("ReadOnly", false.into())]))
        .map(|m| Rc::new(RefCell::new(m)))
        .map_err(err)
}
pub(crate) fn register(ui: &mut Controls) {
    ui.commands_mut().register("WizardReset", "Начать заново");
    ui.commands_mut().register("WizardLate", "Поздний ответ");
}
struct Pending {
    request: WizardRequest,
    decision: WizardDecision,
    at: Instant,
}
pub(crate) struct State {
    owner: WidgetId,
    name: WidgetId,
    remote: WidgetId,
    endpoint: WidgetId,
    folder: WidgetId,
    review: WidgetId,
    summary: WidgetId,
    fingerprint: (String, bool, String, String),
    version: u64,
    pending: Vec<Pending>,
    transitions: u64,
    rejected: u64,
    finished: u64,
    cancelled: u64,
    stale: u64,
}
impl State {
    pub(crate) fn install(ui: &mut Controls, view: &MountedView) -> Result<Option<Self>, String> {
        let Some(owner) = view.find_name("ProjectWizard") else {
            return Ok(None);
        };
        let id = |name| {
            view.find_name(name)
                .ok_or_else(|| format!("Missing {name}"))
        };
        let mut state = Self {
            owner,
            name: id("WizardProjectName")?,
            remote: id("WizardRemote")?,
            endpoint: id("WizardEndpoint")?,
            folder: id("WizardFolder")?,
            review: id("WizardReview")?,
            summary: id("WizardSummary")?,
            fingerprint: Default::default(),
            version: 0,
            pending: vec![],
            transitions: 0,
            rejected: 0,
            finished: 0,
            cancelled: 0,
            stale: 0,
        };
        state.fingerprint = state.values(ui);
        Ok(Some(state))
    }
    fn values(&self, ui: &Controls) -> (String, bool, String, String) {
        let text = |id| ui.document(id).map_or("", |d| d.text()).to_owned();
        (
            text(self.name),
            ui.checked(self.remote).unwrap_or(false),
            text(self.endpoint),
            text(self.folder),
        )
    }
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.pending.iter().map(|p| p.at).min()
    }
    fn decision(&self, request: &WizardRequest) -> WizardDecision {
        match request.intent() {
            WizardIntent::Next => match request.step() {
                "project" if self.fingerprint.0.trim().is_empty() => {
                    WizardDecision::Reject("Укажите название проекта".into())
                }
                "project" => WizardDecision::Step(
                    if self.fingerprint.1 {
                        "remote"
                    } else {
                        "local"
                    }
                    .into(),
                ),
                "remote" if !self.fingerprint.2.starts_with("server:") => {
                    WizardDecision::Reject("Адрес должен начинаться с server:".into())
                }
                "local" if self.fingerprint.3.trim().is_empty() => {
                    WizardDecision::Reject("Укажите имя локальной папки".into())
                }
                _ => WizardDecision::Step("review".into()),
            },
            WizardIntent::Back => WizardDecision::Step(request.targets()[0].clone()),
            WizardIntent::Finish => WizardDecision::Finish,
            WizardIntent::Cancel => WizardDecision::Cancel,
        }
    }
    pub(crate) fn refresh(&mut self, ui: &mut Controls) -> Result<(), String> {
        self.refresh_at(ui, Instant::now())
    }
    pub(super) fn refresh_at(&mut self, ui: &mut Controls, now: Instant) -> Result<(), String> {
        let values = self.values(ui);
        if values != self.fingerprint {
            self.version = self
                .version
                .checked_add(1)
                .ok_or("Wizard demo version exhausted")?;
            ui.set_wizard_context_version(self.owner, self.version)
                .map_err(err)?;
            self.fingerprint = values;
        }
        for request in ui.drain_wizard_requests() {
            if self.pending.len() == 64 {
                self.pending.remove(0);
            }
            let decision = self.decision(&request);
            self.pending.push(Pending {
                request,
                decision,
                at: now + Duration::from_millis(350),
            });
        }
        let pending = std::mem::take(&mut self.pending);
        for p in pending {
            if p.at > now {
                self.pending.push(p);
                continue;
            }
            if ui.complete_wizard(&p.request, p.decision).map_err(err)? == WizardCompletion::Stale {
                self.stale += 1;
            }
        }
        for event in ui.drain_wizard_events() {
            match event.kind {
                WizardEventKind::StepChanged { .. } => self.transitions += 1,
                WizardEventKind::Rejected { .. } => self.rejected += 1,
                WizardEventKind::Finished => self.finished += 1,
                WizardEventKind::Cancelled => self.cancelled += 1,
                WizardEventKind::Reset => {}
            }
        }
        ui.set_label(
            self.review,
            format!(
                "Проект: {} · маршрут: {} · папка: {}",
                self.fingerprint.0,
                if self.fingerprint.1 {
                    "удалённый"
                } else {
                    "локальный"
                },
                self.fingerprint.3
            ),
        )
        .map_err(err)?;
        ui.set_label(self.summary,format!("Шаг: {} · переходов: {} · ошибок: {} · завершений: {} · отмен: {} · устаревших: {} · ожидает: {} · результат: {:?}",ui.wizard_current_step(self.owner).unwrap_or(""),self.transitions,self.rejected,self.finished,self.cancelled,self.stale,self.pending.len(),ui.wizard_result(self.owner))).map_err(err)
    }
    pub(crate) fn command(&mut self, ui: &mut Controls, command: &str) -> Result<bool, String> {
        match command {
            "WizardReset" => ui.reset_wizard(self.owner).map_err(err)?,
            "WizardLate" => {
                if ui.wizard_result(self.owner).is_some()
                    || ui.wizard_status(self.owner).is_some_and(|s| {
                        matches!(s, rust_desktop_ui_controls::WizardStatus::Pending(_))
                    })
                    || ui
                        .wizard_model(self.owner)
                        .and_then(|m| m.step(ui.wizard_current_step(self.owner)?))
                        .is_none_or(|s| s.is_final)
                    || ui.wizard_settings(self.owner).is_some_and(|s| s.read_only)
                {
                    return Ok(true);
                }
                ui.request_wizard(self.owner, WizardIntent::Next)
                    .map_err(err)?;
                for request in ui.drain_wizard_requests() {
                    if self.pending.len() == 64 {
                        self.pending.remove(0);
                    }
                    let decision = self.decision(&request);
                    self.pending.push(Pending {
                        request,
                        decision,
                        at: Instant::now() + Duration::from_millis(600),
                    });
                }
                self.version = self
                    .version
                    .checked_add(1)
                    .ok_or("Wizard demo version exhausted")?;
                ui.set_wizard_context_version(self.owner, self.version)
                    .map_err(err)?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_controls::Theme;
    use rust_desktop_ui_core::{Mutation, Size};
    #[test]
    fn wizard_demo_mounts_through_real_form_command_registry() {
        let definition = rust_desktop_ui_xaml::parse(include_str!("../WizardWindow.xaml")).unwrap();
        let mut form = crate::form::Form::new_with_model(definition, false, Some(model().unwrap()))
            .expect("the real form must register every compiled Wizard command");
        form.ui.update(Size::new(1040.0, 760.0)).unwrap();
        let owner = form.view.find_name("ProjectWizard").unwrap();
        assert_eq!(form.ui.wizard_current_step(owner), Some("project"));
        let editor = form.view.find_name("WizardProjectName").unwrap();
        assert!(form.ui.runtime.bounds(editor).is_some());
    }

    #[test]
    fn wizard_demo_compiles_and_delayed_guard_preserves_empty_name_then_takes_chosen_branch() {
        let definition = rust_desktop_ui_xaml::parse(include_str!("../WizardWindow.xaml"))
            .unwrap()
            .content;
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        register(&mut ui);
        for (id, label) in [("Light", "Light"), ("Dark", "Dark")] {
            ui.commands_mut().register(id, label);
        }
        let root = ui.root();
        let view =
            MountedView::mount_with_context(&mut ui, root, &definition, model().unwrap()).unwrap();
        let mut state = State::install(&mut ui, &view).unwrap().unwrap();
        ui.update(Size::new(1040.0, 760.0)).unwrap();
        state.refresh(&mut ui).unwrap();
        let owner = view.find_name("ProjectWizard").unwrap();
        let editor = view.find_name("WizardProjectName").unwrap();
        ui.request_wizard(owner, WizardIntent::Next).unwrap();
        state.refresh(&mut ui).unwrap();
        assert!(state.deadline().is_some());
        assert_eq!(ui.wizard_current_step(owner), Some("project"));
        state
            .refresh_at(&mut ui, state.deadline().unwrap())
            .unwrap();
        assert!(
            matches!(ui.wizard_status(owner),Some(rust_desktop_ui_controls::WizardStatus::Error(message)) if message.contains("название"))
        );
        assert_eq!(ui.wizard_current_step(owner), Some("project"));
        assert_eq!(ui.accepted_value(editor), Some(""));
        assert!(state.command(&mut ui, "WizardReset").unwrap());
        state.refresh(&mut ui).unwrap();
        ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
        ui.set_text(editor, "Example").unwrap();
        state.refresh(&mut ui).unwrap();
        ui.request_wizard(owner, WizardIntent::Next).unwrap();
        state.refresh(&mut ui).unwrap();
        state
            .refresh_at(&mut ui, state.deadline().unwrap())
            .unwrap();
        assert_eq!(ui.document(editor).unwrap().text(), "Example");
        assert_eq!(ui.accepted_value(editor), Some(""));
        assert_eq!(ui.wizard_current_step(owner), Some("local"));
    }
}
