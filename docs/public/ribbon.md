# Ribbon

`Ribbon` — композиция retained controls с независимыми CommandID. Приложение
передаёт `RibbonModel`; компонент не содержит бизнес-логику команд. Используются
собственные темы и прямоугольные/text primitives, без сторонних фирменных
ресурсов. Это общепринятая структура tabs/groups/actions, а не копия темы
конкретного коммерческого продукта.

## Модель

```rust
use rust_desktop_ui_controls::{Ribbon, RibbonModel, RibbonTab, RibbonGroup,
    RibbonItem, RibbonItemKind};

let ribbon = Ribbon::new(RibbonModel {
    quick_access: vec!["save".into()],
    tabs: vec![RibbonTab {
        id: "home".into(), label: "Главная".into(), key_tip: "H".into(),
        context: None,
        groups: vec![RibbonGroup {
            id: "document".into(), label: "Документ".into(), collapse_priority: 1,
            items: vec![
                RibbonItem::new("new", RibbonItemKind::Large, "N"),
                RibbonItem::new("save", RibbonItemKind::Small, "S"),
                RibbonItem::new("preview", RibbonItemKind::Toggle, "P"),
                RibbonItem::new("export", RibbonItemKind::Split {
                    menu: vec!["export.csv".into(), "export.json".into()],
                }, "E"),
            ],
        }],
    }],
})?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Команды регистрируются в `Controls::commands_mut()` до построения Ribbon.
Label, enabled/can_execute и checked state берутся из общей registry.
Large item занимает высокую ячейку, Small items укладываются вертикальными
колонками по три, Toggle сохраняет checked state, Split имеет отдельную primary
action и кнопку меню. Secondary commands используют те же CommandID, что и
другие controls приложения.

Tab и group IDs стабильны в модели. `set_active_tab` переключает доступную
вкладку. `set_context(Some(name))` показывает вкладки с соответствующим context;
если активная вкладка исчезает, выбирается первая доступная. `add_quick_access`
и `remove_quick_access` изменяют QAT без дублирования command state.

## Интеграция

1. `ribbon.mount(&mut ui, parent)` создаёт контейнер.
2. `ribbon.update(&mut ui, available_width)` обновляет его при смене ширины,
   темы или модели. Затем `ui.update(viewport)` вычисляет geometry.
3. Перед `ui.dispatch(event)` вызовите `ribbon.handle_input(&mut ui, &event)`.
   Если возвращено true, событие уже обработано KeyTips.
4. После dispatch вызовите `ribbon.process(&mut ui)`: обработаются tabs и
   dropdown buttons. Возвращённые WidgetId относятся к остальной части UI.
5. Приложение читает command invocations из общей registry и рисует `ui.scene()`.

Во время KeyTips host подавляет отдельные committed Text events, чтобы буква
команды не попадала в TextBox. Alt/F10 переводят focus на Ribbon. Контейнер
сохраняет положение среди siblings при rebuild; selection/context/QAT и checked
state принадлежат модели/registry. Внутренние item WidgetId могут изменяться при
перекомпоновке; приложение не должно использовать их как CommandID.

## Адаптация к ширине

`layout_groups` возвращает проверяемый план Full/Collapsed/Overflow. Группы с
большим `collapse_priority` схлопываются первыми. Collapsed group заменяется
кнопкой, открывающей меню всех её команд, включая secondary commands Split.
Если даже все collapsed groups не помещаются, остаётся одно меню All commands.
Команды не исчезают при переходе между планами. Очень узкий viewport может
обрезать подпись, но не меняет набор доступных command actions.

QAT размещает столько фиксированных items, сколько помещается в доступной
ширине; оставшиеся появляются через More. PopupMenu ограничивается viewport
и прокручивается при необходимости. Tabs делят доступную ширину без наложения.
Детерминированные тесты проверяют три ширины, порядок siblings и сохранение
доступа к overflow commands.

## Клавиатура

Alt или F10 показывает KeyTips tabs и первых девяти QAT items. Буква вкладки
выбирает её и показывает item KeyTips; последовательность символов item
выполняет команду, в том числе если её group сейчас в overflow. Цифры 1–9
выполняют соответствующие QAT commands. Escape возвращает к предыдущему уровню
или закрывает KeyTips. Ctrl+Alt не включает KeyTips, сохраняя AltGr text input.

Обычный Tab/Shift+Tab, Enter/Space и keyboard navigation popup остаются доступны.
Split secondary commands открываются кнопкой More и выбираются Up/Down/Enter.
KeyTip names проверяются при создании: только буквы/цифры, уникальные в пределах
своего уровня и не являющиеся префиксами друг друга. Tab KeyTips не начинаются
с зарезервированных для QAT цифр 1–9. Command availability
проверяется при выполнении, а не только при построении Ribbon.

Темы Light/Dark/Compact меняют colors и density через общие Controls tokens.
Высота подписей групп и малых кнопок вычисляется по реальным метрикам
системного шрифта. Caption учитывает переносы при ширине своей группы;
группы и контейнер увеличиваются при необходимости. Измеряется вся модель,
поэтому смена вкладки, context или overflow сохраняет высоту в пределах темы.
Размер шрифта не используется как предположение о высоте строки.
Координаты остаются logical pixels; native host и renderer применяют DPI.
Фактические проверки разных scale factors и доступности screen reader
фиксируются отдельно от unit tests и compile CI.
