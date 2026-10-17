//! Opt-in diagnostics for the breadcrumb example.
use super::*;
pub(super) fn snapshot(ui: &Controls, view: &MountedView) -> String {
    view.names.iter().filter_map(|(name,&id)|{
        let p=ui.breadcrumb_parts(id)?;let path=ui.breadcrumb_path(id)?;
        let part=|id|ui.runtime.bounds(id).zip(ui.runtime.clip(id)).and_then(|(bounds,clip)|bounds.intersection(clip)).map_or_else(||"null".into(),rectangle);
        let segments=path.segments.iter().map(|segment|{
            let button=ui.breadcrumb_segment_button(id,&segment.key).unwrap();let children=ui.breadcrumb_children_button(id,&segment.key).unwrap();
            format!("{{\"key\":{},\"label\":{},\"button\":{},\"children\":{},\"visible\":{}}}",string(&segment.key),string(&segment.label),part(button),part(children),ui.is_enabled(button))
        }).collect::<Vec<_>>().join(",");
        let items=ui.selection_items(p.list.container).unwrap_or_default().iter().map(|item|format!("{{\"key\":{},\"text\":{},\"enabled\":{}}}",string(&item.key),string(&item.text),item.enabled)).collect::<Vec<_>>().join(",");
        Some(format!("{{\"name\":{},\"path\":{},\"draft\":{},\"editing\":{},\"status\":{},\"children_status\":{},\"editor\":{},\"overflow\":{},\"history\":{},\"edit\":{},\"popup_open\":{},\"list\":{},\"history_count\":{},\"segments\":[{segments}],\"items\":[{items}]}}",string(name),string(&path.text),string(ui.breadcrumb_draft(id).unwrap_or("")),ui.breadcrumb_is_editing(id),string(&format!("{:?}",ui.breadcrumb_status(id))),string(&format!("{:?}",ui.breadcrumb_children_status(id))),part(p.editor),part(p.overflow_button),part(p.history_button),part(p.edit_button),ui.flyout_is_open(p.popup),part(p.list.container),ui.breadcrumb_history(id).map_or(0,|history|history.len())))
    }).collect::<Vec<_>>().join(",")
}
