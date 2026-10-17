//! Explicit workspace save/restore over existing retained controls, without I/O.
use rust_desktop_ui_controls::{
    Controls, Theme,
    workspace::{WorkspaceCodec, WorkspaceError, WorkspaceStore},
};
use rust_desktop_ui_core::{Axis, LayoutKind, LayoutStyle, Length, Mutation, Size, WidgetId};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PanelLayout {
    width: u32,
    gap: u32,
}
struct LayoutCodec;
impl WorkspaceCodec<PanelLayout> for LayoutCodec {
    fn kind(&self) -> &str {
        "example-panel-layout"
    }
    fn version(&self) -> u32 {
        2
    }
    fn validate(&self, value: &PanelLayout) -> Result<(), WorkspaceError> {
        if (80..=480).contains(&value.width) && value.gap <= 40 {
            Ok(())
        } else {
            Err(WorkspaceError::new("Panel width or gap is out of range"))
        }
    }
    fn encode(&self, value: &PanelLayout) -> Result<String, WorkspaceError> {
        serde_json::to_string(value).map_err(json_error)
    }
    fn decode(
        &self,
        version: u32,
        payload: &str,
        _: &PanelLayout,
    ) -> Result<PanelLayout, WorkspaceError> {
        if version == 1 {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Old {
                pixels: u32,
            }
            let old: Old = serde_json::from_str(payload).map_err(json_error)?;
            Ok(PanelLayout {
                width: old.pixels,
                gap: 12,
            })
        } else if version == 2 {
            serde_json::from_str(payload).map_err(json_error)
        } else {
            Err(WorkspaceError::new("Unsupported panel version"))
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sidebar {
    visible: bool,
}
struct SidebarCodec;
impl WorkspaceCodec<Sidebar> for SidebarCodec {
    fn kind(&self) -> &str {
        "example-sidebar"
    }
    fn version(&self) -> u32 {
        1
    }
    fn validate(&self, _: &Sidebar) -> Result<(), WorkspaceError> {
        Ok(())
    }
    fn encode(&self, value: &Sidebar) -> Result<String, WorkspaceError> {
        serde_json::to_string(value).map_err(json_error)
    }
    fn decode(&self, version: u32, payload: &str, _: &Sidebar) -> Result<Sidebar, WorkspaceError> {
        if version != 1 {
            return Err(WorkspaceError::new("Unsupported sidebar version"));
        }
        serde_json::from_str(payload).map_err(json_error)
    }
}
fn json_error(_: serde_json::Error) -> WorkspaceError {
    WorkspaceError::new("Invalid component JSON")
}

fn restore(
    store: &mut WorkspaceStore,
    ui: &mut Controls,
    root: WidgetId,
    sidebar: WidgetId,
    source: &str,
) -> Result<(), WorkspaceError> {
    let pending = store.prepare_restore(source)?;
    let layout = pending
        .get::<PanelLayout>("layout")
        .ok_or_else(|| WorkspaceError::new("Layout registration mismatch"))?;
    let visibility = pending
        .get::<Sidebar>("sidebar")
        .ok_or_else(|| WorkspaceError::new("Sidebar registration mismatch"))?;
    if ui.runtime.tree().parent(sidebar) != Some(root) {
        return Err(WorkspaceError::new(
            "Sidebar is no longer owned by this workspace",
        ));
    }
    let mut root_style = ui
        .runtime
        .tree()
        .node(root)
        .ok_or_else(|| WorkspaceError::new("Workspace owner removed"))?
        .props
        .style
        .clone();
    root_style.gap = layout.gap as f32;
    let mut sidebar_props = ui
        .runtime
        .tree()
        .node(sidebar)
        .ok_or_else(|| WorkspaceError::new("Sidebar removed"))?
        .props
        .clone();
    sidebar_props.style.width = Length::Px(layout.width as f32);
    sidebar_props.collapsed = !visibility.visible;
    ui.runtime
        .apply_batch(&[
            Mutation::SetStyle {
                node: root,
                style: root_style,
            },
            Mutation::SetProps {
                node: sidebar,
                props: sidebar_props,
            },
        ])
        .map_err(|_| WorkspaceError::new("Workspace UI projection failed"))?;
    // Everything fallible is complete. No model/codec callback runs after this batch.
    pending.commit();
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut ui = Controls::new(Theme::FluentLight)?;
    let root = ui.root();
    ui.runtime.apply(Mutation::SetStyle {
        node: root,
        style: LayoutStyle {
            kind: LayoutKind::Stack(Axis::Horizontal),
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            gap: 12.0,
            ..Default::default()
        },
    })?;
    let sidebar = ui.panel(
        root,
        LayoutStyle {
            width: Length::Px(160.0),
            height: Length::Percent(1.0),
            ..Default::default()
        },
    )?;
    let content = ui.panel(
        root,
        LayoutStyle {
            width: Length::Px(400.0),
            height: Length::Percent(1.0),
            ..Default::default()
        },
    )?;
    let editor = ui.text_box(content, "Document", "accepted", LayoutStyle::default())?;
    ui.set_text(editor, "private unsaved draft")?;
    let mut store = WorkspaceStore::new("main-window")?;
    store.register(
        "layout",
        PanelLayout {
            width: 160,
            gap: 12,
        },
        LayoutCodec,
    )?;
    store.register("sidebar", Sidebar { visible: true }, SidebarCodec)?;
    let saved = store.save_json()?;
    assert!(!saved.contains("draft"));
    let ids = ui.runtime.tree().preorder();
    let mut candidate: serde_json::Value = serde_json::from_str(&saved)?;
    candidate["components"][0]["version"] = 1.into();
    candidate["components"][0]["payload"] = "{\"pixels\":240}".into();
    restore(
        &mut store,
        &mut ui,
        root,
        sidebar,
        &serde_json::to_string(&candidate)?,
    )?;
    ui.update(Size::new(800.0, 600.0))?;
    assert_eq!(ui.runtime.bounds(sidebar).unwrap().width, 240.0);
    assert_eq!(store.get::<PanelLayout>("layout").unwrap().gap, 12);
    let accepted = store.save_json()?;
    let before = ui.runtime.tree().node(sidebar).unwrap().props.clone();
    candidate["components"][1]["payload"] = "{\"visible\":\"invalid\"}".into();
    assert!(
        restore(
            &mut store,
            &mut ui,
            root,
            sidebar,
            &serde_json::to_string(&candidate)?
        )
        .is_err()
    );
    assert_eq!(store.save_json()?, accepted);
    assert_eq!(ui.runtime.tree().node(sidebar).unwrap().props, before);
    restore(&mut store, &mut ui, root, sidebar, &saved)?;
    ui.update(Size::new(800.0, 600.0))?;
    assert_eq!(ui.runtime.bounds(sidebar).unwrap().width, 160.0);
    assert_eq!(ui.runtime.tree().preorder(), ids);
    assert_eq!(ui.document(editor).unwrap().text(), "private unsaved draft");
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
    println!(
        "{{\"passed\":true,\"migration\":true,\"participant_failure_atomic\":true,\"retained_ids\":true,\"draft_preserved\":true,\"explicit_restore\":true}}"
    );
    Ok(())
}
