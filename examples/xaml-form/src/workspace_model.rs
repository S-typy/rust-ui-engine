//! Application actions for the reusable workspace components.
use crate::form::Form;
use rust_desktop_ui_controls::Controls;
use rust_desktop_ui_core::{InputEvent, Key};
use rust_desktop_ui_platform_winit::{
    DialogButton, DialogButtons, DialogCompletion, DialogOptions, DialogOutcome, DialogRequest,
    DialogSession, FileDialog, FileDialogFilter, FileDialogKind, MessageDialog, PlatformEvent,
};

#[derive(Default)]
pub(crate) struct WorkspaceState {
    pub(crate) native_request: Option<DialogRequest>,
    pub(crate) open_modal: bool,
    pub(crate) session: Option<DialogSession<DialogOutcome>>,
    pub(crate) initial_directory: Option<std::path::PathBuf>,
    next_request: u64,
    alternate_template: bool,
}
pub(crate) fn register(ui: &mut Controls) {
    for (id, label) in [
        ("WorkspaceDialog", "Диалог формы"),
        ("WorkspaceMessage", "Сообщение"),
        ("WorkspaceOpen", "Открыть файл"),
        ("WorkspaceSave", "Выбрать имя"),
        ("WorkspaceFolder", "Выбрать папку"),
        ("DialogAccept", "Принять"),
        ("DialogCancel", "Отмена"),
        ("WorkspaceReorder", "Переместить вкладку"),
        ("WorkspaceQuery", "Выполнить запрос"),
        ("WorkspaceTemplate", "Сменить шаблон"),
    ] {
        ui.commands_mut().register(id, label);
    }
}
pub(crate) fn label(form: &mut Form, name: &str, value: &str) -> Result<(), String> {
    if let Some(id) = form.find_name(name) {
        form.ui
            .set_label(id, value)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
pub(crate) fn command(form: &mut Form, command: &str) -> Result<bool, String> {
    let options = match command {
        "WorkspaceDialog" => {
            form.workspace.open_modal = true;
            return Ok(true);
        }
        "DialogAccept" => {
            accept(form)?;
            return Ok(true);
        }
        "DialogCancel" => {
            if let Some(session) = &mut form.workspace.session {
                session.cancel();
            }
            return Ok(true);
        }
        "WorkspaceReorder" => {
            if let Some(tabs) = form.find_name("WorkspaceTabs") {
                let mut order = form.ui.tab_order(tabs).ok_or("Missing tabs")?.to_vec();
                order.rotate_left(1);
                form.ui
                    .reorder_tabs(tabs, &order)
                    .map_err(|error| error.to_string())?;
            }
            return Ok(true);
        }
        "WorkspaceQuery" => {
            query(form)?;
            return Ok(true);
        }
        "WorkspaceTemplate" => {
            if let Some(owner) = form.find_name("TemplateAction") {
                let alternate = !form.workspace.alternate_template;
                let icon = if alternate { "Pin" } else { "Check" };
                let source = format!(
                    r#"<Window xmlns="urn:rust-ui-engine:ui" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"><Window.Resources><ControlTemplate x:Key="Action" TargetType="Button"><StackPanel Orientation="Horizontal" Spacing="12"><Icon Symbol="{icon}" Width="20" Height="20"/><ContentPresenter x:Name="PART_Content"/></StackPanel></ControlTemplate></Window.Resources><Button Content="Сменить шаблон" Template="{{StaticResource Action}}"/></Window>"#
                );
                let definition =
                    rust_desktop_ui_xaml::parse(&source).map_err(|error| error.to_string())?;
                let template = definition
                    .content
                    .common
                    .control_template
                    .as_ref()
                    .ok_or("Template missing")?;
                form.view
                    .replace_control_template(&mut form.ui, owner, template)?;
                if let Some(group) = form.find_name("DocumentGroup") {
                    let header = rust_desktop_ui_xaml::parse(if alternate {
                        r#"<Window xmlns="urn:rust-ui-engine:ui"><TextBlock Text="Документ · новый заголовок" FontWeight="SemiBold" VerticalAlignment="Center"/></Window>"#
                    } else {
                        r#"<Window xmlns="urn:rust-ui-engine:ui"><TextBlock Text="Документ" FontWeight="SemiBold" VerticalAlignment="Center"/></Window>"#
                    }).map_err(|error| error.to_string())?;
                    form.view.replace_group_header_template(
                        &mut form.ui,
                        group,
                        &header.content,
                    )?;
                }
                form.workspace.alternate_template = alternate;
            }
            return Ok(true);
        }
        "WorkspaceMessage" => DialogOptions::Message(MessageDialog {
            title: "Подтверждение действия".into(),
            message: "Продолжить работу с документом?".into(),
            buttons: DialogButtons::YesNoCancel,
            default_button: DialogButton::Cancel,
        }),
        "WorkspaceOpen" | "WorkspaceSave" | "WorkspaceFolder" => {
            let kind = match command {
                "WorkspaceOpen" => FileDialogKind::Open,
                "WorkspaceSave" => FileDialogKind::Save,
                _ => FileDialogKind::Folder,
            };
            let mut file = FileDialog::new(kind);
            file.initial_directory = form.workspace.initial_directory.clone();
            file.title = match kind {
                FileDialogKind::Open => "Выберите файл",
                FileDialogKind::Save => "Выберите имя файла",
                FileDialogKind::Folder => "Выберите папку",
            }
            .into();
            if kind != FileDialogKind::Folder {
                file.filters = vec![
                    FileDialogFilter {
                        label: "Текстовые документы".into(),
                        extensions: vec!["txt".into(), "md".into()],
                    },
                    FileDialogFilter {
                        label: "Все файлы".into(),
                        extensions: vec!["*".into()],
                    },
                ];
                file.multiple = kind == FileDialogKind::Open;
            }
            if kind == FileDialogKind::Save {
                file.file_name = Some("Новый документ.txt".into());
                file.default_extension = Some("txt".into());
            }
            DialogOptions::File(file)
        }
        _ => return Ok(false),
    };
    if form.workspace.native_request.is_none() {
        form.workspace.next_request = form
            .workspace
            .next_request
            .checked_add(1)
            .ok_or("Dialog request identity exhausted")?;
        form.workspace.native_request = Some(DialogRequest {
            id: form.workspace.next_request,
            options,
        });
    }
    Ok(true)
}
fn accept(form: &mut Form) -> Result<(), String> {
    if form.workspace.session.is_none() {
        return Ok(());
    }
    if let Some(editor) = form.find_name("DialogEditor") {
        form.ui
            .commit_edit(editor)
            .map_err(|error| error.to_string())?;
        if form.ui.validation_error(editor).is_some() {
            return Ok(());
        }
    }
    form.workspace
        .session
        .as_mut()
        .unwrap()
        .complete(DialogOutcome::Button(DialogButton::Ok));
    Ok(())
}
pub(crate) fn dialog_key(form: &mut Form, event: &PlatformEvent) -> Result<bool, String> {
    if form.workspace.session.is_none() {
        return Ok(false);
    }
    if let PlatformEvent::Input(InputEvent::KeyDown { key, modifiers, .. }) = event {
        if modifiers.control || modifiers.alt || modifiers.super_key {
            return Ok(false);
        }
        match key {
            Key::Escape => {
                form.workspace.session.as_mut().unwrap().cancel();
                return Ok(true);
            }
            Key::Enter
                if form
                    .ui
                    .runtime
                    .focused()
                    .is_none_or(|id| form.ui.document(id).is_some()) =>
            {
                if form
                    .ui
                    .runtime
                    .focused()
                    .and_then(|id| form.ui.document(id))
                    .is_some_and(|document| document.composition().is_some())
                {
                    return Ok(false);
                }
                accept(form)?;
                return Ok(true);
            }
            _ => {}
        }
    }
    Ok(false)
}

fn query(form: &mut Form) -> Result<(), String> {
    use rust_desktop_ui_data::*;
    let schema = TableSchema::new(vec![
        FieldSchema::new("Category", FieldType::Text),
        FieldSchema::new("Quantity", FieldType::Integer),
    ])
    .map_err(|e| e.to_string())?;
    let table = DataTable::new(
        schema,
        vec![
            DataRow::new(
                1,
                vec![
                    CellValue::Text("Оборудование".into()),
                    CellValue::Integer(12),
                ],
            ),
            DataRow::new(
                2,
                vec![CellValue::Text("Канцелярия".into()), CellValue::Integer(8)],
            ),
            DataRow::new(
                3,
                vec![
                    CellValue::Text("Оборудование".into()),
                    CellValue::Integer(5),
                ],
            ),
            DataRow::new(
                4,
                vec![CellValue::Text("Канцелярия".into()), CellValue::Integer(0)],
            ),
        ],
    )
    .map_err(|e| e.to_string())?;
    let result = QueryEngine::execute(
        &table,
        &QueryRequest {
            filter: Some(Criteria::Compare {
                left: QueryExpression::field("Quantity"),
                operator: ComparisonOperator::Greater,
                right: QueryExpression::literal(CellValue::Integer(0)),
            }),
            group_by: vec![QueryExpression::field("Category")],
            aggregates: vec![Aggregate {
                function: AggregateFunction::Sum,
                expression: Some(QueryExpression::field("Quantity")),
            }],
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?;
    let mut text = format!(
        "Строк после фильтра: {}. Общий итог: {}",
        result.row_keys.len(),
        result.aggregates[0].display()
    );
    for group in result.groups {
        text.push_str(&format!(
            "\n{}: {}",
            group.key[0].display(),
            group.aggregates[0].display()
        ));
    }
    label(form, "QueryResult", &text)
}
pub(crate) fn dialog_completed(
    form: &mut Form,
    completion: DialogCompletion,
) -> Result<(), String> {
    println!(
        "dialog-completed id={} result={:?}",
        completion.id, completion.result
    );
    let text = match completion.result {
        Ok(DialogOutcome::Cancelled) => "Выбор отменён; прежнее значение сохранено".into(),
        Ok(DialogOutcome::Button(button)) => format!("Результат: {button:?}"),
        Ok(DialogOutcome::Paths(paths)) => {
            let display = paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join("; ");
            label(form, "FilePath", &display)?;
            format!(
                "Выбрано: {}. Файловые операции не выполнялись.",
                paths.len()
            )
        }
        Err(error) => format!("Ошибка выбора: {error}"),
    };
    label(form, "DialogStatus", &text)
}
