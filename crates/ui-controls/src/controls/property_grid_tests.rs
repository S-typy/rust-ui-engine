use super::*;
use rust_desktop_ui_data::{
    CellValue, CollectionAdapter, DataAdapter, DataError, DataRow, DataSource, DataTable,
    DataWrite, FieldSchema, FieldType, TableSchema, WriteCapabilities,
};
use std::{cell::RefCell, rc::Rc};

fn table() -> DataTable {
    let mut text = FieldSchema::new("name", FieldType::Text);
    text.nullable = true;
    let mut flag = FieldSchema::new("enabled", FieldType::Bool);
    flag.nullable = true;
    DataTable::new(
        TableSchema::new(vec![
            text,
            FieldSchema::new("count", FieldType::Integer),
            flag,
        ])
        .unwrap(),
        vec![
            DataRow::new(
                1,
                vec![
                    CellValue::Null,
                    CellValue::Integer(i64::MIN),
                    CellValue::Null,
                ],
            ),
            DataRow::new(
                2,
                vec![
                    CellValue::Text(String::new()),
                    CellValue::Integer(i64::MAX),
                    CellValue::Bool(false),
                ],
            ),
        ],
    )
    .unwrap()
}
fn settings() -> PropertyGridSettings {
    let mut name = PropertyDescriptor::new("name");
    name.category = "Общие".into();
    name.default = Some(CellValue::Text("Untitled".into()));
    let mut count = PropertyDescriptor::new("count");
    count.category = "Общие".into();
    count.groups = vec!["Размеры".into()];
    count.default = Some(CellValue::Integer(10));
    let mut flag = PropertyDescriptor::new("enabled");
    flag.category = "Общие".into();
    flag.default = Some(CellValue::Null);
    PropertyGridSettings {
        properties: vec![name, count, flag],
        ..Default::default()
    }
}
fn fixture() -> (Controls, PropertyGrid, DataSourceHandle) {
    let source = Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(table())).unwrap(),
    ));
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let grid = PropertyGrid::mount(
        &mut ui,
        root,
        source.clone(),
        &[1, 2],
        settings(),
        &DataFormFactories::default(),
        LayoutStyle {
            width: Length::Px(920.0),
            ..Default::default()
        },
    )
    .unwrap();
    ui.update(Size::new(960.0, 700.0)).unwrap();
    (ui, grid, source)
}
#[test]
fn mixed_is_explicit_without_first_object_value_and_exact_integer_apply_is_one_event() {
    let (mut ui, mut grid, source) = fixture();
    for name in ["name", "count", "enabled"] {
        assert_eq!(grid.accepted(name), Some(PropertyValue::Mixed));
        assert_eq!(grid.draft(&ui, name).unwrap(), None);
        assert!(
            !ui.runtime
                .tree()
                .node(grid.property(name).unwrap())
                .unwrap()
                .props
                .visible
        );
    }
    grid.begin_edit(&mut ui, "count").unwrap();
    assert_eq!(
        grid.draft(&ui, "count").unwrap(),
        Some(CellValue::Integer(10))
    );
    assert_eq!(grid.accepted("count"), Some(PropertyValue::Mixed));
    grid.set_draft(&mut ui, "count", CellValue::Integer(i64::MAX))
        .unwrap();
    assert_eq!(
        grid.commit(&mut ui, "count").unwrap(),
        PropertyCommit::Saved
    );
    assert_eq!(
        grid.accepted("count"),
        Some(PropertyValue::Uniform(CellValue::Integer(i64::MAX)))
    );
    for key in [1, 2] {
        assert_eq!(
            source.borrow().value(key, "count"),
            Some(&CellValue::Integer(i64::MAX))
        );
    }
    assert_eq!(grid.take_changes().len(), 1);
    assert_eq!(
        grid.commit(&mut ui, "count").unwrap(),
        PropertyCommit::Unchanged
    );
    assert!(grid.take_changes().is_empty());
}
#[test]
fn nullable_reset_and_defaults_do_not_confuse_mixed_empty_or_false() {
    let (mut ui, mut grid, source) = fixture();
    grid.set_draft(&mut ui, "name", CellValue::Text(String::new()))
        .unwrap();
    grid.commit(&mut ui, "name").unwrap();
    assert_eq!(
        grid.accepted("name"),
        Some(PropertyValue::Uniform(CellValue::Text(String::new())))
    );
    grid.set_draft(&mut ui, "name", CellValue::Null).unwrap();
    grid.commit(&mut ui, "name").unwrap();
    assert_eq!(
        grid.accepted("name"),
        Some(PropertyValue::Uniform(CellValue::Null))
    );
    grid.reset(&mut ui, "name").unwrap();
    grid.set_draft(&mut ui, "enabled", CellValue::Bool(false))
        .unwrap();
    grid.commit(&mut ui, "enabled").unwrap();
    assert_eq!(
        grid.accepted("enabled"),
        Some(PropertyValue::Uniform(CellValue::Bool(false)))
    );
    grid.reset(&mut ui, "enabled").unwrap();
    for key in [1, 2] {
        assert_eq!(
            source.borrow().value(key, "name"),
            Some(&CellValue::Text("Untitled".into()))
        );
        assert_eq!(
            source.borrow().value(key, "enabled"),
            Some(&CellValue::Null)
        );
    }
}
#[test]
fn optimistic_batch_checks_unchanged_targets_and_preserves_business_accepted_on_failure() {
    let (mut ui, mut grid, source) = fixture();
    grid.set_draft(&mut ui, "count", CellValue::Integer(i64::MAX))
        .unwrap();
    source
        .borrow_mut()
        .set_cell(2, "count", CellValue::Integer(17))
        .unwrap();
    assert!(matches!(
        grid.commit(&mut ui, "count").unwrap(),
        PropertyCommit::Rejected(_)
    ));
    assert_eq!(
        source.borrow().value(1, "count"),
        Some(&CellValue::Integer(i64::MIN))
    );
    assert_eq!(grid.accepted("count"), Some(PropertyValue::Mixed));
    assert_eq!(
        grid.draft(&ui, "count").unwrap(),
        Some(CellValue::Integer(i64::MAX))
    );
    assert!(grid.take_changes().is_empty());
    grid.refresh(&mut ui).unwrap();
    assert!(grid.error().unwrap().contains("Source changed"));
    assert_eq!(grid.accepted("count"), Some(PropertyValue::Mixed));
}
struct Rejecting {
    table: DataTable,
    calls: Rc<RefCell<usize>>,
}
impl DataAdapter for Rejecting {
    fn load(&mut self) -> Result<DataTable, DataError> {
        Ok(self.table.clone())
    }
    fn write_cell(
        &mut self,
        _: i64,
        _: &str,
        _: &CellValue,
        _: &CellValue,
    ) -> Result<DataTable, DataError> {
        Err(DataError::Adapter("setter rejected".into()))
    }
    fn write_capabilities(&self) -> WriteCapabilities {
        WriteCapabilities::transactional_crud()
    }
    fn write_batch(&mut self, _: &[DataWrite]) -> Result<DataTable, DataError> {
        *self.calls.borrow_mut() += 1;
        Err(DataError::Adapter("setter rejected".into()))
    }
}
#[test]
fn failing_setter_is_one_call_and_keeps_all_objects_and_accepted_state() {
    let calls = Rc::new(RefCell::new(0));
    let original = table();
    let source = Rc::new(RefCell::new(
        DataSource::new(Rejecting {
            table: original.clone(),
            calls: calls.clone(),
        })
        .unwrap(),
    ));
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let mut grid = PropertyGrid::mount(
        &mut ui,
        root,
        source.clone(),
        &[1, 2],
        settings(),
        &DataFormFactories::default(),
        LayoutStyle::default(),
    )
    .unwrap();
    assert!(matches!(
        grid.reset(&mut ui, "name").unwrap(),
        PropertyCommit::Rejected(_)
    ));
    assert_eq!(*calls.borrow(), 1);
    assert_eq!(source.borrow().table(), &original);
    assert_eq!(grid.accepted("name"), Some(PropertyValue::Mixed));
    assert_eq!(
        grid.draft(&ui, "name").unwrap(),
        Some(CellValue::Text("Untitled".into()))
    );
    assert!(grid.take_changes().is_empty());
}
#[test]
fn nested_expand_cancel_and_theme_preserve_editor_identity_and_draft() {
    let (mut ui, mut grid, _) = fixture();
    let editor = grid.property_editor("count").unwrap();
    grid.begin_edit(&mut ui, "count").unwrap();
    ui.set_text(editor, "42").unwrap();
    let group = grid.group(&["Общие", "Размеры"]).unwrap();
    ui.set_expander_expanded(group.container, false).unwrap();
    ui.update(Size::new(960.0, 700.0)).unwrap();
    ui.set_theme(Theme::FluentDark).unwrap();
    ui.set_expander_expanded(group.container, true).unwrap();
    ui.update(Size::new(960.0, 700.0)).unwrap();
    assert_eq!(grid.property_editor("count"), Some(editor));
    assert_eq!(ui.document(editor).unwrap().text(), "42");
    grid.cancel(&mut ui, "count").unwrap();
    assert_eq!(grid.draft(&ui, "count").unwrap(), None);
    assert_eq!(grid.accepted("count"), Some(PropertyValue::Mixed));
}
#[test]
fn invalid_metadata_defaults_and_external_recipe_changes_fail_before_publication() {
    let (mut ui, mut grid, source) = fixture();
    let before = ui.runtime.tree().preorder();
    let root = ui.root();
    let mut invalid = settings();
    invalid.properties[0].default = Some(CellValue::Integer(1));
    assert!(
        PropertyGrid::mount(
            &mut ui,
            root,
            source,
            &[1, 2],
            invalid,
            &DataFormFactories::default(),
            LayoutStyle::default()
        )
        .is_err()
    );
    assert_eq!(before, ui.runtime.tree().preorder());
    let owner = grid.property("count").unwrap();
    let mut s = ui.numeric_settings(owner).unwrap().clone();
    s.precision = 2;
    ui.set_numeric_settings(owner, s).unwrap();
    assert!(
        grid.set_draft(&mut ui, "count", CellValue::Integer(2))
            .is_err()
    );
    assert_eq!(grid.accepted("count"), Some(PropertyValue::Mixed));
}
#[test]
fn readonly_composite_and_null_actions_are_unavailable() {
    let (mut ui, _, source) = fixture();
    let root = ui.root();
    let mut settings = settings();
    settings.read_only = true;
    let mut grid = PropertyGrid::mount(
        &mut ui,
        root,
        source,
        &[1, 2],
        settings,
        &DataFormFactories::default(),
        LayoutStyle::default(),
    )
    .unwrap();
    assert!(grid.begin_edit(&mut ui, "count").is_err());
    assert!(grid.set_draft(&mut ui, "name", CellValue::Null).is_err());
    for name in ["name", "count", "enabled"] {
        let (a, b, c) = grid.property_actions(name).unwrap();
        assert!(!ui.is_enabled(a) && !ui.is_enabled(b) && !ui.is_enabled(c));
    }
}
#[test]
fn keyboard_actions_are_nonfatal_and_own_activation_consumption_preserves_others() {
    let (mut ui, mut grid, _) = fixture();
    let (_, _, reset) = grid.property_actions("name").unwrap();
    ui.runtime.apply(Mutation::Focus(Some(reset))).unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Enter,
        modifiers: Default::default(),
        repeat: false,
    })
    .unwrap();
    ui.dispatch(InputEvent::KeyUp {
        key: Key::Enter,
        modifiers: Default::default(),
    })
    .unwrap();
    assert!(grid.update(&mut ui).unwrap());
    assert_eq!(
        grid.accepted("name"),
        Some(PropertyValue::Uniform(CellValue::Text("Untitled".into())))
    );
    grid.begin_edit(&mut ui, "count").unwrap();
    let editor = grid.property_editor("count").unwrap();
    ui.set_text(editor, "invalid").unwrap();
    let (_, apply, _) = grid.property_actions("count").unwrap();
    ui.perform_action(apply, SemanticAction::Activate, None)
        .unwrap();
    assert!(grid.update(&mut ui).unwrap());
    assert!(grid.error().is_some());
    assert_eq!(grid.accepted("count"), Some(PropertyValue::Mixed));
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    assert!(
        grid.input(
            &mut ui,
            &InputEvent::KeyDown {
                key: Key::Escape,
                modifiers: Default::default(),
                repeat: false
            }
        )
        .unwrap()
    );
    assert_eq!(grid.draft(&ui, "count").unwrap(), None);
}

