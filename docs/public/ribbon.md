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
колонками по три; эти размеры не определяют поведение команды. `register_toggle`
создаёт переключаемую команду: `checked: Some(bool)` означает Toggle, `None` —
обычное действие. Поэтому Small, Large и primary/secondary actions Split могут
быть переключаемыми. Split имеет отдельную primary action и кнопку меню;
раскрытие меню само по себе не выполняет primary action. Secondary commands
используют те же CommandID, что и другие controls приложения.

`RibbonItemKind::Toggle` сохранён для совместимости как крупное представление
с начальной подсказкой типа. При первом mount Ribbon обходит все такие элементы,
включая скрытые contextual tabs, до построения QAT. У уже зарегистрированной
команды `None` становится `Some(false)`; существующий checked сохраняется.
Отсутствующие команды автоматически не регистрируются. Этот проход выполняется
один раз за время жизни Ribbon. Поздние изменения `Some` ↔ `None` не отменяются
при смене темы, ширины, context или повторном mount. Для нового кода предпочтительна
явная `register_toggle` перед mount и обычные Small/Large presentations.

Tab и group IDs стабильны в модели. `set_active_tab` переключает доступную
вкладку. `set_context(Some(name))` показывает вкладки с соответствующим context;
если активная вкладка исчезает, выбирается первая доступная. `add_quick_access`
и `remove_quick_access` изменяют QAT без дублирования command state.

## Compiled XAML

