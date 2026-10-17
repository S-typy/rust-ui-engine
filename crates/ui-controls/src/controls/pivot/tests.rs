use super::*;
use rust_desktop_ui_core::Modifiers;
use rust_desktop_ui_data::{Aggregate, AggregateFunction, DataQuery, QueryExpression};
fn table(n: usize) -> DataTable {
    let mut region = FieldSchema::new("Region", FieldType::Text);
    region.nullable = true;
    DataTable::new(
        TableSchema::new(vec![
            region,
            FieldSchema::new("City", FieldType::Text),
            FieldSchema::new("Year", FieldType::Integer),
            FieldSchema::new("Amount", FieldType::Integer),
        ])
        .unwrap(),
        (0..n)
            .map(|i| {
                DataRow::new(
                    if i == 0 {
                        i64::MIN
                    } else if i == 1 {
                        i64::MAX
                    } else {
                        i as i64
                    },
                    vec![
                        if i == 0 {
                            CellValue::Null
                        } else {
                            CellValue::Text(format!("R{}", i % 200))
                        },
                        CellValue::Text(format!("C{}", i % 100)),
                        CellValue::Integer(2025 + (i % 2) as i64),
                        CellValue::Integer(i as i64),
                    ],
                )
            })
            .collect(),
    )
    .unwrap()
}
fn definition() -> PivotDefinition {
    PivotDefinition {
        row_fields: vec!["Region".into(), "City".into()],
        column_fields: vec!["Year".into()],
        measures: vec![rust_desktop_ui_data::PivotMeasure {
            title: "Sum".into(),
            aggregate: Aggregate {
                function: AggregateFunction::Sum,
                expression: Some(QueryExpression::field("Amount")),
            },
        }],
        filter: None,
    }
}
fn fixture(n: usize) -> (Controls, PivotView, DataSourceHandle) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let source = Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(table(n))).unwrap(),
    ));
    let root = ui.root();
    let view = PivotView::mount(
        &mut ui,
        root,
        source.clone(),
        definition(),
        LayoutStyle {
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            ..Default::default()
        },
    )
    .unwrap();
    ui.update(Size::new(1200.0, 650.0)).unwrap();
    (ui, view, source)
}
#[test]
fn pivot_real_geometry_retains_grid_and_full_chrome_at_narrow_width() {
    let (mut ui, mut p, _) = fixture(250);
    let grid = p.parts.grid;
    for size in [Size::new(1200.0, 650.0), Size::new(640.0, 500.0)] {
        p.update(&mut ui).unwrap();
        ui.update(size).unwrap();
        for id in [
            p.parts.expand_row,
            p.parts.collapse_row,
            p.parts.expand_column,
            p.parts.collapse_column,
            p.parts.drill_down,
        ] {
            let b = ui.runtime.bounds(id).unwrap();
            let c = ui.runtime.clip(id).unwrap();
            assert!(b.width >= 159.0 && b.height >= 35.0, "{b:?}");
            assert!(
                b.intersection(c)
                    .is_some_and(|v| v.width >= b.width - 0.1 && v.height >= b.height - 0.1),
                "{b:?} / {c:?}"
            );
        }
        assert_eq!(p.parts.grid, grid);
        assert!(
            ui.runtime.bounds(grid).unwrap().width
                >= ui.runtime.bounds(p.parts.container).unwrap().width - 0.1,
            "grid={:?},owner={:?},size={size:?}",
            ui.runtime.bounds(grid),
            ui.runtime.bounds(p.parts.container)
        );
        assert!(ui.grid_frame(grid).unwrap().stats.materialized_rows < 40);
    }
}
#[test]
fn pivot_expansion_retains_identity_and_collapse_selects_nearest_visible_parent() {
    let (mut ui, mut p, _) = fixture(250);
    let parent = p
        .snapshot
        .row_paths()
        .iter()
        .find(|v| {
            v.depth() == 1
                && p.snapshot
                    .row_paths()
                    .iter()
                    .any(|c| c.parent() == Some(v.id()))
        })
        .unwrap()
        .id()
        .clone();
    p.set_expanded(&mut ui, PivotAxis::Rows, &parent, true)
        .unwrap();
    let child = p
        .snapshot
        .row_paths()
        .iter()
        .find(|v| v.parent() == Some(&parent))
        .unwrap()
        .id()
        .clone();
    let key = p.identities[&child];
    ui.grid_focus_cell(p.parts.grid, key, None).unwrap();
    let grid = p.parts.grid;
    let editor = ui.runtime.tree().children(grid).unwrap()[0];
    p.set_expanded(&mut ui, PivotAxis::Rows, &parent, false)
        .unwrap();
    assert_eq!(p.selected_row_path(&ui), Some(&parent));
    assert_eq!(ui.runtime.tree().children(grid).unwrap()[0], editor);
    p.set_expanded(&mut ui, PivotAxis::Rows, &parent, true)
        .unwrap();
    assert_eq!(p.identities[&child], key);
}
#[test]
fn pivot_definition_failure_is_atomic_and_source_refresh_keeps_surviving_path() {
    let (mut ui, mut p, source) = fixture(250);
    let before = p.snapshot.definition().clone();
    let keys = p.identities.clone();
    let mut bad = before.clone();
    bad.column_fields = vec!["Region".into()];
    assert!(p.set_definition(&mut ui, bad).is_err());
    assert_eq!(p.snapshot.definition(), &before);
    assert_eq!(p.identities, keys);
    let path = p.rows[1].1.clone();
    ui.grid_focus_cell(p.parts.grid, p.identities[&path], None)
        .unwrap();
    source
        .borrow_mut()
        .replace_adapter(CollectionAdapter::new(table(251)), DataQuery::default())
        .unwrap();
    p.update(&mut ui).unwrap();
    assert_eq!(p.selected_row_path(&ui), Some(&path));
    assert_eq!(p.identities[&path], keys[&path]);
}
#[test]
fn pivot_field_pointer_capture_cancel_and_keyboard_reorder() {
    let (mut ui, mut p, _) = fixture(40);
    let field = p.field_widget("City").unwrap();
    let b = ui.runtime.bounds(field).unwrap();
    let pos = Point::new(b.x + 10.0, b.y + 10.0);
    let down = InputEvent::PointerDown {
        position: pos,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    };
    assert!(p.before_input(&mut ui, &down).unwrap());
    assert_eq!(ui.runtime.captured(), Some(field));
    p.cancel_pointer(&mut ui).unwrap();
    assert_eq!(ui.runtime.captured(), None);
    assert_eq!(p.snapshot.definition().row_fields, vec!["Region", "City"]);
    ui.runtime.apply(Mutation::Focus(Some(field))).unwrap();
    assert!(
        p.before_input(
            &mut ui,
            &InputEvent::KeyDown {
                key: Key::ArrowLeft,
                modifiers: Modifiers {
                    control: true,
                    ..Default::default()
                },
                repeat: false
            }
        )
        .unwrap()
    );
    assert_eq!(p.snapshot.definition().row_fields, vec!["City", "Region"]);
    assert_eq!(p.field_widget("City"), Some(field));
    assert_eq!(ui.runtime.focused(), Some(field));
}
#[test]
fn pivot_pointer_drop_moves_field_once_and_escape_preserves_definition() {
    let (mut ui, mut p, _) = fixture(40);
    let field = p.field_widget("City").unwrap();
    let b = ui.runtime.bounds(field).unwrap();
    let start = Point::new(b.x + 10.0, b.y + 10.0);
    let target = ui.runtime.bounds(p.parts.column_fields).unwrap();
    let end = Point::new(target.x + 400.0, target.y + 15.0);
    let down = InputEvent::PointerDown {
        position: start,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    };
    p.before_input(&mut ui, &down).unwrap();
    p.before_input(&mut ui, &InputEvent::PointerMoved { position: end })
        .unwrap();
    p.before_input(
        &mut ui,
        &InputEvent::KeyDown {
            key: Key::Escape,
            modifiers: Modifiers::default(),
            repeat: false,
        },
    )
    .unwrap();
    assert_eq!(p.snapshot.definition(), &definition());
    p.before_input(&mut ui, &down).unwrap();
    p.before_input(&mut ui, &InputEvent::PointerMoved { position: end })
        .unwrap();
    p.before_input(
        &mut ui,
        &InputEvent::PointerUp {
            position: end,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
    )
    .unwrap();
    assert_eq!(p.snapshot.definition().row_fields, vec!["Region"]);
    assert_eq!(p.snapshot.definition().column_fields, vec!["Year", "City"]);
    assert_eq!(ui.runtime.captured(), None);
    assert!(p.take_drill_down_events().is_empty());
}
#[test]
fn pivot_drilldown_preserves_full_source_keys_and_rejects_stale_revision() {
    let (mut ui, mut p, source) = fixture(12);
    p.request_drill_down(&mut ui).unwrap();
    let e = p.take_drill_down_events();
    assert_eq!(e.len(), 1);
    assert!(e[0].data.row_keys.contains(&i64::MIN) && e[0].data.row_keys.contains(&i64::MAX));
    source
        .borrow_mut()
        .replace_adapter(CollectionAdapter::new(table(13)), DataQuery::default())
        .unwrap();
    assert!(p.request_drill_down(&mut ui).is_err());
    assert!(p.take_drill_down_events().is_empty());
    p.update(&mut ui).unwrap();
    p.request_drill_down(&mut ui).unwrap();
    assert_eq!(p.take_drill_down_events()[0].data.row_keys.len(), 13);
}
#[test]
fn pivot_100k_input_remains_viewport_virtualized_and_has_typed_totals() {
    let (mut ui, mut p, _) = fixture(100_000);
    p.update(&mut ui).unwrap();
    ui.update(Size::new(1200.0, 650.0)).unwrap();
    let frame = ui.grid_frame(p.parts.grid).unwrap();
    assert!(frame.stats.logical_rows > 200);
    assert!(frame.stats.materialized_rows < 40);
    assert!(frame.stats.cells_read < 300);
    assert_eq!(
        p.snapshot
            .value(
                p.snapshot.row_paths()[0].id(),
                p.snapshot.column_paths()[0].id(),
                0
            )
            .unwrap(),
        &CellValue::Integer(4_999_950_000)
    );
}
#[test]
fn pivot_required_part_mutation_rejects_before_publishing_definition() {
    let (mut ui, mut p, _) = fixture(40);
    let before = p.snapshot.definition().clone();
    let field = p.field_widget("City").unwrap();
    ui.runtime
        .apply(Mutation::Reparent {
            node: field,
            parent: ui.root(),
        })
        .unwrap();
    assert!(
        p.move_field(&mut ui, "City", PivotAxis::Columns, 1)
            .is_err()
    );
    assert_eq!(p.snapshot.definition(), &before);
}

#[test]
fn pivot_source_borrow_failure_releases_owned_capture_before_error() {
    let (mut ui, mut p, source) = fixture(40);
    let field = p.field_widget("City").unwrap();
    let b = ui.runtime.bounds(field).unwrap();
    p.before_input(
        &mut ui,
        &InputEvent::PointerDown {
            position: Point::new(b.x + 10.0, b.y + 10.0),
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
    )
    .unwrap();
    let before = p.snapshot.definition().clone();
    let guard = source.borrow_mut();
    assert!(p.update(&mut ui).is_err());
    assert_eq!(ui.runtime.captured(), None);
    assert!(p.drag.is_none());
    assert_eq!(p.snapshot.definition(), &before);
    drop(guard);
}

#[test]
fn pivot_external_projection_replacement_cannot_create_false_drill_provenance() {
    let (mut ui, mut p, _) = fixture(40);
    let before = p.snapshot.definition().clone();
    let source = ui.grid_source(p.parts.grid).unwrap();
    let table = source.borrow().table().clone();
    let mut rows = table.rows().to_vec();
    rows[0].values[0] = CellValue::Text("forged".into());
    source
        .borrow_mut()
        .replace_adapter(
            CollectionAdapter::new(DataTable::new(table.schema().clone(), rows).unwrap()),
            DataQuery::default(),
        )
        .unwrap();
    assert!(p.request_drill_down(&mut ui).is_err());
    assert!(p.take_drill_down_events().is_empty());
    assert_eq!(p.snapshot.definition(), &before);
}
#[test]
fn pivot_external_cards_recipe_is_rejected_but_native_sort_query_is_allowed() {
    let (mut ui, mut p, _) = fixture(40);
    let source = ui.grid_source(p.parts.grid).unwrap();
    source
        .borrow_mut()
        .set_query(DataQuery {
            sort: vec![rust_desktop_ui_data::SortDescriptor {
                field: "axis".into(),
                direction: rust_desktop_ui_data::SortDirection::Descending,
            }],
            ..Default::default()
        })
        .unwrap();
    p.update(&mut ui).unwrap();
    ui.set_grid_presentation(
        p.parts.grid,
        GridPresentationSettings {
            view: GridViewMode::Cards,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(p.update(&mut ui).is_err());
    assert!(p.take_drill_down_events().is_empty());
}

#[test]
fn pivot_long_field_caption_is_measured_ellipsis_with_full_accessible_name() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let name = "VeryLongUnicodeFieldNameéRepeatedéRepeatedé";
    let schema = TableSchema::new(vec![FieldSchema::new(name, FieldType::Text)]).unwrap();
    let source = Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(
            DataTable::new(
                schema,
                vec![DataRow::new(1, vec![CellValue::Text("A".into())])],
            )
            .unwrap(),
        ))
        .unwrap(),
    ));
    let d = PivotDefinition {
        row_fields: vec![name.into()],
        column_fields: Vec::new(),
        measures: vec![rust_desktop_ui_data::PivotMeasure {
            title: "Count".into(),
            aggregate: Aggregate {
                function: AggregateFunction::CountRows,
                expression: None,
            },
        }],
        filter: None,
    };
    let root = ui.root();
    let p = PivotView::mount(&mut ui, root, source, d, LayoutStyle::default()).unwrap();
    let id = p.field_widget(name).unwrap();
    assert!(ui.controls[&id].label.ends_with('…'));
    assert!(
        ui.measure_text(&ui.controls[&id].label.clone(), 4096.0, false)
            .unwrap()
            .width
            <= 132.0
    );
    assert!(
        ui.controls[&id]
            .automation_name
            .as_ref()
            .unwrap()
            .contains(name)
    );
    assert_eq!(ui.tooltip(id), Some(name));
}

#[test]
fn pivot_multiline_field_display_is_normalized_without_changing_identity() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    assert_eq!(
        PivotView::field_caption(&mut ui, "Region\nName\tX").unwrap(),
        "Region Name X"
    );
}
