use super::*;
use rust_desktop_ui_core::Size;
use serde_json::{Value, json};

type JsonChange = Box<dyn Fn(&mut Value)>;

fn id(value: &str) -> DockPaneId {
    DockPaneId::new(value).unwrap()
}

fn catalog(names: &[&str]) -> Vec<DockPane> {
    names
        .iter()
        .map(|name| DockPane {
            id: id(name),
            title: format!("Catalog-only title: {name}"),
            closable: true,
            min_size: Size::new(120.0, 80.0),
        })
        .collect()
}

fn tabs(group: u64, names: &[&str]) -> DockNode {
    DockNode::Tabs {
        id: DockGroupId(group),
        panes: names.iter().map(|name| id(name)).collect(),
        active: id(names[0]),
    }
}

fn sample() -> DockLayout {
    let mut layout = DockLayout::new(
        catalog(&["Документ 🦀", "Свойства", "Output", "Search", "Заметки/α"]),
        DockNode::Split {
            id: DockSplitId(1),
            axis: Axis::Horizontal,
            ratio: f32::from_bits(0x3f123456),
            first: Box::new(tabs(1, &["Документ 🦀", "Заметки/α"])),
            second: Box::new(DockNode::Split {
                id: DockSplitId(2),
                axis: Axis::Vertical,
                ratio: 0.15,
                first: Box::new(tabs(2, &["Свойства"])),
                second: Box::new(tabs(3, &["Output", "Search"])),
            }),
        },
    )
    .unwrap();
    layout.activate(&id("Заметки/α")).unwrap();
    layout
        .float_pane(&id("Свойства"), Rect::new(-1439.5, -72.25, 640.5, 380.125))
        .unwrap();
    layout.close(&id("Search")).unwrap();
    layout
}

fn reject_without_mutation(layout: &mut DockLayout, source: &str) -> DockError {
    let before = layout.snapshot();
    let error = layout.restore_json(source).unwrap_err();
    assert_eq!(layout.snapshot(), before, "{source}");
    error
}

#[test]
fn placement_json_round_trips_unicode_tabs_splits_windows_closed_and_float_bits() {
    let original = sample();
    let source = original.snapshot_json().unwrap();
    assert!(source.ends_with('\n'));
    assert!(source.len() <= MAX_DOCK_SNAPSHOT_BYTES);
    for absent in [
        "Catalog-only title",
        "WidgetId",
        "min_size",
        "closable",
        "\"text\"",
        "\"value\"",
    ] {
        assert!(!source.contains(absent), "{absent}");
    }
    let mut restored = DockLayout::new(original.panes().to_vec(), None).unwrap();
    let catalog_before = restored.panes().to_vec();
    assert!(restored.restore_json(&source).unwrap());
    assert_eq!(restored.snapshot(), original.snapshot());
    assert_eq!(restored.panes(), catalog_before);
    assert_eq!(restored.snapshot_json().unwrap(), source);
    assert!(!restored.restore_json(&source).unwrap());
    assert_eq!(
        restored.window_panes(DockWindowId::MAIN).unwrap(),
        [id("Документ 🦀"), id("Заметки/α"), id("Output")]
    );
    let DockNode::Split { ratio, .. } = restored.root(DockWindowId::MAIN).unwrap() else {
        panic!()
    };
    assert_eq!(ratio.to_bits(), 0x3f123456);
}

#[test]
fn root_null_is_explicit_and_empty_or_all_closed_layouts_round_trip() {
    for catalog in [Vec::new(), catalog(&["One", "Two"])] {
        let mut layout = DockLayout::new(catalog, None).unwrap();
        let source = layout.snapshot_json().unwrap();
        assert!(source.contains("\"root\": null"));
        assert!(!layout.restore_json(&source).unwrap());
        let mut value: Value = serde_json::from_str(&source).unwrap();
        value.as_object_mut().unwrap().remove("root");
        assert!(
            reject_without_mutation(&mut layout, &value.to_string())
                .0
                .contains("missing field")
        );
    }
}

