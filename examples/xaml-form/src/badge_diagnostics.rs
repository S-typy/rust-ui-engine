//! Opt-in example diagnostics for attached indicators.
use crate::{
    popup_diagnostics::{rectangle, string},
    view::MountedView,
};
use rust_desktop_ui_controls::{BadgeContent, Controls};

pub(super) fn snapshot(ui: &Controls, view: &MountedView) -> String {
    let mut names: Vec<_> = view.names.iter().collect();
    names.sort_by_key(|(name, _)| *name);
    names.into_iter().filter_map(|(name,&id)|{
        let settings=ui.badge(id)?;
        let bounds=ui.badge_bounds(id).map_or_else(||"null".into(),rectangle);
        let ancestor_clip=ui.runtime.clip(id).map_or_else(||"null".into(),rectangle);
        let (kind,content)=match &settings.content {
            BadgeContent::Count(count)=>("Count",count.to_string()),
            BadgeContent::Text(text)=>("Text",text.clone()),
            BadgeContent::Icon(icon)=>("Icon",format!("{icon:?}")),
        };
        Some(format!("{{\"name\":{},\"owner\":{},\"bounds\":{},\"ancestor_clip\":{},\"kind\":{},\"content\":{},\"anchor\":{},\"offset\":[{},{}],\"clip\":{},\"visible\":{},\"enabled\":{}}}",string(name),string(&format!("{id:?}")),bounds,ancestor_clip,string(kind),string(&content),string(&format!("{:?}",settings.anchor)),settings.offset.x,settings.offset.y,string(&format!("{:?}",settings.clip)),settings.visible,ui.is_enabled(id)))
    }).collect::<Vec<_>>().join(",")
}