#[test]
fn deterministic_seed_respects_custom_date_and_fractional_numeric_bounds() {
    let schema = TableSchema::new(vec![
        FieldSchema::new("date", FieldType::Date),
        FieldSchema::new("count", FieldType::Integer),
    ])
    .unwrap();
    let source = Rc::new(RefCell::new(
        DataSource::new(CollectionAdapter::new(
            DataTable::new(
                schema,
                vec![
                    DataRow::new(
                        1,
                        vec![
                            CellValue::Date("1980-01-01".parse().unwrap()),
                            CellValue::Integer(1),
                        ],
                    ),
                    DataRow::new(
                        2,
                        vec![
                            CellValue::Date("1981-01-01".parse().unwrap()),
                            CellValue::Integer(2),
                        ],
                    ),
                ],
            )
            .unwrap(),
        ))
        .unwrap(),
    ));
    let mut factories = DataFormFactories::default();
    factories
        .register(
            "PastDate",
            DataFormEditorFactory::Date(DateEditSettings {
                maximum: Some(DateValue::new(1990, 12, 31).unwrap()),
                ..Default::default()
            }),
        )
        .unwrap();
    factories
        .register(
            "FractionalBounds",
            DataFormEditorFactory::Numeric(NumericEditSettings {
                minimum: "0.5".parse().unwrap(),
                maximum: "3.5".parse().unwrap(),
                precision: 1,
                ..Default::default()
            }),
        )
        .unwrap();
    let mut date = PropertyDescriptor::new("date");
    date.metadata.editor = "PastDate".into();
    let mut count = PropertyDescriptor::new("count");
    count.metadata.editor = "FractionalBounds".into();
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let mut grid = PropertyGrid::mount(
        &mut ui,
        root,
        source,
        &[1, 2],
        PropertyGridSettings {
            properties: vec![date, count],
            ..Default::default()
        },
        &factories,
        LayoutStyle::default(),
    )
    .unwrap();
    grid.begin_edit(&mut ui, "date").unwrap();
    assert_eq!(
        grid.draft(&ui, "date").unwrap(),
        Some(CellValue::Date("1990-12-31".parse().unwrap()))
    );
    grid.begin_edit(&mut ui, "count").unwrap();
    assert_eq!(
        grid.draft(&ui, "count").unwrap(),
        Some(CellValue::Integer(1))
    );
    assert_eq!(grid.accepted("count"), Some(PropertyValue::Mixed));
}
#[test]
fn hostile_validator_error_is_bounded_and_nonfatal() {
    let (mut ui, mut grid, source) = fixture();
    grid.begin_edit(&mut ui, "name").unwrap();
    let editor = grid.property_editor("name").unwrap();
    let null = grid.property_null_toggle("name").unwrap();
    ui.set_checked(null, false).unwrap();
    ui.set_editor_validator(
        editor,
        Some(EditorValidator::new(|_| {
            Err(format!("A{}", "\u{0345}".repeat(40)))
        })),
    )
    .unwrap();
    ui.set_text(editor, "draft").unwrap();
    assert!(matches!(
        grid.commit(&mut ui, "name").unwrap(),
        PropertyCommit::Rejected(_)
    ));
    assert_eq!(grid.error(), Some("Property change was rejected"));
    assert_eq!(grid.accepted("name"), Some(PropertyValue::Mixed));
    assert_eq!(source.borrow().value(1, "name"), Some(&CellValue::Null));
}
