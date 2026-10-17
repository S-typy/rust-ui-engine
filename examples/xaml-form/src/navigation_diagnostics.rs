//! Opt-in example diagnostics for retained navigation.
use crate::{
    popup_diagnostics::{rectangle, string},
    view::MountedView,
};
use rust_desktop_ui_controls::Controls;
pub(super) fn snapshot(ui: &Controls, view: &MountedView) -> String {
    let mut names: Vec<_> = view.names.iter().collect();
    names.sort_by_key(|(name, _)| *name);
    names.into_iter().filter_map(|(name,&owner)| {
        let model=ui.navigation_model(owner)?;let parts=ui.navigation_parts(owner)?;
        let part=|id| ui.runtime.bounds(id).zip(ui.runtime.clip(id)).and_then(|(r,c)|r.intersection(c)).map_or_else(||"null".into(),rectangle);
        let pages=model.pages().iter().map(|p| {
            let host=ui.navigation_page_host(owner,&p.route).unwrap();let selector=ui.navigation_selector(owner,&p.route).unwrap();
            format!("{{\"route\":{},\"section\":{},\"host\":{},\"host_bounds\":{},\"selector\":{},\"enabled\":{},\"selected\":{}}}",string(&p.route),string(&p.section),string(&format!("{host:?}")),part(host),part(selector),p.enabled,p.route==model.route())
        }).collect::<Vec<_>>().join(",");
        let history=model.history().iter().map(|r|string(r)).collect::<Vec<_>>().join(",");
        Some(format!("{{\"name\":{},\"route\":{},\"view\":{},\"error\":{},\"history\":[{history}],\"position\":{},\"back\":{},\"forward\":{},\"menu\":{},\"pages\":[{pages}]}}",string(name),string(model.route()),string(&format!("{:?}",ui.navigation_settings(owner).unwrap().view)),ui.navigation_error(owner).map_or_else(||"null".into(),string),model.history_position(),part(parts.back_button),part(parts.forward_button),part(parts.menu_button)))
    }).collect::<Vec<_>>().join(",")
}
