//! Opt-in retained-state snapshots for the example's native input checks.
use crate::{
    popup_diagnostics::{rectangle, string},
    view::MountedView,
};
use rust_desktop_ui_controls::Controls;

pub(crate) fn snapshot(ui: &Controls, view: &MountedView) -> String {
    let mut names: Vec<_> = view.names.iter().collect();
    names.sort_by_key(|(name, _)| *name);
    let values = names.into_iter().map(|(name, &id)| {
        let bounds = ui.runtime.bounds(id).map_or_else(|| "null".into(), rectangle);
        let clip = ui.runtime.clip(id).map_or_else(|| "null".into(), rectangle);
        let text = ui.document(id).map(|doc| doc.text()).or_else(|| ui.label_text(id));
        let range = ui.range_slider_values(id).map_or_else(|| "null".into(), |(low, high)| format!("[{low},{high}]"));
        format!("{{\"name\":{},\"id\":{},\"bounds\":{},\"clip\":{},\"text\":{},\"accepted\":{},\"error\":{},\"pending\":{},\"check\":{},\"selected\":{},\"value\":{},\"range\":{},\"ratio\":{},\"focused\":{},\"flyout\":{}}}",
            string(name), string(&format!("{id:?}")), bounds, clip,
            text.map_or_else(|| "null".into(), string), ui.accepted_value(id).map_or_else(|| "null".into(), string),
            ui.validation_error(id).map_or_else(|| "null".into(), string), string(&format!("{:?}", ui.editor_validation_status(id))),
            ui.check_state(id).map_or_else(|| "null".into(), |state| string(&format!("{state:?}"))),
            ui.radio_selected_key(id).map_or_else(|| "null".into(), string),
            ui.slider_value(id).map_or_else(|| "null".into(), |value| value.to_string()), range,
            ui.split_ratio(id).map_or_else(|| "null".into(), |value| value.to_string()),
            ui.runtime.focused() == Some(id),
            ui.button_flyout(id).is_some_and(|flyout| ui.flyout_is_open(flyout)))
    }).collect::<Vec<_>>().join(",");
    let nodes = ui
        .semantics()
        .into_iter()
        .map(|node| {
            format!(
                "{{\"id\":{},\"role\":{},\"label\":{},\"bounds\":{},\"focused\":{},\"value\":{}}}",
                string(&format!("{:?}", node.id)),
                string(&format!("{:?}", node.role)),
                string(&node.label),
                rectangle(node.bounds),
                node.focused,
                node.value.as_deref().map_or_else(|| "null".into(), string)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"named\":[{values}],\"nodes\":[{nodes}]}}")
}
