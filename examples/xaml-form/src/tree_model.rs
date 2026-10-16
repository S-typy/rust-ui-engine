//! Application-owned hierarchy and controlled lazy completion for the example.
use crate::view::MountedView;
use rust_desktop_ui_controls::{BindingContext, Controls, DataValue, TreeSourceHandle};
use rust_desktop_ui_core::WidgetId;
use rust_desktop_ui_treegrid::{
    ColumnId, IndexedTreeSource, RequestId, RowKey, TreeChildren, TreeDataSource, TreeNode,
};
use std::{cell::RefCell, rc::Rc};

pub(crate) type SharedSource = Rc<RefCell<IndexedTreeSource>>;
pub(crate) fn model() -> Result<Rc<RefCell<BindingContext>>, String> {
    Ok(Rc::new(RefCell::new(
        BindingContext::new(DataValue::object([
            ("SelectedNode", "1".into()),
            ("ReadOnly", false.into()),
        ]))
        .map_err(err)?,
    )))
}
fn node(
    key: u64,
    parent: Option<u64>,
    label: impl Into<String>,
    kind: &str,
    state: &str,
) -> TreeNode {
    let mut node = TreeNode::new(RowKey(key), parent.map(RowKey), label);
    node.cells.insert(ColumnId(1), kind.into());
    node.cells.insert(ColumnId(2), state.into());
    node
}
fn records() -> Vec<TreeNode> {
    let mut records = vec![
        node(1, None, "Проект", "Папка", "Открыт"),
        node(2, None, "Документы", "Папка", "Общий доступ"),
        node(3, None, "Архив", "Папка", "2025–2026"),
        node(11, Some(1), "Исходный код", "Папка", "В работе"),
        node(12, Some(1), "План работ", "Документ", "Согласован"),
        node(
            u64::MAX,
            Some(1),
            "Узел с полным 64-битным ключом",
            "Документ",
            "Готов",
        ),
        node(111, Some(11), "main.rs", "Исходный код", "Изменён"),
        node(112, Some(11), "lib.rs", "Исходный код", "Готов"),
    ];
    let mut lazy = node(90, None, "Удалённые документы", "Папка", "Не загружено");
    lazy.children = TreeChildren::Unloaded;
    records.push(lazy);
    records.extend((0..200).map(|index| {
        node(
            1000 + index,
            Some(2),
            format!("Документ {:03}", index + 1),
            "Документ",
            if index % 3 == 0 {
                "На проверке"
            } else {
                "Готов"
            },
        )
    }));
    records
}
pub(crate) fn register(ui: &mut Controls) -> Result<SharedSource, String> {
    let source = Rc::new(RefCell::new(
        IndexedTreeSource::new(records()).map_err(err)?,
    ));
    ui.register_tree_source(
        "TreeNodes",
        TreeSourceHandle::from_shared(Rc::clone(&source)),
    )
    .map_err(err)?;
    for (id, label) in [
        ("TreeBeginLoad", "Начать загрузку"),
        ("TreeCompleteLoad", "Получить ответ"),
        ("TreeCancelLoad", "Отменить загрузку"),
        ("TreeRename", "Переименовать узел"),
        ("TreeReset", "Исходное дерево"),
        ("TreeReorderColumns", "Переставить колонки"),
        ("TreePinColumn", "Закрепить колонку"),
        ("TreeResizeColumn", "Изменить ширину"),
    ] {
        ui.commands_mut().register(id, label);
    }
    Ok(source)
}
pub(crate) struct State {
    tree: WidgetId,
    grid: WidgetId,
    summary: WidgetId,
    status: WidgetId,
    source: SharedSource,
    request: Option<RequestId>,
    responses: usize,
}
impl State {
    pub(crate) fn install(
        ui: &mut Controls,
        view: &MountedView,
        source: SharedSource,
    ) -> Result<Option<Self>, String> {
        let Some(tree) = view.find_name("Hierarchy") else {
            return Ok(None);
        };
        let required = |name| {
            view.find_name(name)
                .ok_or_else(|| format!("Missing {name}"))
        };
        let grid = required("HierarchyGrid")?;
        for id in [tree, grid] {
            ui.tree_set_expanded(id, RowKey(1), true).map_err(err)?;
        }
        Ok(Some(Self {
            tree,
            grid,
            summary: required("TreeSummary")?,
            status: required("TreeStatus")?,
            source,
            request: None,
            responses: 0,
        }))
    }
    pub(crate) fn command(&mut self, ui: &mut Controls, command: &str) -> Result<bool, String> {
        if !command.starts_with("Tree") {
            return Ok(false);
        }
        let message = match command {
            "TreeBeginLoad" => {
                self.request = ui
                    .tree_set_expanded(self.tree, RowKey(90), true)
                    .map_err(err)?
                    .or_else(|| {
                        ui.tree_model(self.tree)?
                            .pending_requests()
                            .into_iter()
                            .find(|request| request.parent() == RowKey(90))
                    });
                if self.request.is_some() {
                    "Запрос ожидает ответа. Можно отменить загрузку.".into()
                } else {
                    "Удалённые документы уже загружены. Сбросьте дерево для нового запроса.".into()
                }
            }
            "TreeCompleteLoad" => {
                if let Some(request) = self.request.take() {
                    let applied = ui
                        .tree_complete_indexed_children(
                            self.tree,
                            &self.source,
                            request,
                            vec![
                                node(901, Some(90), "Отчёт сервера", "Документ", "Загружен"),
                                node(902, Some(90), "Вложения", "Папка", "Загружены"),
                            ],
                        )
                        .map_err(err)?;
                    if applied {
                        self.responses += 1;
                        "Ответ принят: два дочерних узла добавлены атомарно.".into()
                    } else {
                        "Поздний ответ отклонён: источник остался без изменений.".into()
                    }
                } else {
                    "Нет сохранённого запроса. Нажмите «Начать загрузку».".into()
                }
            }
            "TreeCancelLoad" => {
                ui.tree_set_expanded(self.tree, RowKey(90), false)
                    .map_err(err)?;
                "Загрузка отменена. «Получить ответ» проверит отклонение позднего ответа.".into()
            }
            "TreeRename" => {
                let key = ui
                    .tree_selected_keys(self.tree)
                    .and_then(|keys| keys.first().copied())
                    .unwrap_or(RowKey(1));
                let mut records = self.source.borrow().nodes().to_vec();
                if let Some(node) = records.iter_mut().find(|node| node.key == key) {
                    node.cells
                        .insert(ColumnId(0), format!("Изменённый узел {}", key.0));
                }
                self.source.borrow_mut().replace(records).map_err(err)?;
                "Подпись изменена в обоих представлениях; ключ выбора сохранён.".into()
            }
            "TreeReset" => {
                self.source.borrow_mut().replace(records()).map_err(err)?;
                self.request = None;
                for id in [self.tree, self.grid] {
                    ui.tree_refresh(id).map_err(err)?;
                    ui.tree_set_expanded(id, RowKey(90), false).map_err(err)?;
                    ui.set_tree_selected_key(id, Some(RowKey(1))).map_err(err)?;
                    ui.tree_scroll_to(id, 0.0, 0.0).map_err(err)?;
                }
                "Исходное дерево восстановлено.".into()
            }
            "TreeReorderColumns" => {
                ui.tree_reorder_column(self.grid, ColumnId(2), Some(ColumnId(0)))
                    .map_err(err)?;
                "Колонка «Состояние» переставлена; закреплённые колонки остаются слева.".into()
            }
            "TreePinColumn" => {
                let pinned = ui
                    .tree_model(self.grid)
                    .unwrap()
                    .columns()
                    .iter()
                    .find(|column| column.id == ColumnId(2))
                    .unwrap()
                    .pinned;
                ui.tree_pin_column(self.grid, ColumnId(2), !pinned)
                    .map_err(err)?;
                format!(
                    "Закрепление колонки «Состояние»: {}.",
                    if pinned {
                        "выключено"
                    } else {
                        "включено"
                    }
                )
            }
            "TreeResizeColumn" => {
                let width = ui
                    .tree_model(self.grid)
                    .unwrap()
                    .columns()
                    .iter()
                    .find(|column| column.id == ColumnId(0))
                    .unwrap()
                    .width;
                ui.tree_resize_column(
                    self.grid,
                    ColumnId(0),
                    if width >= 360.0 { 220.0 } else { width + 40.0 },
                )
                .map_err(err)?;
                "Ширина колонки «Название» изменена.".into()
            }
            _ => return Ok(false),
        };
        ui.set_label(self.status, message).map_err(err)?;
        Ok(true)
    }
    pub(crate) fn refresh(&self, ui: &mut Controls) -> Result<(), String> {
        let selected = ui
            .tree_selected_keys(self.tree)
            .unwrap_or_default()
            .iter()
            .map(|key| key.0.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let pending = ui
            .tree_model(self.tree)
            .map_or(0, |model| model.pending_requests().len());
        let source = self.source.borrow();
        ui.set_label(self.summary,format!("Ключ: {selected} · записей: {} · версия: {} · запросов: {pending} · принятых ответов: {}",source.nodes().len(),source.revision(),self.responses)).map_err(err)
    }
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
