use rust_desktop_ui_controls::{
    Controls, RibbonGroup, RibbonItem, RibbonItemKind as Kind, RibbonModel, RibbonTab,
};

pub fn model(ui: &mut Controls) -> RibbonModel {
    for (id, label) in [
        ("open", "Открыть"),
        ("save", "Сохранить"),
        ("menu", "Действия"),
        ("edit", "Изменить"),
        ("copy", "Копировать"),
        ("expand", "Раскрыть"),
        ("collapse", "Свернуть"),
        ("light", "Светлая"),
        ("dark", "Тёмная"),
        ("compact", "Компактная"),
        ("pin", "Закрепить"),
        ("restore", "Сброс колонок"),
        ("qat_add", "+ Быстрый доступ"),
        ("qat_remove", "− Быстрый доступ"),
        ("first", "В начало"),
        ("last", "В конец"),
        ("clear", "Сбросить фильтр"),
        ("controls", "Компоненты"),
        ("library", "Библиотека"),
        ("about", "О проекте"),
        ("favorite", "Избранное"),
        ("disabled", "Недоступно"),
    ] {
        ui.commands_mut().register(id, label);
    }
    ui.commands_mut().set_enabled(&"disabled".into(), false);
    let group = |id: &str, label: &str, priority, items| RibbonGroup {
        id: id.into(),
        label: label.into(),
        collapse_priority: priority,
        items,
    };
    let item = |id: &str, kind, tip: &str| RibbonItem::new(id, kind, tip);
    RibbonModel {
        quick_access: vec!["save".into(), "copy".into()],
        tabs: vec![
            RibbonTab {
                id: "home".into(),
                label: "Главная".into(),
                key_tip: "H".into(),
                context: None,
                groups: vec![
                    group(
                        "file",
                        "Файл",
                        2,
                        vec![
                            item("open", Kind::Large, "O"),
                            item("save", Kind::Small, "S"),
                            item(
                                "menu",
                                Kind::Split {
                                    menu: vec!["edit".into(), "copy".into(), "disabled".into()],
                                },
                                "M",
                            ),
                        ],
                    ),
                    group(
                        "editing",
                        "Редактирование",
                        1,
                        vec![
                            item("edit", Kind::Large, "E"),
                            item("copy", Kind::Small, "C"),
                            item("favorite", Kind::Toggle, "F"),
                        ],
                    ),
                    group(
                        "tree",
                        "Структура",
                        0,
                        vec![
                            item("expand", Kind::Small, "X"),
                            item("collapse", Kind::Small, "Z"),
                            item("clear", Kind::Small, "R"),
                        ],
                    ),
                ],
            },
            RibbonTab {
                id: "view".into(),
                label: "Вид".into(),
                key_tip: "V".into(),
                context: None,
                groups: vec![
                    group(
                        "theme",
                        "Тема",
                        2,
                        vec![
                            item("light", Kind::Large, "L"),
                            item("dark", Kind::Small, "D"),
                            item("compact", Kind::Small, "C"),
                        ],
                    ),
                    group(
                        "columns",
                        "Колонки",
                        1,
                        vec![
                            item("pin", Kind::Toggle, "P"),
                            item("restore", Kind::Small, "R"),
                        ],
                    ),
                    group(
                        "quick",
                        "Быстрый доступ",
                        0,
                        vec![
                            item("qat_add", Kind::Small, "A"),
                            item("qat_remove", Kind::Small, "Q"),
                        ],
                    ),
                ],
            },
            RibbonTab {
                id: "tools".into(),
                label: "Инструменты".into(),
                key_tip: "T".into(),
                context: None,
                groups: vec![
                    group(
                        "navigate",
                        "Навигация",
                        2,
                        vec![
                            item("first", Kind::Large, "F"),
                            item("last", Kind::Small, "L"),
                        ],
                    ),
                    group(
                        "gallery",
                        "Галерея",
                        1,
                        vec![
                            item("controls", Kind::Small, "C"),
                            item("library", Kind::Small, "B"),
                        ],
                    ),
                    group(
                        "help",
                        "Справка",
                        0,
                        vec![
                            item("about", Kind::Large, "A"),
                            item("disabled", Kind::Small, "D"),
                        ],
                    ),
                ],
            },
            RibbonTab {
                id: "selection".into(),
                label: "Выбранная книга".into(),
                key_tip: "B".into(),
                context: Some("book".into()),
                groups: vec![group(
                    "book",
                    "Книга",
                    0,
                    vec![
                        item("edit", Kind::Large, "E"),
                        item("copy", Kind::Small, "C"),
                        item("favorite", Kind::Toggle, "F"),
                    ],
                )],
            },
        ],
    }
}