#[test]
fn strict_schema_rejects_unknown_missing_duplicate_and_wrongly_typed_fields() {
    let mut layout = sample();
    let original = layout.snapshot_json().unwrap();
    for field in [
        "format",
        "version",
        "root",
        "floating",
        "closed",
        "auto_hide",
    ] {
        let mut value: Value = serde_json::from_str(&original).unwrap();
        value.as_object_mut().unwrap().remove(field);
        reject_without_mutation(&mut layout, &value.to_string());
    }
    let changes: Vec<JsonChange> = vec![
        Box::new(|v| v["unknown"] = json!(true)),
        Box::new(|v| v["root"]["widget_id"] = json!(1)),
        Box::new(|v| v["root"]["first"]["title"] = json!("not serialized")),
        Box::new(|v| v["floating"][0]["owner"] = json!(0)),
        Box::new(|v| v["floating"][0]["bounds"]["monitor"] = json!(1)),
        Box::new(|v| v["format"] = json!("other.product")),
        Box::new(|v| v["version"] = json!(DOCK_SNAPSHOT_VERSION + 1)),
        Box::new(|v| v["version"] = json!("1")),
        Box::new(|v| v["root"]["kind"] = json!("other")),
        Box::new(|v| v["root"]["axis"] = json!("sideways")),
        Box::new(|v| v["root"]["id"] = json!(-1)),
        Box::new(|v| v["root"]["id"] = json!(1.5)),
        Box::new(|v| v["root"]["first"]["active"] = Value::Null),
        Box::new(|v| v["closed"] = Value::Null),
    ];
    for change in changes {
        let mut value: Value = serde_json::from_str(&original).unwrap();
        change(&mut value);
        reject_without_mutation(&mut layout, &value.to_string());
    }
    for (field, literal) in [
        ("version", "1"),
        ("format", "\"rust-ui-engine.docking\""),
        ("root", "null"),
    ] {
        let source = original.replacen('{', &format!("{{\"{field}\":{literal},"), 1);
        assert!(
            reject_without_mutation(&mut layout, &source)
                .0
                .contains("duplicate field")
        );
    }
    for (needle, duplicate) in [
        (
            "\"kind\": \"split\"",
            "\"kind\": \"split\", \"kind\": \"split\"",
        ),
        (
            "\"axis\": \"horizontal\"",
            "\"axis\": \"horizontal\", \"axis\": \"horizontal\"",
        ),
        ("\"ratio\":", "\"ratio\": 0.5, \"ratio\":"),
        ("\"width\":", "\"width\": 10.0, \"width\":"),
    ] {
        let source = original.replacen(needle, duplicate, 1);
        assert_ne!(source, original);
        assert!(
            reject_without_mutation(&mut layout, &source)
                .0
                .contains("duplicate field")
        );
    }
}

#[test]
fn invalid_catalog_placement_geometry_and_ratios_are_atomic() {
    let mut layout = sample();
    let original = layout.snapshot_json().unwrap();
    let changes: Vec<JsonChange> = vec![
        Box::new(|v| v["root"]["first"]["panes"][0] = json!("Unknown")),
        Box::new(|v| v["root"]["first"]["panes"][1] = json!("Документ 🦀")),
        Box::new(|v| v["root"]["first"]["active"] = json!("Output")),
        Box::new(|v| v["root"]["first"]["id"] = v["root"]["second"]["id"].clone()),
        Box::new(|v| v["root"]["first"]["panes"][0] = json!("line\nbreak")),
        Box::new(|v| v["root"]["first"]["panes"][0] = json!("a".repeat(129))),
        Box::new(|v| v["floating"][0]["id"] = json!(0)),
        Box::new(|v| v["floating"][0]["bounds"]["width"] = json!(0)),
        Box::new(|v| v["floating"][0]["bounds"]["height"] = json!(-2)),
        Box::new(|v| {
            v["floating"][0]["bounds"]["x"] = json!(3.4e38);
            v["floating"][0]["bounds"]["width"] = json!(3.4e38);
        }),
        Box::new(|v| v["closed"] = json!(["Search", "Search"])),
        Box::new(|v| v["closed"] = json!([])),
        Box::new(|v| v["root"]["ratio"] = json!(0)),
        Box::new(|v| v["root"]["ratio"] = json!(1)),
        Box::new(|v| v["root"]["ratio"] = json!(-0.5)),
        Box::new(|v| v["root"]["ratio"] = json!(1e39)),
    ];
    for change in changes {
        let mut value: Value = serde_json::from_str(&original).unwrap();
        change(&mut value);
        reject_without_mutation(&mut layout, &value.to_string());
    }
    for ratio in [0.001, 0.999] {
        let mut value: Value = serde_json::from_str(&original).unwrap();
        value["root"]["ratio"] = json!(ratio);
        assert!(layout.restore_json(&value.to_string()).unwrap());
    }
}

#[test]
fn malformed_json_trailing_content_and_byte_budget_are_checked_before_restore() {
    let mut layout = sample();
    for source in [
        "",
        "{",
        "[]",
        "null",
        "true",
        "{\"version\":NaN}",
        "{\"version\":Infinity}",
        "{\"version\":1e500}",
        "{\"format\":\"\\uD800\"}",
    ] {
        reject_without_mutation(&mut layout, source);
    }
    let original = layout.snapshot_json().unwrap();
    reject_without_mutation(&mut layout, &format!("{original}{{}}"));
    reject_without_mutation(&mut layout, &format!("\u{feff}{original}"));
    let padded = format!(
        "{original}{}",
        " ".repeat(MAX_DOCK_SNAPSHOT_BYTES - original.len())
    );
    assert_eq!(padded.len(), MAX_DOCK_SNAPSHOT_BYTES);
    assert!(!layout.restore_json(&padded).unwrap());
    assert!(
        reject_without_mutation(&mut layout, &(padded + " "))
            .0
            .contains("one-MiB")
    );
}

