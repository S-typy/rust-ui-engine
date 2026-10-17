//! Opt-in example diagnostics for accepted and previewed gallery samples.
use crate::{
    popup_diagnostics::{rectangle, string},
    view::MountedView,
};
use rust_desktop_ui_controls::Controls;
pub(super) fn snapshot(ui: &Controls, view: &MountedView) -> String {
    let mut names = view.names.iter().collect::<Vec<_>>();
    names.sort_by_key(|(name, _)| *name);
    names.into_iter().filter_map(|(name,&owner)|{let model=ui.gallery_model(owner)?;let part=|id|ui.runtime.bounds(id).zip(ui.runtime.clip(id)).and_then(|(r,c)|r.intersection(c)).map_or_else(||"null".into(),rectangle);
 let items=model.items().iter().map(|item|{let id=ui.gallery_item_widget(owner,&item.item.key).unwrap();format!("{{\"key\":{},\"label\":{},\"group\":{},\"id\":{},\"bounds\":{},\"enabled\":{},\"selected\":{}}}",string(&item.item.key),string(&item.item.text),string(&item.group),string(&format!("{id:?}")),part(id),item.item.enabled,ui.gallery_selected_key(owner)==Some(item.item.key.as_str()))}).collect::<Vec<_>>().join(",");
 Some(format!("{{\"name\":{},\"owner\":{},\"parent\":{},\"selected\":{},\"preview\":{},\"error\":{},\"readonly\":{},\"popup_open\":{},\"items\":[{items}]}}",string(name),string(&format!("{owner:?}")),string(&format!("{:?}",ui.runtime.tree().parent(owner))),ui.gallery_selected_key(owner).map_or_else(||"null".into(),string),ui.gallery_preview_key(owner).map_or_else(||"null".into(),string),ui.gallery_error(owner).map_or_else(||"null".into(),string),ui.gallery_settings(owner).unwrap().is_read_only,ui.gallery_popup(owner).is_some_and(|p|ui.flyout_is_open(p))))
 }).collect::<Vec<_>>().join(",")
}