Профиль [XAML](xaml.md#ribbon-в-xaml) описывает эту же модель через
`Ribbon`, `RibbonTab`, `RibbonGroup`, `Button`, `ToggleButton` и `SplitButton`.
`Ribbon.QuickAccessToolbar` содержит ссылки на команды. Разметка компилируется
в `RibbonDefinition` при сборке, а адаптер примера преобразует его в RibbonModel.
Label, enabled и checked по-прежнему задаёт CommandRegistry. Адаптер заранее
проверяет все ссылки и требует уже зарегистрированное checked state для
декларативного ToggleButton; legacy-подсказка `RibbonItemKind::Toggle` ему не нужна.

`RibbonCommandAppearance` задаёт `icon: Option<ButtonIcon>` и
`tool_tip: Option<String>` для command placements. Передайте его через
`Ribbon::set_command_appearance` до mount или update. ToolTip `None` использует
подпись команды, пустая строка отключает подсказку. Собственные IconSymbol и
общий ToolTip lifecycle используют те же примитивы Controls, что обычные кнопки.
QAT и body получают одно описание, а labels и state продолжают поступать из registry.

Пример запускается командой `cargo run -p xaml-form --locked`. Его
[MainWindow.xaml](../../examples/xaml-form/MainWindow.xaml) задаёт ленту вместе
с редактором; `--controls` открывает отдельную форму базовых элементов.
В одном окне поддерживается один Ribbon; вкладки и command items не создают
отдельных XAML namescope entries. Embedded editors, backstage, templates и
runtime bindings пока не входят в профиль.

## Интеграция

1. `ribbon.mount(&mut ui, parent)` создаёт контейнер.
2. `ribbon.update(&mut ui, available_width)` обновляет его при смене ширины,
   темы или модели. Затем `ui.update(viewport)` вычисляет geometry.
3. Перед `ui.dispatch(event)` вызовите `ribbon.handle_input(&mut ui, &event)`.
   Если возвращено true, событие уже обработано KeyTips.
4. После dispatch вызовите `ribbon.process(&mut ui)`: обработаются tabs и
   dropdown buttons. Возвращённые WidgetId относятся к остальной части UI.
5. Приложение читает command invocations из общей registry и рисует `ui.scene()`.

Если ширина родителя известна до первого layout, используйте
`mount_with_width(&mut ui, parent, width)`. `preflight(&mut ui, width)` проверяет
план и измеряет желаемую высоту до монтажа. XAML-адаптер использует отдельный
стабильный wrapper и передаёт внутреннему Ribbon его фактическую ширину после
layout; перестройка командных placements не заменяет остальные controls формы.

Повторный `mount` поддерживается только с тем же существующим parent и живым
контейнером. Другой parent или удалённый контейнер возвращают ошибку до изменения
состояния Ribbon. Для нового размещения после удаления создайте новый Ribbon;
автоматическое восстановление или перенос контейнера этим методом не выполняются.

Во время KeyTips host подавляет отдельные committed Text events, чтобы буква
команды не попадала в TextBox. В том числе подавляется парное событие последней
команды, даже если KeyTips к этому моменту уже скрыты. Alt/F10 переводят focus
на Ribbon; завершение режима возвращает доступный прежний focus. Контейнер
сохраняет положение среди siblings при rebuild; selection/context/QAT и checked
state принадлежат модели/registry. Внутренние item WidgetId могут изменяться при
перекомпоновке; приложение не должно использовать их как CommandID.
При восстановлении фокуса команды с несколькими размещениями выбирается первое
доступное совпадение в логическом порядке построенного дерева; отдельная
идентичность размещения между rebuild пока не сохраняется.

Перед явным accessibility action host вызывает `cancel_key_tips(&mut ui)` и
снимает собственное подавление Text events. Этот метод очищает режим и префикс
без восстановления прежнего focus: целевой focus определяет действие пользователя.

Все способы пользовательской активации используют `CommandRegistry::activate`:
body, QAT, меню групп и More, primary/secondary Split, обычная клавиатура и KeyTips.
Доступность проверяется до изменения checked; успешный Toggle меняется один раз,
callback вызывается один раз, а `CommandInvocation` содержит новое checked.
`Ribbon::process` не вызывает команду повторно. Программный `execute` остаётся
прямым вызовом без переключения checked и не используется для этих маршрутов.

Уже построенные command placements читают текущую registry, включая смену
`Some` ↔ `None`: checked, семантика и поведение меняются без пересоздания WidgetId.
Это действует и для открытого меню. Обновление рисуемой сцены требует `ui.update`.
Обычные изменения checked, disabled или can_execute сами по себе не перестраивают
Ribbon. Hover, press и focus принадлежат конкретному размещению.

## Адаптация к ширине

Runtime вычисляет Full/Collapsed/Overflow по измеренной ширине команд: учитываются
текущая подпись, видимый KeyTip, иконка с интервалом и padding. Small items делят
колонку по три; её ширина равна максимальной ширине элементов. Группы с
большим `collapse_priority` схлопываются первыми. Collapsed group заменяется
кнопкой, открывающей меню всех её команд, включая secondary commands Split.
Если даже все collapsed groups не помещаются, остаётся одно меню All commands.
Команды не исчезают при переходе между планами. Очень узкий viewport может
обрезать подпись, но не меняет набор доступных command actions.

QAT размещает начальную последовательность команд по их измеренным ширинам.
Если все команды помещаются, More не создаётся; иначе расчёт резервирует место
под кнопку More и интервалы, а оставшиеся команды попадают в её меню.
Один измеренный план используется при preflight и построении controls, поэтому
проверка generated menu limits соответствует фактическому размещению.
PopupMenu ограничивается viewport
и прокручивается при необходимости. Tabs делят доступную ширину без наложения.
Детерминированные тесты проверяют три ширины, порядок siblings и сохранение
доступа к overflow commands.

Ширина отдельной Large/QAT кнопки имеет минимум 104 logical pixels, Small — 128;
измеренные ширины ограничены 512. Более длинная подпись может обрезаться, при
этом полная строка остаётся в registry и подсказке команды. Collapsed group
учитывает измеренный заголовок, строку Commands, padding и стрелку; минимум — 88,
максимум — 512, но её ширина не превышает ширину полной группы. Схлопывание
применяется только когда оно уменьшает группу; иначе при нехватке места
используется All commands. Вкладки имеют ширину не больше 150. Это ограниченная
командная компоновка, без произвольных XAML styles/templates для Ribbon items.

Публичный `layout_groups(groups, width)` сохранён как независимая оценка по
базовым размерам 104/128. Он не принимает текстовую систему или appearance и
не предсказывает точный план смонтированного Ribbon с измеренными подписями.
Для фактического плана после `update` используйте `Ribbon::presentations()`.

QAT More, collapsed groups, All commands и secondary часть Split используют
общий DropDownButton и общий popup lifecycle. Вертикальная композиция Ribbon
Split сохраняется; самостоятельный `Controls::split_button` располагает части
горизонтально. Пустая secondary menu или пустая свёрнутая группа отображается
отключённой кнопкой без popup. В узком QAT More рисуется стрелкой и сохраняет
доступное имя More.

Одна генерируемая menu ограничена 256 entries, включая secondary команды Split.
Перед mount/rebuild проверяется фактический план для выбранной ширины: превышение
возвращает ошибку до изменения существующего дерева, popup и command checked.
Общее число команд модели может быть больше, если данный план не создаёт меню
сверх лимита; уменьшение ширины тогда может вернуть ошибку. Изменённая модель
остаётся доступной для исправления, а старое визуальное дерево сохраняется.

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
KeyTips работают только у смонтированного, существующего и доступного Ribbon.
Скрытие или отключение контейнера либо предка отменяет режим и накопленный
префикс при следующем событии; команды через KeyTips не обходят эту границу.

Темы Light/Dark/Compact меняют colors и density через общие Controls tokens.
Высота подписей групп и малых кнопок вычисляется по реальным метрикам
системного шрифта. Caption учитывает переносы при измеренной ширине своей группы;
группы и контейнер увеличиваются при необходимости. Измеряется вся модель,
поэтому смена вкладки, context или overflow сохраняет высоту в пределах темы.
Размер шрифта не используется как предположение о высоте строки.
Координаты остаются logical pixels; native host и renderer применяют DPI.
Фактические проверки разных scale factors и доступности screen reader
фиксируются отдельно от unit tests и compile CI.
