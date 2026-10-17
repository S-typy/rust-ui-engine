use super::*;
use crate::{Controls, DockGroupId, DockLayout, DockNode, DockPane, DockPaneId, Theme};
use rust_desktop_ui_core::{LayoutStyle, Length, Mutation, Size};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Width {
    width: u32,
}
struct WidthCodec;
impl WorkspaceCodec<Width> for WidthCodec {
    fn kind(&self) -> &str {
        "panel-width"
    }
    fn version(&self) -> u32 {
        2
    }
    fn validate(&self, v: &Width) -> Result<(), WorkspaceError> {
        if (80..=800).contains(&v.width) {
            Ok(())
        } else {
            Err(error("Width is outside 80..800"))
        }
    }
    fn encode(&self, v: &Width) -> Result<String, WorkspaceError> {
        serde_json::to_string(v).map_err(|_| error("Cannot encode width"))
    }
    fn decode(&self, version: u32, payload: &str, _: &Width) -> Result<Width, WorkspaceError> {
        match version {
            1 => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Old {
                    pixels: u32,
                }
                let value: Old =
                    serde_json::from_str(payload).map_err(|_| error("Invalid old width"))?;
                Ok(Width {
                    width: value.pixels,
                })
            }
            2 => serde_json::from_str(payload).map_err(|_| error("Invalid width")),
            _ => Err(error("Unsupported width version")),
        }
    }
}
fn fixture() -> WorkspaceStore {
    let mut s = WorkspaceStore::new("main").unwrap();
    s.register("left", Width { width: 160 }, WidthCodec)
        .unwrap();
    s.register("right", Width { width: 320 }, WidthCodec)
        .unwrap();
    s
}
fn json(s: &WorkspaceStore) -> serde_json::Value {
    serde_json::from_str(&s.save_json().unwrap()).unwrap()
}
fn source(v: &serde_json::Value) -> String {
    serde_json::to_string(v).unwrap()
}

#[test]
fn workspace_restore_is_explicit_typed_and_staged_drop_cancels() {
    let mut s = fixture();
    let saved = s.save_json().unwrap();
    let generation = s.generation();
    s.replace("left", Width { width: 400 }).unwrap();
    assert_eq!(s.get::<Width>("left").unwrap().width, 400);
    {
        let candidate = s.prepare_restore(&saved).unwrap();
        assert_eq!(candidate.get::<Width>("left").unwrap().width, 160);
        assert!(candidate.get::<String>("left").is_none());
    }
    assert_eq!(s.get::<Width>("left").unwrap().width, 400);
    assert_eq!(s.generation(), generation + 1);
    assert_eq!(s.restore_json(&saved).unwrap(), generation + 2);
    assert_eq!(s.save_json().unwrap(), saved);
}
#[test]
fn workspace_missing_unknown_duplicate_type_and_version_preserve_all_components() {
    let mut s = fixture();
    let original = s.save_json().unwrap();
    let generation = s.generation();
    let base = json(&s);
    for change in 0..8 {
        let mut value = base.clone();
        match change {
            0 => {
                value["components"].as_array_mut().unwrap().pop();
            }
            1 => value["components"][0]["id"] = "unknown".into(),
            2 => {
                let first = value["components"][0].clone();
                value["components"][1] = first;
            }
            3 => value["components"][0]["kind"] = "another-type".into(),
            4 => value["components"][0]["version"] = 3.into(),
            5 => value["version"] = 2.into(),
            6 => value["workspace"] = "other".into(),
            _ => value["components"][0]["version"] = 0.into(),
        }
        assert!(s.restore_json(&source(&value)).is_err(), "case {change}");
        assert_eq!(s.save_json().unwrap(), original);
        assert_eq!(s.generation(), generation);
    }
}
#[test]
fn workspace_duplicate_envelope_and_component_fields_are_not_silently_overwritten() {
    let mut s = fixture();
    let saved = s.save_json().unwrap();
    for text in [
        saved.replacen("\"version\": 1", "\"version\": 1, \"version\": 1", 1),
        saved.replacen("\"id\": \"left\"", "\"id\": \"left\", \"id\": \"right\"", 1),
        saved.replacen("\"format\":", "\"unexpected\": 1, \"format\":", 1),
    ] {
        assert!(s.restore_json(&text).is_err());
        assert_eq!(s.save_json().unwrap(), saved);
    }
    let mut v = json(&s);
    v["components"][0]["payload"] = "{\"width\":160,\"width\":400}".into();
    assert!(s.restore_json(&source(&v)).is_err());
    assert_eq!(s.save_json().unwrap(), saved);
}
#[test]
fn workspace_migration_is_owned_by_codec_and_any_failure_cancels_every_candidate() {
    let mut s = fixture();
    let mut old = json(&s);
    for r in old["components"].as_array_mut().unwrap() {
        r["version"] = 1.into();
        r["payload"] = "{\"pixels\":240}".into();
    }
    s.restore_json(&source(&old)).unwrap();
    assert_eq!(s.get::<Width>("left").unwrap().width, 240);
    assert_eq!(json(&s)["components"][0]["version"], 2);
    let saved = s.save_json().unwrap();
    let generation = s.generation();
    old["components"][1]["payload"] = "{\"pixels\":9000}".into();
    assert!(s.restore_json(&source(&old)).is_err());
    assert_eq!(s.save_json().unwrap(), saved);
    assert_eq!(s.generation(), generation);
    old["components"][1]["payload"] = "{\"unknown\":240}".into();
    assert!(s.restore_json(&source(&old)).is_err());
    assert_eq!(s.save_json().unwrap(), saved);
}
#[test]
fn workspace_wrong_rust_type_invalid_ids_and_size_limits_are_atomic() {
    let mut s = fixture();
    let saved = s.save_json().unwrap();
    let generation = s.generation();
    assert!(s.replace("left", String::from("wrong type")).is_err());
    assert!(s.replace("left", Width { width: 0 }).is_err());
    assert!(s.register("left", Width { width: 80 }, WidthCodec).is_err());
    assert!(
        s.register("bad id", Width { width: 80 }, WidthCodec)
            .is_err()
    );
    assert!(
        s.restore_json(&" ".repeat(MAX_WORKSPACE_BYTES + 1))
            .is_err()
    );
    assert_eq!(s.generation(), generation);
    assert_eq!(s.save_json().unwrap(), saved);
    for i in 2..MAX_WORKSPACE_COMPONENTS {
        s.register(format!("p{i}"), Width { width: 80 }, WidthCodec)
            .unwrap();
    }
    assert!(
        s.register("overflow", Width { width: 80 }, WidthCodec)
            .is_err()
    );
    assert_eq!(s.component_ids().len(), 64);
}
#[test]
fn workspace_generation_overflow_is_checked_before_preparation_or_live_apply() {
    let mut s = fixture();
    let saved = s.save_json().unwrap();
    s.generation = u64::MAX;
    assert!(s.prepare_restore(&saved).is_err());
    assert!(s.replace("left", Width { width: 240 }).is_err());
    assert!(
        s.register("third", Width { width: 80 }, WidthCodec)
            .is_err()
    );
    assert_eq!(s.save_json().unwrap(), saved);
}

