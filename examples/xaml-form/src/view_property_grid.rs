//! Retained property inspector over explicitly registered typed objects.
use super::*;
use rust_desktop_ui_controls::{
    DataFormFactories, DataFormField, PropertyDescriptor, PropertyGrid, PropertyGridSettings,
};
use rust_desktop_ui_data::{CellValue, FieldType};
use rust_desktop_ui_xaml::PropertyGridDefinition;

fn settings(ui: &Controls, value: &PropertyGridDefinition) -> Result<PropertyGridSettings, String> {
    let source = ui
        .data_source(&value.data_source)
        .ok_or_else(|| format!("Unknown source '{}'", value.data_source))?;
    let source = source.try_borrow().map_err(err)?;
    if value
        .object_keys
        .iter()
        .any(|key| source.row(*key).is_none())
    {
        return Err("PropertyGrid names an unknown object key".into());
    }
    let properties = value
        .properties
        .iter()
        .map(|p| {
            let field = source
                .table()
                .schema()
                .field(&p.metadata.field)
                .ok_or_else(|| format!("Unknown property '{}'", p.metadata.field))?;
            let default = if p.default_is_null {
                Some(CellValue::Null)
            } else if let Some(raw) = &p.default_value {
                let v = match field.kind {
                    FieldType::Date => CellValue::Date(raw.parse().map_err(err)?),
                    FieldType::Bool => CellValue::Bool(raw.parse::<bool>().map_err(err)?),
                    _ => field.parse(raw).map_err(err)?,
                };
                // Static values use exact canonical storage spelling; Text is literal.
                let canonical = match &v {
                    CellValue::Date(value) => value.to_string(),
                    CellValue::Time(value) => value.to_string(),
                    CellValue::Bool(value) => value.to_string(),
                    _ => v.display(),
                };
                if v == CellValue::Null || (field.kind != FieldType::Text && canonical != *raw) {
                    return Err(format!(
                        "DefaultValue for '{}' must use canonical storage text",
                        field.name
                    ));
                }
                Some(v)
            } else {
                None
            };
            if let Some(v) = &default {
                field.validate(v).map_err(err)?;
            }
            Ok(PropertyDescriptor {
                metadata: DataFormField {
                    field: p.metadata.field.clone(),
                    label: p.metadata.label.clone(),
                    visible: p.metadata.visible,
                    read_only: p.metadata.is_read_only,
                    editor: p.metadata.editor.clone(),
                },
                category: p.category.clone(),
                description: p.description.clone(),
                groups: p.groups.clone(),
                choices: p.choices.clone(),
                choice_keys: p.choice_keys.clone(),
                mask: p.mask.clone(),
                uri_format: p.uri_format.clone(),
                currency_symbol: p.currency_symbol.clone(),
                minimum: p.minimum,
                maximum: p.maximum,
                step: p.step,
                full_width: p.full_width,
                preview: match p.preview {
                    rust_desktop_ui_xaml::PropertyPreview::Auto => {
                        rust_desktop_ui_controls::PropertyPreview::Auto
                    }
                    rust_desktop_ui_xaml::PropertyPreview::Switch => {
                        rust_desktop_ui_controls::PropertyPreview::Switch
                    }
                    rust_desktop_ui_xaml::PropertyPreview::Color => {
                        rust_desktop_ui_controls::PropertyPreview::Color
                    }
                    rust_desktop_ui_xaml::PropertyPreview::Progress => {
                        rust_desktop_ui_controls::PropertyPreview::Progress
                    }
                },
                default,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(PropertyGridSettings {
        properties,
        label_width: value.label_width,
        label_fraction: value.label_fraction,
        scrollable: value.scrollable,
        read_only: value.is_read_only,
    })
}
pub(super) fn validate_source(
    ui: &Controls,
    value: &PropertyGridDefinition,
    common: &rust_desktop_ui_xaml::CommonProperties,
) -> Result<(), String> {
    rust_desktop_ui_xaml::validate_property_grid(value, common)?;
    settings(ui, value)?;
    Ok(())
}
impl MountedView {
    pub(super) fn mount_property_grid(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        value: &PropertyGridDefinition,
        style: LayoutStyle,
        width: f32,
        context: &str,
    ) -> Result<WidgetId, String> {
        let settings = settings(ui, value)?;
        let source = ui
            .data_source(&value.data_source)
            .ok_or("Property source is not registered")?;
        let mut grid = PropertyGrid::mount_with_editors(
            ui,
            parent,
            source,
            &value.object_keys,
            settings,
            &DataFormFactories::default(),
            &self.property_editors,
            style,
        )
        .map_err(err)?;
        for p in &value.properties {
            if p.display_template.is_some() {
                let button = grid
                    .property_actions(&p.metadata.field)
                    .ok_or("Missing property row")?
                    .0;
                let slot = self.mount_property_presentation(
                    ui,
                    button,
                    p,
                    if p.full_width {
                        width
                    } else {
                        (width
                            - value
                                .label_fraction
                                .map_or(value.label_width, |fraction| width * fraction))
                        .max(0.)
                    },
                    context,
                )?;
                grid.configure_display_template(
                    ui,
                    &p.metadata.field,
                    slot.map(|id| (id, p.value_part().is_some_and(|v| v.common.width.is_none()))),
                )
                .map_err(err)?;
            }
        }
        let id = grid.container;
        self.property_grids.insert(id, grid);
        Ok(id)
    }
    pub(crate) fn property_grid_input(
        &mut self,
        ui: &mut Controls,
        event: &rust_desktop_ui_core::InputEvent,
    ) -> Result<bool, String> {
        for grid in self.property_grids.values_mut() {
            if grid.input(ui, event).map_err(err)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub(crate) fn pending_property_text_editor(&self, ui: &Controls) -> Option<WidgetId> {
        self.property_grids
            .values()
            .find_map(|grid| grid.pending_text_editor(ui))
    }
    pub(crate) fn prepare_property_text(
        &mut self,
        ui: &mut Controls,
        text: &str,
    ) -> Result<(), String> {
        for grid in self.property_grids.values_mut() {
            if grid.prepare_text_input(ui, text).map_err(err)? {
                break;
            }
        }
        Ok(())
    }
    pub(super) fn sync_property_grids(&mut self, ui: &mut Controls) -> bool {
        self.property_grids
            .retain(|id, _| ui.runtime.tree().contains(*id));
        let mut changed = false;
        for grid in self.property_grids.values_mut() {
            match grid.update(ui) {
                Ok(value) => changed |= value,
                Err(error) => eprintln!("PropertyGrid update: {error}"),
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_controls::{PropertyCommit, PropertyValue, Theme};
    use rust_desktop_ui_core::{InputEvent, Key, Mutation, Size};
    use rust_desktop_ui_data::{
        CollectionAdapter, DataRow, DataSource, DataTable, FieldSchema, TableSchema,
    };
    use std::{cell::RefCell, rc::Rc};
    fn source() -> rust_desktop_ui_controls::DataSourceHandle {
        Rc::new(RefCell::new(
            DataSource::new(CollectionAdapter::new(
                DataTable::new(
                    TableSchema::new(vec![FieldSchema::new("count", FieldType::Integer)]).unwrap(),
                    vec![
                        DataRow::new(1, vec![CellValue::Integer(i64::MIN)]),
                        DataRow::new(2, vec![CellValue::Integer(i64::MAX)]),
                    ],
                )
                .unwrap(),
            ))
            .unwrap(),
        ))
    }
    #[test]
    fn committed_text_and_ime_activate_the_focused_property_once_and_protect_passwords() {
        use rust_desktop_ui_platform_winit::{DesktopApp, PlatformEvent};
        let mut form = crate::form::Form::new(
            crate::generated_property_catalog::window_definition(),
            false,
        )
        .unwrap();
        form.ui.update(Size::new(760., 730.)).unwrap();
        let grid_id = form.view.names["EditorCatalog"];
        let grid = &form.view.property_grids[&grid_id];
        let display = grid.property_actions("title").unwrap().0;
        let editor = grid.property_editor("title").unwrap();
        let password_button = grid.property_actions("password").unwrap().0;
        let password = grid.property_editor("password").unwrap();
        form.ui
            .runtime
            .apply(Mutation::Focus(Some(display)))
            .unwrap();
        assert_eq!(form.ime_target(), Some(editor));
        assert!(form.ime_cursor().is_some());
        let key = |key| {
            PlatformEvent::Input(InputEvent::KeyDown {
                key,
                modifiers: Default::default(),
                repeat: false,
            })
        };
        form.event(key(Key::Character("f".into()))).unwrap();
        assert_eq!(form.ui.runtime.focused(), Some(display));
        form.event(PlatformEvent::Text("ф".into())).unwrap();
        assert_eq!(form.ui.document(editor).unwrap().text(), "ф");
        form.event(key(Key::Escape)).unwrap();
        assert_eq!(form.ui.runtime.focused(), Some(display));
        form.event(PlatformEvent::ImePreedit("に".into(), None))
            .unwrap();
        assert_eq!(form.ime_target(), Some(editor));
        assert!(form.ime_cursor().is_some());
        assert_eq!(form.ime_composition_active(), Some(true));
        form.event(PlatformEvent::ImeCommit("日本".into())).unwrap();
        assert_eq!(form.ui.document(editor).unwrap().text(), "日本");
        form.event(key(Key::Enter)).unwrap();
        assert_eq!(
            form.view.property_grids[&grid_id].accepted("title"),
            Some(PropertyValue::Uniform(CellValue::Text("日本".into())))
        );

        form.ui
            .runtime
            .apply(Mutation::Focus(Some(password_button)))
            .unwrap();
        assert_eq!(form.ime_target(), Some(password));
        assert!(form.ime_is_password());
        assert!(
            form.ui
                .semantics()
                .iter()
                .any(|node| node.id == password_button
                    && node.role == rust_desktop_ui_controls::Role::PasswordBox)
        );
        form.event(PlatformEvent::Text("secret-fragment".into()))
            .unwrap();
        assert!(form.ime_is_password());
        assert_eq!(
            form.ui.password_value(password).unwrap().expose_secret(),
            "secret-fragment"
        );
        form.ui.update(Size::new(760., 730.)).unwrap();
        assert!(
            form.ui
                .scene()
                .texts
                .iter()
                .all(|run| !run.text.contains("secret-fragment"))
        );
        form.event(key(Key::Escape)).unwrap();
    }

    #[test]
    fn compiled_inspector_mixed_exact_values_and_keyboard_cancel_retain_ids() {
        let window=rust_desktop_ui_xaml::parse(r#"<Window xmlns="urn:rust-ui-engine:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"><PropertyGrid x:Name="Inspector" DataSource="Objects" ObjectKeys="1,2"><PropertyGrid.Properties><Property FieldName="count" Category="Layout" Group="Dimensions" DefaultValue="9223372036854775807"/></PropertyGrid.Properties></PropertyGrid></Window>"#).unwrap();
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let source = source();
        ui.register_data_source("Objects", source.clone()).unwrap();
        let root = ui.root();
        let mut view = MountedView::mount(&mut ui, root, &window.content).unwrap();
        ui.update(Size::new(960.0, 600.0)).unwrap();
        let owner = view.names["Inspector"];
        let grid = view.property_grids.get_mut(&owner).unwrap();
        let editor = grid.property_editor("count").unwrap();
        assert_eq!(grid.accepted("count"), Some(PropertyValue::Mixed));
        grid.begin_edit(&mut ui, "count").unwrap();
        ui.set_text(editor, "7").unwrap();
        ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
        assert!(
            view.property_grid_input(
                &mut ui,
                &InputEvent::KeyDown {
                    key: Key::Escape,
                    modifiers: Default::default(),
                    repeat: false
                }
            )
            .unwrap()
        );
        let grid = view.property_grids.get_mut(&owner).unwrap();
        assert_eq!(grid.draft(&ui, "count").unwrap(), None);
        assert_eq!(grid.reset(&mut ui, "count").unwrap(), PropertyCommit::Saved);
        assert_eq!(grid.property_editor("count"), Some(editor));
        assert_eq!(
            source.borrow().value(1, "count"),
            Some(&CellValue::Integer(i64::MAX))
        );
    }
    #[test]
    fn incompatible_metadata_or_unknown_identity_rolls_back_whole_mount() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        ui.register_data_source("Objects", source()).unwrap();
        let root = ui.root();
        let before = ui.runtime.tree().preorder();
        for body in [
            r#"<PropertyGrid DataSource="Objects" ObjectKeys="1,3"/>"#,
            r#"<PropertyGrid DataSource="Objects" ObjectKeys="1,2"><PropertyGrid.Properties><Property FieldName="count" DefaultValue="1.5"/></PropertyGrid.Properties></PropertyGrid>"#,
        ] {
            let window = rust_desktop_ui_xaml::parse(&format!(
                "<Window xmlns='urn:rust-ui-engine:ui'>{body}</Window>"
            ))
            .unwrap();
            assert!(MountedView::mount(&mut ui, root, &window.content).is_err());
            assert_eq!(ui.runtime.tree().preorder(), before);
        }
    }

    #[test]
    fn property_defaults_use_storage_dates_and_booleans_instead_of_display_text() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let source = Rc::new(RefCell::new(
            DataSource::new(CollectionAdapter::new(
                DataTable::new(
                    TableSchema::new(vec![
                        FieldSchema::new("date", FieldType::Date),
                        FieldSchema::new("enabled", FieldType::Bool),
                    ])
                    .unwrap(),
                    vec![DataRow::new(
                        1,
                        vec![
                            CellValue::Date("2026-10-17".parse().unwrap()),
                            CellValue::Bool(false),
                        ],
                    )],
                )
                .unwrap(),
            ))
            .unwrap(),
        ));
        ui.register_data_source("Objects", source.clone()).unwrap();
        let root = ui.root();
        let xml = r#"<Window xmlns="urn:rust-ui-engine:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"><PropertyGrid x:Name="Inspector" DataSource="Objects" ObjectKeys="1"><PropertyGrid.Properties><Property FieldName="date" DefaultValue="2026-01-01"/><Property FieldName="enabled" DefaultValue="true"/></PropertyGrid.Properties></PropertyGrid></Window>"#;
        let window = rust_desktop_ui_xaml::parse(xml).unwrap();
        let mut view = MountedView::mount(&mut ui, root, &window.content).unwrap();
        let grid = view
            .property_grids
            .get_mut(&view.names["Inspector"])
            .unwrap();
        grid.reset(&mut ui, "date").unwrap();
        grid.reset(&mut ui, "enabled").unwrap();
        assert_eq!(
            source.borrow().value(1, "date"),
            Some(&CellValue::Date("2026-01-01".parse().unwrap()))
        );
        assert_eq!(
            source.borrow().value(1, "enabled"),
            Some(&CellValue::Bool(true))
        );
        for invalid in [
            xml.replace("2026-01-01", "01.01.2026"),
            xml.replace("true", "Да"),
        ] {
            let window = rust_desktop_ui_xaml::parse(&invalid).unwrap();
            assert!(MountedView::mount(&mut ui, root, &window.content).is_err());
        }
    }

    #[test]
    fn property_demo_allocates_editor_width_and_draws_mixed_captions_in_real_form() {
        let definition =
            rust_desktop_ui_xaml::parse(include_str!("../PropertyWindow.xaml")).unwrap();
        for theme in [Theme::FluentLight, Theme::FluentDark] {
            let mut form =
                crate::form::Form::new_with_model(definition.clone(), false, None).unwrap();
            form.ui.set_theme(theme).unwrap();
            let owner = form.view.find_name("DocumentProperties").unwrap();
            let editor = form.view.find_name("PropertyNameEditor").unwrap();
            for width in [1180.0, 1000.0] {
                form.ui.update(Size::new(width, 880.0)).unwrap();
                let grid = &form.view.property_grids[&owner];
                for field in ["name", "enabled", "identifier", "count"] {
                    let row = form
                        .ui
                        .runtime
                        .bounds(grid.property_row(field).unwrap())
                        .unwrap();
                    let run = form
                        .ui
                        .scene()
                        .texts
                        .iter()
                        .find(|run| {
                            run.text == "Разные значения" && run.bounds.intersection(row).is_some()
                        })
                        .unwrap_or_else(|| panic!("Missing mixed caption for {field} at {width}"));
                    let visible = run.bounds.intersection(run.clip).unwrap();
                    assert!(
                        visible.width >= 100.0 && visible.height >= 16.0,
                        "Mixed caption collapsed for {field}: {visible:?}"
                    );
                    assert!(
                        visible.width + 0.1 >= run.bounds.width
                            && visible.height + 0.1 >= run.bounds.height,
                        "Mixed caption clipped for {field}"
                    );
                }
                for field in ["price", "created"] {
                    let editor = grid.property_editor(field).unwrap();
                    assert!(form.ui.runtime.bounds(editor).is_none());
                    let rect = form
                        .ui
                        .runtime
                        .bounds(grid.property_actions(field).unwrap().0)
                        .unwrap();
                    assert!(rect.width >= 200.0, "Narrow {field} editor: {rect:?}");
                    let text = form.ui.document(editor).unwrap().text();
                    assert!(
                        form.ui
                            .scene()
                            .texts
                            .iter()
                            .any(|run| run.text == text && run.bounds.intersection(rect).is_some()),
                        "Missing {field} display"
                    );
                }
            }
            form.view
                .property_grids
                .get_mut(&owner)
                .unwrap()
                .begin_edit(&mut form.ui, "name")
                .unwrap();
            form.ui.set_text(editor, "Локальный черновик").unwrap();
            for width in [1180.0, 1000.0, 1180.0] {
                form.ui.update(Size::new(width, 880.0)).unwrap();
                assert_eq!(
                    form.view.property_grids[&owner].property_editor("name"),
                    Some(editor)
                );
                assert_eq!(
                    form.ui.document(editor).unwrap().text(),
                    "Локальный черновик"
                );
                let null = form.view.property_grids[&owner]
                    .property_null_toggle("name")
                    .unwrap();
                let null_bounds = form.ui.runtime.bounds(null).unwrap();
                let null_run = form
                    .ui
                    .scene()
                    .texts
                    .iter()
                    .find(|run| {
                        run.text == "NULL" && run.bounds.intersection(null_bounds).is_some()
                    })
                    .expect("Nullable control must draw its complete caption")
                    .clone();
                let natural = form
                    .ui
                    .measure_text_with_font_size(&null_run.text, 4096.0, false, null_run.font_size)
                    .unwrap();
                let visible = null_run.bounds.intersection(null_run.clip).unwrap();
                assert!(
                    natural.width <= visible.width + 0.1,
                    "NULL caption clipped: required={natural:?}, visible={visible:?}, width={width}, theme={theme:?}"
                );
                let rect = form.ui.runtime.bounds(editor).unwrap();
                assert!(rect.width >= 200.0, "Editing width collapsed: {rect:?}");
                let run = form
                    .ui
                    .scene()
                    .texts
                    .iter()
                    .find(|run| run.text == "Локальный черновик")
                    .expect("Editable draft must have a real text run");
                let visible = run.bounds.intersection(run.clip).unwrap();
                assert!(
                    visible.width + 0.1 >= run.bounds.width
                        && visible.height + 0.1 >= run.bounds.height,
                    "Draft clipped: {run:?}"
                );
            }
        }
    }

    #[test]
    fn publication_inspector_aligns_rows_and_tracks_the_selected_description() {
        use rust_desktop_ui_controls::SemanticAction;
        let mut form = crate::form::Form::new_with_model(
            crate::generated_property_editors::window_definition(),
            false,
            None,
        )
        .unwrap();
        let grid = form
            .view
            .property_grids
            .get_mut(&form.view.names["PublicationProperties"])
            .unwrap();
        let ui = &mut form.ui;
        for width in [660., 420., 900.] {
            ui.update(Size::new(width, 525.)).unwrap();
            let row = ui
                .runtime
                .bounds(grid.property_row("author").unwrap())
                .unwrap();
            let button = ui
                .runtime
                .bounds(grid.property_actions("author").unwrap().0)
                .unwrap();
            assert!(((button.x - row.x - 1.) / row.width - 0.44).abs() < 0.005);
            assert!(
                ui.runtime
                    .bounds(grid.row_viewport().unwrap())
                    .unwrap()
                    .height
                    > 250.
            );
            assert!(row.height <= 30., "ordinary row height {}", row.height);
            let y = |s| {
                ui.scene()
                    .texts
                    .iter()
                    .find(|t| t.text == s && t.bounds.intersection(row).is_some())
                    .unwrap()
                    .bounds
                    .y
            };
            assert!((y("Автор") - y("Анна Мельникова")).abs() < 0.75);
        }
        let color_row = grid.property_row("accent").unwrap();
        let before_edit = ui.runtime.bounds(color_row).unwrap();
        grid.begin_edit(ui, "accent").unwrap();
        ui.update(Size::new(900., 525.)).unwrap();
        assert_eq!(ui.runtime.bounds(color_row).unwrap(), before_edit);
        let color_editor = grid.property_editor("accent").unwrap();
        ui.set_text(color_editor, "#A40442").unwrap();
        grid.cancel(ui, "accent").unwrap();
        assert_eq!(
            ui.label_text(grid.property_actions("accent").unwrap().0),
            Some("#28769B")
        );
        grid.set_draft(ui, "accent", CellValue::Text("#A40442".into()))
            .unwrap();
        assert_eq!(grid.commit(ui, "accent").unwrap(), PropertyCommit::Saved);
        assert_eq!(
            ui.label_text(grid.property_actions("accent").unwrap().0),
            Some("#A40442")
        );
        let action = grid.property_actions("progress").unwrap().0;
        ui.perform_action(action, SemanticAction::Activate, None)
            .unwrap();
        grid.update(ui).unwrap();
        ui.update(Size::new(660., 525.)).unwrap();
        assert!(
            ui.scene()
                .texts
                .iter()
                .any(|t| t.text == "Доля завершённых проверок перед публикацией.")
        );
        assert!(
            ui.runtime
                .bounds(grid.property_editor("progress").unwrap())
                .is_none()
        );
        grid.begin_edit(ui, "alerts").unwrap();
        let checkbox = grid.property("alerts").unwrap();
        assert_eq!(
            ui.check_presentation(checkbox),
            Some(rust_desktop_ui_controls::CheckPresentation::Switch)
        );
        ui.set_checked(checkbox, false).unwrap();
        grid.commit(ui, "alerts").unwrap();
        assert_eq!(
            ui.data_source("InspectionObjects")
                .unwrap()
                .borrow()
                .value(1, "alerts"),
            Some(&CellValue::Bool(false))
        );
        let (search, _, _) = grid.toolbar_parts();
        ui.set_text(search, "статус").unwrap();
        grid.update(ui).unwrap();
        ui.update(Size::new(660., 525.)).unwrap();
        assert!(
            ui.scene()
                .texts
                .iter()
                .any(|t| t.text == "Этап согласования перед публикацией.")
        );
    }

    #[test]
    #[ignore = "requires a hardware GPU adapter"]
    fn render_property_inspector_frames() {
        use rust_desktop_ui_platform_winit::DesktopApp;
        use rust_desktop_ui_xaml::Theme as WindowTheme;
        let output = std::env::var_os("PROPERTY_GRID_FRAME_DIR").map(std::path::PathBuf::from);
        let mut gpu =
            pollster::block_on(rust_desktop_ui_render_wgpu::OffscreenRenderer::new()).unwrap();
        eprintln!("GPU {:?}", gpu.adapter_info());
        for (name, theme, width, height, scale) in [
            ("publication", WindowTheme::Light, 660., 525., 1.25),
            ("dark", WindowTheme::Dark, 660., 525., 1.25),
            ("narrow", WindowTheme::Light, 420., 525., 1.25),
            ("scale100", WindowTheme::Light, 660., 525., 1.),
            ("scale150", WindowTheme::Light, 660., 525., 1.5),
            ("documents", WindowTheme::Light, 700., 760., 1.25),
        ] {
            let mut definition = if name == "documents" {
                crate::generated_properties::window_definition()
            } else {
                crate::generated_property_editors::window_definition()
            };
            definition.theme = theme;
            let mut form = crate::form::Form::new_with_model(definition, false, None).unwrap();
            let size = Size::new(width, height);
            form.update(size, scale).unwrap();
            form.update(size, scale).unwrap();
            let bitmap = gpu
                .render(
                    form.ui.scene(),
                    [
                        (width * scale).round() as u32,
                        (height * scale).round() as u32,
                    ],
                    scale,
                )
                .unwrap();
            let swatch_pixels = bitmap
                .pixels()
                .chunks_exact(4)
                .filter(|pixel| pixel[..3] == [0x28, 0x76, 0x9b])
                .count();
            assert!(
                swatch_pixels >= 50,
                "{name}: property color swatch outside the visible viewport"
            );
            if let Some(dir) = &output {
                std::fs::create_dir_all(dir).unwrap();
                std::fs::write(
                    dir.join(format!("{name}.png")),
                    rust_desktop_ui_images::encode_bitmap_png(&bitmap).unwrap(),
                )
                .unwrap();
            }
        }
    }
    #[test]
    fn compiled_inspector_filters_sorts_and_writes_a_templated_choice() {
        use rust_desktop_ui_controls::SemanticAction;
        let mut form = crate::form::Form::new_with_model(
            crate::generated_properties::window_definition(),
            false,
            None,
        )
        .unwrap();
        let ui = &mut form.ui;
        let grid = form
            .view
            .property_grids
            .get_mut(&form.view.names["DocumentProperties"])
            .unwrap();
        let size = Size::new(700., 850.);
        ui.update(size).unwrap();
        let (search, grouped, alphabetical) = grid.toolbar_parts();
        let small = Size::new(500., 400.);
        ui.update(small).unwrap();
        grid.update_scroll_extent(ui).unwrap();
        let viewport = grid.row_viewport().unwrap();
        let initial = ui.runtime.bounds(search).unwrap();
        ui.scroll_to(viewport, 180.).unwrap();
        ui.update(small).unwrap();
        assert!(ui.scroll_offset(viewport).unwrap().y > 100.);
        assert_eq!(ui.runtime.bounds(search), Some(initial));
        ui.scroll_to(viewport, 0.).unwrap();
        ui.update(size).unwrap();
        grid.update_scroll_extent(ui).unwrap();
        let name = grid.property_editor("name").unwrap();
        grid.set_draft(ui, "name", CellValue::Text("Черновик имени".into()))
            .unwrap();
        ui.perform_action(alphabetical, SemanticAction::Activate, None)
            .unwrap();
        grid.update(ui).unwrap();
        ui.update(size).unwrap();
        let row_y = |field: &str| {
            ui.runtime
                .bounds(grid.property_row(field).unwrap())
                .unwrap()
                .y
        };
        assert!(row_y("author") < row_y("edition") && row_y("edition") < row_y("name"));
        ui.set_text(search, "СТАТУС").unwrap();
        grid.update(ui).unwrap();
        ui.update(size).unwrap();
        assert!(ui.runtime.bounds(name).is_none());
        assert!(
            ui.runtime
                .bounds(grid.property_row("status").unwrap())
                .is_some()
        );
        assert!(
            !ui.scene()
                .texts
                .iter()
                .any(|run| run.text == "Свойства не найдены")
        );
        ui.set_text(search, "абракадабра").unwrap();
        grid.update(ui).unwrap();
        ui.update(size).unwrap();
        assert!(
            ui.scene()
                .texts
                .iter()
                .any(|run| run.text == "Свойства не найдены")
        );
        ui.perform_action(grouped, SemanticAction::Activate, None)
            .unwrap();
        ui.set_text(search, "").unwrap();
        grid.update(ui).unwrap();
        ui.update(size).unwrap();
        assert_eq!(ui.document(name).unwrap().text(), "Черновик имени");
        let group = grid.group(&["Параметры", "Размеры"]).unwrap();
        ui.set_expander_expanded(group.container, false).unwrap();
        ui.set_text(search, "количество").unwrap();
        grid.update(ui).unwrap();
        assert_eq!(ui.expander_expanded(group.container), Some(true));
        ui.set_text(search, "").unwrap();
        grid.update(ui).unwrap();
        assert_eq!(ui.expander_expanded(group.container), Some(false));
        grid.begin_edit(ui, "status").unwrap();
        let combo = grid.property("status").unwrap();
        ui.set_selected_keys(combo, &["Утверждён".into()]).unwrap();
        assert_eq!(grid.commit(ui, "status").unwrap(), PropertyCommit::Saved);
        assert!(
            grid.set_draft(ui, "status", CellValue::Text("Неизвестно".into()))
                .is_err()
        );
        ui.update(size).unwrap();
        assert!(ui.scene().texts.iter().any(|run| run.text == "Утверждён"));
        ui.set_text(search, "Карточка документа").unwrap();
        grid.update(ui).unwrap();
        ui.update(size).unwrap();
        let cover = ui
            .runtime
            .bounds(grid.property_row("cover").unwrap())
            .unwrap();
        let note = ui
            .runtime
            .bounds(grid.property_row("notes").unwrap())
            .unwrap();
        assert!(cover.height >= 112. && cover.y + cover.height <= note.y + 1.);
        assert!(
            ui.scene()
                .texts
                .iter()
                .any(|run| run.text.starts_with("Согласовать финальную редакцию")
                    && run.bounds.intersection(note).is_some())
        );
        let source = ui.data_source("InspectionObjects").unwrap();
        for key in [1, 2] {
            assert_eq!(
                source.borrow().value(key, "status"),
                Some(&CellValue::Text("Утверждён".into()))
            );
        }
    }
}
