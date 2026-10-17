//! Retained declarative workflow pages; transition guards belong to the app.
use super::*;
use rust_desktop_ui_controls::{
    StepProgressItem, StepProgressValue, StepState, WizardModel, WizardSettings, WizardStep,
};
use rust_desktop_ui_xaml::{StepProgressDefinition, StepStateDefinition, WizardDefinition};

fn model(value: &WizardDefinition) -> Result<WizardModel, String> {
    WizardModel::new(
        value
            .steps
            .iter()
            .map(|s| WizardStep {
                id: s.id.clone(),
                title: s.header.clone(),
                next: s.next.clone(),
                is_final: s.is_final,
            })
            .collect(),
        if value.start_step.is_empty() {
            value.steps.first().map_or("", |s| s.id.as_str())
        } else {
            &value.start_step
        },
    )
    .map_err(err)
}
fn progress(value: &StepProgressDefinition) -> StepProgressValue {
    StepProgressValue {
        current: value.current_step.clone(),
        steps: value
            .steps
            .iter()
            .map(|s| StepProgressItem {
                id: s.id.clone(),
                title: s.header.clone(),
                state: match s.state {
                    StepStateDefinition::Upcoming => StepState::Upcoming,
                    StepStateDefinition::Current => StepState::Current,
                    StepStateDefinition::Completed => StepState::Completed,
                    StepStateDefinition::Error => StepState::Error,
                    StepStateDefinition::Skipped => StepState::Skipped,
                },
            })
            .collect(),
    }
}
impl MountedView {
    pub(super) fn mount_wizard(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        element: &ElementDefinition,
        style: LayoutStyle,
        available_width: f32,
        context: &str,
    ) -> Result<WidgetId, String> {
        let ElementKind::Wizard(value) = &element.kind else {
            return Err("Expected Wizard".into());
        };
        let label = element
            .common
            .automation_name
            .as_deref()
            .or(element.common.name.as_deref())
            .unwrap_or("Мастер");
        let p = ui
            .wizard(
                parent,
                label,
                model(value)?,
                WizardSettings {
                    show_progress: value.show_progress,
                    allow_cancel: value.allow_cancel,
                    read_only: value.is_read_only,
                },
                style,
            )
            .map_err(err)?;
        for step in &value.steps {
            let host = ui.wizard_page(p.container, &step.id).unwrap();
            self.mount_element(
                ui,
                host,
                &step.content,
                Axis::Vertical,
                available_width,
                context,
            )?;
        }
        Ok(p.container)
    }
    pub(super) fn mount_step_progress(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        value: &StepProgressDefinition,
        label: &str,
        style: LayoutStyle,
    ) -> Result<WidgetId, String> {
        ui.step_progress(parent, label, progress(value), style)
            .map(|p| p.container)
            .map_err(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_controls::{Theme, WizardDecision, WizardIntent};
    use rust_desktop_ui_core::{Mutation, Size};
    #[test]
    fn compiled_wizard_retains_named_draft_and_requires_guard_completion() {
        let definition=rust_desktop_ui_xaml::parse("<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'><Wizard x:Name='Flow' Width='720' Height='440'><WizardStep Id='start' Header='Начало' Next='finish'><TextBox x:Name='Draft' Text='accepted'/></WizardStep><WizardStep Id='finish' Header='Готово' IsFinal='True'><TextBlock Text='Review'/></WizardStep></Wizard></Window>").unwrap().content;
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let root = ui.root();
        let view = MountedView::mount(&mut ui, root, &definition).unwrap();
        ui.update(Size::new(900.0, 620.0)).unwrap();
        let owner = view.find_name("Flow").unwrap();
        let editor = view.find_name("Draft").unwrap();
        ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
        ui.set_text(editor, "unsaved").unwrap();
        ui.request_wizard(owner, WizardIntent::Next).unwrap();
        let r = ui.drain_wizard_requests().pop().unwrap();
        assert_eq!(ui.wizard_current_step(owner), Some("start"));
        ui.complete_wizard(&r, WizardDecision::Step("finish".into()))
            .unwrap();
        ui.request_wizard(owner, WizardIntent::Back).unwrap();
        let r = ui.drain_wizard_requests().pop().unwrap();
        ui.complete_wizard(&r, WizardDecision::Step("start".into()))
            .unwrap();
        ui.update(Size::new(900.0, 620.0)).unwrap();
        assert_eq!(view.find_name("Draft"), Some(editor));
        assert_eq!(ui.document(editor).unwrap().text(), "unsaved");
        assert_eq!(ui.accepted_value(editor), Some("accepted"));
    }
}