fn chain_value(panes: usize) -> Value {
    let mut node = json!({"kind":"tabs", "id":0, "panes":["P0"], "active":"P0"});
    for index in 1..panes {
        node = json!({"kind":"split", "id":index, "axis":"horizontal", "ratio":0.5,
            "first":node, "second":{"kind":"tabs", "id":index, "panes":[format!("P{index}")], "active":format!("P{index}")}});
    }
    node
}

#[test]
fn tree_depth_pane_and_window_boundaries_match_the_model() {
    for count in [MAX_DOCK_DEPTH, MAX_DOCK_DEPTH + 1] {
        let names = (0..count).map(|i| format!("P{i}")).collect::<Vec<_>>();
        let refs = names.iter().map(String::as_str).collect::<Vec<_>>();
        let mut layout = DockLayout::new(catalog(&refs), None).unwrap();
        let value = json!({"format":FORMAT,"version":DOCK_SNAPSHOT_VERSION,"root":chain_value(count),"floating":[],"closed":[],"auto_hide":[]});
        if count == MAX_DOCK_DEPTH {
            assert!(layout.restore_json(&value.to_string()).unwrap());
            assert!(
                !layout
                    .restore_json(&layout.snapshot_json().unwrap())
                    .unwrap()
            );
        } else {
            reject_without_mutation(&mut layout, &value.to_string());
        }
    }
    let names = (0..MAX_DOCK_PANES)
        .map(|i| format!("P{i}"))
        .collect::<Vec<_>>();
    let refs = names.iter().map(String::as_str).collect::<Vec<_>>();
    let mut layout = DockLayout::new(catalog(&refs), tabs(1, &refs)).unwrap();
    let source = layout.snapshot_json().unwrap();
    assert!(!layout.restore_json(&source).unwrap());
    let mut value: Value = serde_json::from_str(&source).unwrap();
    value["root"]["panes"]
        .as_array_mut()
        .unwrap()
        .push(json!("P64"));
    assert!(
        reject_without_mutation(&mut layout, &value.to_string())
            .0
            .contains("tab group size")
    );
    for name in refs.iter().take(MAX_DOCK_FLOATING_WINDOWS) {
        layout
            .float_pane(&id(name), Rect::new(-200.0, 10.0, 400.0, 300.0))
            .unwrap();
    }
    let source = layout.snapshot_json().unwrap();
    assert!(!layout.restore_json(&source).unwrap());
    let mut value: Value = serde_json::from_str(&source).unwrap();
    let extra = value["floating"][0].clone();
    value["floating"].as_array_mut().unwrap().push(extra);
    assert!(
        reject_without_mutation(&mut layout, &value.to_string())
            .0
            .contains("container limits")
    );
}

#[test]
fn parser_recursion_guard_and_failed_restores_preserve_id_allocation() {
    let mut layout = sample();
    let before = layout.clone();
    // Valid JSON syntax with a pathological nested tree: parser protection is
    // intentionally left enabled even though the domain limit is much smaller.
    let mut node = r#"{"kind":"tabs","id":1,"panes":["x"],"active":"x"}"#.to_owned();
    for index in 0..150 {
        node = format!(
            r#"{{"kind":"split","id":{index},"axis":"horizontal","ratio":0.5,"first":{node},"second":{{"kind":"tabs","id":1,"panes":["x"],"active":"x"}}}}"#
        );
    }
    let source =
        format!(r#"{{"format":"{FORMAT}","version":1,"root":{node},"floating":[],"closed":[]}}"#);
    assert!(source.len() < MAX_DOCK_SNAPSHOT_BYTES);
    reject_without_mutation(&mut layout, &source);
    let mut control = before;
    let bounds = Rect::new(40.0, -80.0, 360.0, 240.0);
    assert_eq!(
        layout.float_pane(&id("Output"), bounds).unwrap(),
        control.float_pane(&id("Output"), bounds).unwrap()
    );
    assert_eq!(layout.snapshot(), control.snapshot());
}

#[test]
fn unsigned_ids_are_preserved_without_floating_point_conversion() {
    let mut layout = sample();
    let mut value: Value = serde_json::from_str(&layout.snapshot_json().unwrap()).unwrap();
    value["root"]["id"] = json!(u64::MAX);
    value["floating"][0]["id"] = json!(u64::MAX);
    layout.restore_json(&value.to_string()).unwrap();
    let encoded = layout.snapshot_json().unwrap();
    assert!(encoded.contains("18446744073709551615"));
    let mut restored = DockLayout::new(layout.panes().to_vec(), None).unwrap();
    restored.restore_json(&encoded).unwrap();
    assert_eq!(restored.snapshot(), layout.snapshot());
}
