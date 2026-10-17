//! Opt-in workflow diagnostics for the example's native checks.
use super::*;
pub(super) fn snapshot(ui: &Controls, view: &MountedView) -> String {
    view.names.iter().filter_map(|(name,&id)|{
        let p=ui.wizard_parts(id)?;let model=ui.wizard_model(id)?;
        let part=|id|ui.runtime.bounds(id).zip(ui.runtime.clip(id)).and_then(|(bounds,clip)|bounds.intersection(clip)).map_or_else(||"null".into(),rectangle);
        let steps=model.steps().iter().map(|s|format!("{{\"id\":{},\"title\":{},\"page\":{},\"indicator\":{}}}",string(&s.id),string(&s.title),part(ui.wizard_page(id,&s.id).unwrap()),part(ui.step_progress_item(p.progress.container,&s.id).unwrap()))).collect::<Vec<_>>().join(",");
        Some(format!("{{\"name\":{},\"current\":{},\"status\":{},\"result\":{},\"history\":[{}],\"back\":{},\"next\":{},\"finish\":{},\"cancel\":{},\"status_bounds\":{},\"status_text\":{},\"steps\":[{}]}}",string(name),string(ui.wizard_current_step(id).unwrap_or("")),string(&format!("{:?}",ui.wizard_status(id))),string(&format!("{:?}",ui.wizard_result(id))),ui.wizard_history(id).unwrap_or_default().iter().map(|s|string(s)).collect::<Vec<_>>().join(","),part(p.back),part(p.next),part(p.finish),part(p.cancel),part(p.status),string(ui.label_text(p.status).unwrap_or("")),steps))
    }).collect::<Vec<_>>().join(",")
}