#[test]
fn workspace_escaped_output_and_migration_expansion_obey_the_final_envelope_limit() {
    struct BlobCodec;
    impl WorkspaceCodec<String> for BlobCodec {
        fn kind(&self) -> &str {
            "blob"
        }
        fn version(&self) -> u32 {
            2
        }
        fn validate(&self, _: &String) -> Result<(), WorkspaceError> {
            Ok(())
        }
        fn encode(&self, v: &String) -> Result<String, WorkspaceError> {
            Ok(v.clone())
        }
        fn decode(
            &self,
            version: u32,
            payload: &str,
            _: &String,
        ) -> Result<String, WorkspaceError> {
            Ok(if version == 1 {
                "x".repeat(600_000)
            } else {
                payload.into()
            })
        }
    }
    let mut s = WorkspaceStore::new("bounded").unwrap();
    assert!(
        s.register("quote", "\"".repeat(MAX_WORKSPACE_BYTES / 2), BlobCodec)
            .is_err()
    );
    assert_eq!(s.generation(), 0);
    assert_eq!(s.component_ids().len(), 0);
    s.register("a", String::from("small"), BlobCodec).unwrap();
    s.register("b", String::from("small"), BlobCodec).unwrap();
    let saved = s.save_json().unwrap();
    let generation = s.generation();
    let mut old = json(&s);
    for record in old["components"].as_array_mut().unwrap() {
        record["version"] = 1.into();
    }
    assert!(s.restore_json(&source(&old)).is_err());
    assert_eq!(s.save_json().unwrap(), saved);
    assert_eq!(s.generation(), generation);
}

