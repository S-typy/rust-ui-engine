//! Opt-in retained-state snapshots for the example's native input checks.
use crate::{
    popup_diagnostics::{rectangle, string},
    view::MountedView,
};
use rust_desktop_ui_controls::Controls;
#[path = "badge_diagnostics.rs"]
mod badges;
#[path = "breadcrumb_diagnostics.rs"]
mod breadcrumbs;
#[path = "wizard_diagnostics.rs"]
mod wizards;

pub(crate) fn snapshot(ui: &Controls, view: &MountedView) -> String {
    let mut names: Vec<_> = view.names.iter().collect();
    names.sort_by_key(|(name, _)| *name);
    let values = names.into_iter().map(|(name, &id)| {
        let bounds = ui.runtime.bounds(id).map_or_else(|| "null".into(), rectangle);
        let clip = ui.runtime.clip(id).map_or_else(|| "null".into(), rectangle);
        let editor = ui.composite_editor(id).or_else(|| ui.search_parts(id).map(|parts| parts.editor)).unwrap_or(id);
        let text = ui.document(editor).map(|doc| doc.text()).or_else(|| ui.label_text(id));
        let calendar = ui.temporal_calendar(id).map_or(id, |calendar| calendar.container);
        let calendar_bounds = ui.calendar_selection(calendar).and_then(|_| ui.runtime.bounds(calendar)).map_or_else(|| "null".into(), rectangle);
        let range = ui.range_slider_values(id).map_or_else(|| "null".into(), |(low, high)| format!("[{low},{high}]"));
        format!("{{\"name\":{},\"id\":{},\"bounds\":{},\"clip\":{},\"text\":{},\"accepted\":{},\"error\":{},\"pending\":{},\"check\":{},\"selected\":{},\"value\":{},\"range\":{},\"ratio\":{},\"focused\":{},\"flyout\":{},\"numeric_open\":{},\"temporal_open\":{},\"calendar_bounds\":{},\"calendar_view\":{},\"protected\":{},\"password_empty\":{},\"password_revealed\":{}}}",
            string(name), string(&format!("{id:?}")), bounds, clip,
            text.map_or_else(|| "null".into(), string), ui.accepted_value(editor).map_or_else(|| "null".into(), string),
            ui.validation_error(editor).map_or_else(|| "null".into(), string), string(&format!("{:?}", ui.editor_validation_status(editor))),
            ui.check_state(id).map_or_else(|| "null".into(), |state| string(&format!("{state:?}"))),
            ui.radio_selected_key(id).map_or_else(|| "null".into(), string),
            ui.slider_value(id).map_or_else(|| "null".into(), |value| value.to_string()), range,
            ui.split_ratio(id).map_or_else(|| "null".into(), |value| value.to_string()),
            ui.runtime.focused() == Some(editor),
            ui.button_flyout(id).is_some_and(|flyout| ui.flyout_is_open(flyout)),
            ui.numeric_calculator_is_open(id), ui.temporal_popup_is_open(id), calendar_bounds,
            ui.calendar_view(calendar).map_or_else(|| "null".into(), |value| string(&format!("{value:?}"))),
            ui.is_password_box(id),
            ui.password_value(id).map_or_else(|| "null".into(), |value| value.is_empty().to_string()),
            ui.password_is_revealed(id).map_or_else(|| "null".into(), |value| value.to_string()))
    }).collect::<Vec<_>>().join(",");
    let nodes = ui
        .semantics()
        .into_iter()
        .map(|node| {
            format!(
                "{{\"id\":{},\"role\":{},\"label\":{},\"bounds\":{},\"focused\":{},\"value\":{},\"error\":{}}}",
                string(&format!("{:?}", node.id)),
                string(&format!("{:?}", node.role)),
                string(&node.label),
                rectangle(node.bounds),
                node.focused,
                node.value.as_deref().map_or_else(|| "null".into(), string),
                node.validation_error.as_deref().map_or_else(|| "null".into(), string)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let selections = view.names.iter().filter_map(|(name, &id)| {
        let keys = ui.selected_keys(id)?;
        let list = |keys: &[String]| keys.iter().map(|key| string(key)).collect::<Vec<_>>().join(",");
        let draft = ui.combo_selection_draft(id).map_or_else(|| "null".into(), |keys| format!("[{}]", list(keys)));
        let stats = ui.list_virtualization_stats(id).map_or_else(|| "null".into(), |stats| {
            let rows = ui.selection_items(id).unwrap_or_default()[stats.range.materialized.clone()].iter().map(|item| {
                let row = ui.list_row_widget(id, &item.key);
                format!("{{\"key\":{},\"id\":{},\"bounds\":{}}}", string(&item.key), row.map_or_else(|| "null".into(), |id| string(&format!("{id:?}"))), row.and_then(|id| ui.runtime.bounds(id)).map_or_else(|| "null".into(), rectangle))
            }).collect::<Vec<_>>().join(",");
            format!("{{\"visible\":[{},{}],\"materialized\":[{},{}],\"realized\":{},\"allocated\":{},\"measured\":{},\"created\":{},\"released\":{},\"rows\":[{}]}}", stats.range.visible.start, stats.range.visible.end, stats.range.materialized.start, stats.range.materialized.end, stats.realized_rows, stats.allocated_slots, stats.measured_rows, stats.created_rows, stats.released_rows, rows)
        });
        Some(format!("{{\"name\":{},\"keys\":[{}],\"draft\":{},\"stats\":{}}}", string(name), list(keys), draft, stats))
    }).collect::<Vec<_>>().join(",");
    let searches = view.names.iter().filter_map(|(name, &id)| {
        let query = ui.search_query(id)?;
        let history = ui.search_history(id).unwrap_or_default().iter().map(|value| string(value)).collect::<Vec<_>>().join(",");
        let suggestions = ui.search_suggestions(id).unwrap_or_default().iter().map(|value| format!("{{\"key\":{},\"text\":{}}}", string(&value.key), string(&value.text))).collect::<Vec<_>>().join(",");
        Some(format!("{{\"name\":{},\"query\":{},\"status\":{},\"open\":{},\"history\":[{history}],\"suggestions\":[{suggestions}]}}", string(name), string(query), string(&format!("{:?}", ui.search_status(id).unwrap())), ui.search_popup_is_open(id)))
    }).collect::<Vec<_>>().join(",");
    let trees = view.names.iter().filter_map(|(name, &id)| {
        let frame = ui.tree_frame(id)?;
        let model = ui.tree_model(id)?;
        let keys = ui.tree_selected_keys(id).unwrap_or_default().iter().map(|key| string(&key.0.to_string())).collect::<Vec<_>>().join(",");
        let rows = frame.semantics.rows.iter().map(|row| {
            let cells = row.cells.iter().map(|cell| format!("{{\"column\":{},\"text\":{},\"bounds\":{}}}",cell.column.0,string(&cell.label),rectangle(cell.bounds))).collect::<Vec<_>>().join(",");
            format!("{{\"key\":{},\"index\":{},\"depth\":{},\"parent\":{},\"bounds\":{},\"selected\":{},\"expanded\":{},\"busy\":{},\"cells\":[{cells}]}}",string(&row.key.0.to_string()),row.index,row.depth,row.parent.map_or_else(||"null".into(),|key|string(&key.0.to_string())),rectangle(row.bounds),row.selected,row.expanded.map_or_else(||"null".into(),|value|value.to_string()),row.busy)
        }).collect::<Vec<_>>().join(",");
        let hits = frame.hits.iter().map(|hit|format!("{{\"kind\":{},\"bounds\":{}}}",string(&format!("{:?}",hit.kind)),rectangle(hit.bounds))).collect::<Vec<_>>().join(",");
        let columns = model.columns().iter().map(|column|format!("{{\"id\":{},\"width\":{},\"pinned\":{}}}",column.id.0,column.width,column.pinned)).collect::<Vec<_>>().join(",");
        let scroll=model.scroll_offset();
        Some(format!("{{\"name\":{},\"keys\":[{keys}],\"logical\":{},\"materialized\":{},\"scroll\":[{},{}],\"rows\":[{rows}],\"hits\":[{hits}],\"columns\":[{columns}]}}",string(name),frame.stats.logical_rows,frame.stats.materialized_rows,scroll.0,scroll.1))
    }).collect::<Vec<_>>().join(",");
    let grids=view.names.iter().filter_map(|(name,&id)|{
        let frame=ui.grid_frame(id)?;
        let rows=frame.semantics.rows.iter().map(|row|{
            let cells=row.cells.iter().map(|cell|format!("{{\"column\":{},\"text\":{},\"bounds\":{},\"editable\":{}}}",cell.column.0,string(&cell.label),rectangle(cell.bounds),cell.editable)).collect::<Vec<_>>().join(",");
            format!("{{\"key\":{},\"kind\":{},\"index\":{},\"depth\":{},\"bounds\":{},\"expanded\":{},\"cells\":[{cells}]}}",string(&row.key.0.to_string()),string(&format!("{:?}",ui.grid_row_kind(id,row.key))),row.index,row.depth,rectangle(row.bounds),row.expanded.map_or_else(||"null".into(),|v|v.to_string()))
        }).collect::<Vec<_>>().join(",");
        let hits=frame.hits.iter().map(|hit|format!("{{\"kind\":{},\"bounds\":{}}}",string(&format!("{:?}",hit.kind)),rectangle(hit.bounds))).collect::<Vec<_>>().join(",");
        let editor=ui.grid_editor(id).map_or_else(||"null".into(),|editor|format!("{{\"id\":{},\"bounds\":{},\"text\":{},\"accepted\":{},\"check\":{},\"error\":{}}}",string(&format!("{editor:?}")),ui.runtime.bounds(editor).map_or_else(||"null".into(),rectangle),ui.document(editor).map_or_else(||"null".into(),|d|string(d.text())),ui.accepted_value(editor).map_or_else(||"null".into(),string),string(&format!("{:?}",ui.check_state(editor))),ui.validation_error(editor).map_or_else(||"null".into(),string)));
        Some(format!("{{\"name\":{},\"key\":{},\"logical\":{},\"materialized\":{},\"mode\":{},\"error\":{},\"editor\":{editor},\"rows\":[{rows}],\"hits\":[{hits}]}}",string(name),ui.grid_selected_key(id).map_or_else(||"null".into(),|key|string(&key.to_string())),frame.stats.logical_rows,frame.stats.materialized_rows,string(&format!("{:?}",ui.grid_presentation(id).map(|p|p.view))),ui.grid_error(id).map_or_else(||"null".into(),string)))
    }).collect::<Vec<_>>().join(",");
    let badges = badges::snapshot(ui, view);
    let breadcrumbs = breadcrumbs::snapshot(ui, view);
    let navigations = crate::navigation_diagnostics::snapshot(ui, view);
    let wizards = wizards::snapshot(ui, view);
    let galleries = crate::gallery_diagnostics::snapshot(ui, view);
    format!(
        "{{\"named\":[{values}],\"nodes\":[{nodes}],\"selections\":[{selections}],\"searches\":[{searches}],\"trees\":[{trees}],\"grids\":[{grids}],\"badges\":[{badges}],\"breadcrumbs\":[{breadcrumbs}],\"navigations\":[{navigations}],\"wizards\":[{wizards}],\"galleries\":[{galleries}]}}"
    )
}