#[test]
fn workspace_codec_errors_are_bounded_even_for_directly_constructed_errors() {
    struct Failing;
    impl WorkspaceCodec<Width> for Failing {
        fn kind(&self) -> &str {
            "failing"
        }
        fn version(&self) -> u32 {
            1
        }
        fn validate(&self, _: &Width) -> Result<(), WorkspaceError> {
            Err(WorkspaceError {
                component: Some("not owned".into()),
                message: "é\n".repeat(1000),
            })
        }
        fn encode(&self, _: &Width) -> Result<String, WorkspaceError> {
            unreachable!()
        }
        fn decode(&self, _: u32, _: &str, _: &Width) -> Result<Width, WorkspaceError> {
            unreachable!()
        }
    }
    let mut s = WorkspaceStore::new("bounded").unwrap();
    let error = s
        .register("pane", Width { width: 80 }, Failing)
        .unwrap_err();
    assert_eq!(error.component.as_deref(), Some("pane"));
    assert!(error.message.len() <= 512);
    assert!(!error.message.contains('\n'));
    assert_eq!(s.generation(), 0);
}
#[test]
fn workspace_staged_ui_batch_retains_editor_and_rolls_back_failed_live_projection() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let panel = ui
        .panel(
            root,
            LayoutStyle {
                width: Length::Px(160.0),
                ..Default::default()
            },
        )
        .unwrap();
    let editor = ui
        .text_box(panel, "Document", "accepted", LayoutStyle::default())
        .unwrap();
    ui.update(Size::new(800.0, 600.0)).unwrap();
    ui.set_text(editor, "unsaved draft").unwrap();
    let mut s = fixture();
    let saved = s.save_json().unwrap();
    let mut changed = json(&s);
    changed["components"][0]["payload"] = "{\"width\":400}".into();
    let doomed = ui
        .button(root, "temporary", None, LayoutStyle::default())
        .unwrap();
    ui.remove(doomed).unwrap();
    let before = ui.runtime.tree().node(panel).unwrap().props.style.clone();
    let generation = s.generation();
    {
        let pending = s.prepare_restore(&source(&changed)).unwrap();
        let mut style = before.clone();
        style.width = Length::Px(pending.get::<Width>("left").unwrap().width as f32);
        assert!(
            ui.runtime
                .apply_batch(&[
                    Mutation::SetStyle { node: panel, style },
                    Mutation::SetStyle {
                        node: doomed,
                        style: LayoutStyle::default()
                    }
                ])
                .is_err()
        );
    }
    assert_eq!(ui.runtime.tree().node(panel).unwrap().props.style, before);
    assert_eq!(s.generation(), generation);
    assert_eq!(s.save_json().unwrap(), saved);
    assert_eq!(ui.document(editor).unwrap().text(), "unsaved draft");
    let pending = s.prepare_restore(&source(&changed)).unwrap();
    let mut style = before;
    style.width = Length::Px(pending.get::<Width>("left").unwrap().width as f32);
    ui.runtime
        .apply_batch(&[Mutation::SetStyle { node: panel, style }])
        .unwrap();
    pending.commit();
    ui.update(Size::new(800.0, 600.0)).unwrap();
    assert_eq!(ui.runtime.bounds(panel).unwrap().width, 400.0);
    assert_eq!(ui.accepted_value(editor), Some("accepted"));
    assert_eq!(ui.document(editor).unwrap().text(), "unsaved draft");
    assert_eq!(ui.runtime.tree().parent(editor), Some(panel));
}
#[test]
fn workspace_dock_codec_preserves_catalog_and_rejects_unknown_panes_atomically() {
    let id = DockPaneId::new("document").unwrap();
    let pane = DockPane {
        id: id.clone(),
        title: "Document secret caption".into(),
        closable: true,
        min_size: Size::new(80.0, 60.0),
    };
    let layout = DockLayout::new(
        vec![pane],
        DockNode::Tabs {
            id: DockGroupId(1),
            panes: vec![id.clone()],
            active: id.clone(),
        },
    )
    .unwrap();
    let mut s = WorkspaceStore::new("dock").unwrap();
    s.register("docking", layout.clone(), DockWorkspaceCodec)
        .unwrap();
    let saved = s.save_json().unwrap();
    assert!(!saved.contains("secret caption"));
    let mut closed = layout;
    closed.close(&id).unwrap();
    s.replace("docking", closed).unwrap();
    s.restore_json(&saved).unwrap();
    assert!(
        matches!(s.get::<DockLayout>("docking").unwrap().snapshot().root,Some(DockNode::Tabs {panes,..}) if panes==vec![id.clone()])
    );
    let altered = saved.replace("document", "unknown");
    assert!(s.restore_json(&altered).is_err());
    assert_eq!(s.save_json().unwrap(), saved);
    assert_eq!(
        s.get::<DockLayout>("docking")
            .unwrap()
            .pane(&id)
            .unwrap()
            .title,
        "Document secret caption"
    );
}
