# Retained controls

`rust-desktop-ui-controls` строит интерактивные controls поверх `UiRuntime`,
`TaffyLayout` и текстовой подсистемы. Пакет не зависит от wgpu или winit.
`Controls::scene()` содержит фон, borders, shaped text commands и caret в общем
порядке рисования. Renderer получает эту сцену через независимый API.

## Создание и цикл приложения

```rust
use rust_desktop_ui_controls::{Controls, Theme};
use rust_desktop_ui_core::{LayoutStyle, Length, Size};

let mut ui = Controls::new(Theme::Light)?;
ui.commands_mut().register("save", "Сохранить");
let root = ui.root();
let save = ui.button(root, "Сохранить", Some("save".into()), LayoutStyle {
    width: Length::Px(160.0),
    ..Default::default()
})?;
ui.text_box(root, "Имя документа", "Документ", LayoutStyle {
    height: Length::Px(100.0),
    ..Default::default()
})?;
ui.update(Size::new(800.0, 600.0))?;
// Отправить ui.scene() в renderer.
# Ok::<(), Box<dyn std::error::Error>>(())
```

Оконный host нормализует `InputEvent`, передаёт его в `dispatch`, затем вызывает
`update` с текущим viewport перед чтением сцены. Committed text, preedit и IME
commit передаются отдельно: `text_input`, `ime_preedit`, `ime_commit`.
`cancel_preedit` отменяет незавершённую композицию. Enter обрабатывается как
команда редактирования; host не должен повторно вставлять управляющий символ
из того же keyboard event.

`needs_update` учитывает изменения runtime и текстовой сцены. `stats` возвращает
накопленные счётчики layout/paint. При простое без изменений повторный update
не выполняет эти проходы. Для мигания caret host может вызывать `blink_caret`
по своему таймеру; метод возвращает false без сфокусированного TextBox. Нет
необходимости запускать постоянный таймер для остальных controls; ToolTip
запрашивает только одно пробуждение на время ожидающего hover.

## Компоненты и состояния

| Builder | Поведение |
|---|---|
| `panel` | Контейнер с padding/layout/clip; default layout — вертикальный Stack |
| `transparent_panel` | Такой же контейнер без фоновой заливки, в том числе при смене темы и отключении |
| `label` | Текст, не получающий focus и pointer hit |
| `icon` | Собственная векторная геометрия; без focus, pointer hit и действий |
| `separator` | Декоративная горизонтальная или вертикальная линия внутри layout box |
| `button` | Primary press захватывает указатель; команда выполняется при release внутри того же control |
| `toggle` | Button с переключаемым checked state, общим с CommandID |
| `checkbox` | Подпись, отдельный квадрат-индикатор и checked state |
| `radio_button` | Круглый индикатор и единственный выбранный элемент явной группы |
| `dropdown_button` | Кнопка раскрытия статического меню |
| `split_button` | Независимые основная кнопка команды и стрелка меню |
| `text_box` | Unicode документ, caret, selection, multiline wrapping, preedit и undo/redo |
| `edit_box` | Однострочный TextBox с горизонтальной прокруткой caret и placeholder |
| `scroll_view` | Viewport с clipped content, ограниченной прокруткой и keyboard navigation |
| `tab_control` | Tab strip и overlay страниц; неактивные страницы не рисуются и не участвуют в focus |
| `popup_menu` | Overlay menu, keyboard navigation, отключённые и toggle items, возврат focus к anchor |
| `popup_menu_entries` | Иерархическое меню с подменю и разделителями |

Label контейнера Panel/ScrollViewer используется для accessibility; его
визуальное содержимое задаётся children или custom scene.

`measure_text(text, width, wrap)` возвращает размер содержимого в logical
pixels по тем же системным шрифтам, теме и правилам shaping, что и renderer.
Ширина задаётся без padding; действует лимит 1 MiB UTF-8 и ограничения
TextEngine. Явные размеры controls автоматически не увеличиваются: Label
высотой 20 px может обрезать строку более высокого системного шрифта.
Композиции могут использовать измерение для собственных размеров. Padding
кнопок по вертикали занимает только место, оставшееся после измеренной строки.
`measure_text_with_font_size(text, width, wrap, font_size)` позволяет измерить
тот же текст с явно заданным размером шрифта в `(0, 512]` logical pixels.

Pointer down у Button ещё не выполняет команду. Увод указателя и release снаружи
отменяют активацию; blur также отменяет pressed/capture state. Enter активирует
сфокусированный Button однократно на keydown. Space показывает pressed state и
активирует на keyup, если focus остался на том же control. Disabled controls и
disabled ancestors блокируют pointer, keyboard и semantic activation.

### ButtonStyle и Fluent-состояния

`Button`, `ToggleButton`, `CheckBox` и `RadioButton` используют общий `ButtonStyle`.
`set_button_style(id, style)` проверяет все метрики и цвета до применения;
ошибка сохраняет прежний стиль. Для других типов контролов метод возвращает
ошибку. `button_style` читает overrides, `button_visual_state` — текущие
`enabled`, `hovered`, `pressed`, `focused` и `checked`.

Метрики: `font_size` в `(0, 512]`, `padding`, `corner_radius` и `border_width`
в `[0, 16384]` logical pixels. Значения должны быть конечными; каналы цветов —
в `[0, 1]`. `None` наследует текущую тему, явные overrides сохраняются при смене
темы. Изменение стиля не заменяет текст, checked state или команду.

`ButtonAppearance` задаёт необязательные `background`, `foreground`, `border`,
`accent` и `checkmark`. Стиль содержит appearances `normal`, `checked`,
`hovered`, `pressed` и `disabled`. Разрешение идёт от темы и `normal` к checked,
затем к hover/press либо disabled; незаданные цвета остаются от предыдущего
слоя, кроме подстановок самой темы. Focus рисуется отдельно и совмещается с
checked/hover/press. Disabled подавляет hover, press и focus, сохраняя checked.
Для pointer press активное оформление действует пока указатель находится
внутри кнопки; release снаружи отменяет действие.

Fluent-темы используют скруглённые кнопки и внутреннее кольцо фокуса.
Для существующих Button/ToggleButton/CheckBox в Light/Dark/Compact прежнее оформление сохраняется до явного
`set_button_style`, который включает общий renderer для этого контрола.
У флажка строка прозрачна, а `background`/`border`/`corner_radius` задают отдельный
индикатор. Отметка строится собственной геометрией, без зависимости от символа
в шрифте. `foreground` относится к подписи, `checkmark` — к отметке.
Текст кнопок выравнивается по центру, флажка — после индикатора; по вертикали
учитываются метрики сформированной строки. Явные размеры автоматически не растут.

RadioButton использует прозрачную строку и круглый индикатор во всех темах.
Его диаметр — 18 logical pixels, ограниченный доступным местом после padding;
отступ подписи после индикатора — 8. Выбранное состояние по умолчанию рисует
фон theme focus и центральную точку диаметром 8; `checkmark` задаёт цвет точки.
Hover/press выбранного индикатора сохраняют этот фон до явного state override.
`corner_radius` у RadioButton влияет на кольцо фокуса строки, сам индикатор
и точка всегда круглые. `border_width` и цвета относятся к индикатору.

У командных Button/ToggleButton зарегистрированное `CommandRegistry.checked`
определяет режим и текущее состояние. Несколько размещений могут последовательно
переключить одну команду даже до следующего `update`. `checked`,
`button_visual_state` и checked/role/actions в `semantics` читают значение сразу;
новая сцена и геометрия публикуются после `update`.

`set_button_icon` добавляет ограниченный встроенный символ; произвольные дочерние
элементы и WPF templates пока не поддерживаются.
Button, меню, Ribbon/QAT и KeyTips используют общую активацию команды.
Hover, press и focus принадлежат отдельным размещениям.

### Расположение содержимого и оформление Ribbon

`set_button_content_layout(id, layout)` выбирает `ButtonContentLayout::Leading`,
`Above` или `IconOnly`. Leading сохраняет обычное расположение значка слева.
Above центрирует значок над подписью с переносом до двух строк и визуальным
многоточием при переполнении; текст сокращается на границе графемы. IconOnly
показывает только значок, а при его отсутствии использует текст. Эти настройки
не меняют полную подпись, команду, checked state и доступное имя.
`measure_button_content` использует тот же расчёт текста и отступов, что renderer;
сам по себе выбор представления не заменяет размеры, заданные host layout.

`set_button_chrome` выбирает `ButtonChrome::Standard`, `Flat` или `Tab`.
Standard сохраняет обычную поверхность кнопки. Flat прозрачен в покое,
сохраняет обратную связь hover/press/checked и кольцо клавиатурного фокуса.
Tab добавляет акцентную линию под выбранной вкладкой. Ribbon выбирает эти
представления для своих размещений; обычные кнопки формы сохраняют defaults.

`set_key_tip(id, Some(text))` задаёт отдельную визуальную метку; `None` её убирает.
Метка не добавляется к подписи, не получает фокус, не создаёт semantic node и
не участвует в измерении layout. Этот метод задаёт только отображение;
последовательности клавиш и активацию команд обрабатывает Ribbon.

### Icon, Separator и иконки кнопок

`icon(parent, symbol, layout)` создаёт самостоятельный Icon. `IconSymbol` содержит
`Check`, `Close`, `Add`, `Remove`, `ChevronDown`, `ChevronRight`, `Save`, `Pin`,
`FolderOpen`, `Copy`, `Edit`, `Eraser`, `Sun`, `Moon` и `Lock`.
Это собственная геометрия линий в системе 16×16, без шрифта, bitmap или внешних
ресурсов. Auto-размеры равны 16×16 logical pixels; явные прямоугольные bounds
равномерно масштабируют и центрируют геометрию по меньшей стороне. Толщина линии
1.5 при размере 16 масштабируется вместе с символом. Скруглённые окончания и
пересечения одного символа объединены в один `StrokePath`: полупрозрачный цвет
не накапливает alpha на стыках. Clip ограничивает видимость исходной геометрии.

`icon_symbol`/`set_icon_symbol` и `icon_style`/`set_icon_style` обращаются только
к живому Icon. `IconStyle` задаёт `foreground` и `disabled_foreground`; по умолчанию
это текущие theme text и muted, включая отключение предка. Каналы RGBA должны
быть конечными в `[0, 1]`. Icon не перехватывает pointer, не получает Tab-focus
и не предлагает semantic actions. Его роль — Image, имя по умолчанию пустое.
`set_label` задаёт доступное имя, не рисуя дополнительную строку текста.

`set_button_icon(id, Some(ButtonIcon::new(symbol)))` поддерживает Button и
ToggleButton, сохраняя подпись, ID, команду и checked state. `button_icon` читает
настройку; `None` удаляет её. `ButtonIcon` содержит `symbol`, `size` (по умолчанию
16) и `spacing` (8). Допустимы конечные size в `(0, 512]` и spacing в `[0, 16384]`.
Встроенная иконка не создаёт semantic child; доступное имя остаётся именем кнопки.
Приложение должно задавать осмысленное доступное имя кнопке без видимой подписи.

В Leading иконка и измеренная подпись центрируются совместно внутри ButtonStyle padding;
gap применяется только при непустой подписи. Цвет обоих берётся из resolved
ButtonStyle foreground, включая disabled и state overrides. Доступное место
ограничивает размер иконки и clip текста; явные bounds кнопки не увеличиваются.
В Light/Dark/Compact добавление иконки включает общий renderer этой кнопки.
После удаления иконки прежнее оформление возвращается, если явный ButtonStyle
не был установлен и сохранены Leading + Standard.

`separator(parent, settings, layout)` принимает `SeparatorSettings`:
`orientation: Axis` (по умолчанию Horizontal), `thickness` (1) и необязательный
`foreground` (theme border). Толщина конечна в `(0, 16384]`, RGBA — в `[0, 1]`.
Линия центрируется в прозрачном layout box и ограничивается его короткой стороной
и clip предка. Отключение не меняет декоративный цвет. Роль — Separator, без
pointer hit, focus и semantic actions; AccessKit отображает её как Splitter без
интерактивных действий.

Auto короткой стороны Separator следует thickness; длинную сторону предоставляет
контейнер или приложение. `separator_settings`/`set_separator_settings` читают и
меняют настройки. Явные размеры сохраняются. Если последующий low-level
`Mutation::SetStyle` изменил управляемую Auto-ось, она перестаёт следовать thickness.
Присваивание точно того же численного размера неотличимо от прежнего значения
и сохраняет управление. Смена только цвета не требует layout. Неверные типы
контролов, удалённые ID и недопустимые значения отклоняются до изменения состояния.

### RadioButton и группы

`radio_group()` создаёт непрозрачный `RadioGroupId`, принадлежащий текущему
`Controls`. `radio_button(parent, label, group, checked, layout)` добавляет элемент
без команды. Чужой идентификатор группы отклоняется до создания узла;
`radio_group_of(id)` возвращает группу живого RadioButton.

`set_checked(id, true)` выбирает элемент и снимает отметку со всех остальных
живых участников группы, включая скрытых и отключённых. `false` может оставить
группу без выбора. При последовательном создании нескольких checked элементов
последний выбирается вместо предыдущего. Удаление, скрытие или отключение
выбранного элемента само по себе не выбирает другой.

Pointer release, Enter, Space и semantic Activate только выбирают RadioButton;
повторная активация сохраняет выбор и обычное событие `drain_activated`.
Четыре стрелки переводят фокус и выбор к следующему/предыдущему доступному
участнику в логическом порядке дерева, с переходом между концами. Отключённые
и скрытые поддеревья пропускаются. Ctrl/Alt/Meta со стрелками не переключают группу;
Shift допускается. `prevent_default` в обработчике стрелки отменяет этот переход.

Для Tab/Shift+Tab группа предоставляет одну позицию: доступный выбранный элемент,
иначе первый доступный по дереву. Вход через Tab не меняет checked state.
Все участники остаются доступны для мыши и программного Focus. Tab из элемента,
сфокусированного без выбора, выходит из группы в соответствующем направлении;
если других Tab-позиций нет, обход возвращается к представителю этой группы.
Изменения структуры учитываются при следующем `update` или `dispatch`.

В `ui-core` свойство `NodeProps::tab_stop` (по умолчанию true) независимо от
`focusable`: false исключает узел только из Tab-обхода. Для RadioButton этим
свойством управляет Controls. Routed Tab сохраняет обычный `prevent_default`.
Семантика RadioButton содержит отдельную роль, checked и действия Focus/Activate;
переключающее действие Toggle не предлагается.

Tab/Shift+Tab обходят доступные controls. У TabControl Left/Right переключают
вкладку и focus. У ScrollViewer Up/Down, PageUp/PageDown и Home/End управляют
scroll offset. Wheel delta имеет смысл желаемого смещения в logical pixels:
положительный Y прокручивает вниз. `set_scroll_extent` меняет высоту content;
при resize и уменьшении extent offset ограничивается заново.

### Меню, DropDownButton и SplitButton

`popup_menu(anchor, &[MenuItem])` сохраняет плоский профиль.
`popup_menu_entries(anchor, &[MenuEntry])` принимает листья `Item(MenuItem)`,
`Submenu { label, items, enabled }` и `Separator`. Лист связан с CommandRegistry;
разделитель не получает focus, подменю раскрывает следующий уровень без вызова
команды. `MenuItem::with_enabled(false)` отключает отдельное размещение.
Доступность команды и её checked продолжают читаться из registry.

Меню размещаются в overlay внутри окна. Корневое меню открывается ниже anchor,
либо выше, если снизу недостаточно места и сверху его больше; вложенное — справа либо слева.
Bounds ограничиваются viewport, длинные меню прокручиваются. Up/Down/Home/End
выбирают доступные пункты, Right раскрывает подменю, Left возвращает к его
владельцу. Enter/Space выполняют пункт или раскрывают подменю. Escape закрывает
один уровень, Tab закрывает всю цепочку и продолжает обход формы. Успешная
команда закрывает цепочку с возвратом focus. При внешнем pointer down меню
закрывается, после чего этот ввод обрабатывается обычным control. Blur окна,
удаление, скрытие или отключение владельца закрывают меню без принудительного
возврата focus. Возврат к anchor допустим только пока он доступен для focus.

Меню проверяется перед заменой существующего: не более восьми уровней,
256 пунктов вместе с разделителями и 1 MiB UTF-8 подписей/идентификаторов.
Пустое меню недопустимо. Это статическое меню внутри окна; отдельные native
popup windows, произвольные XAML templates и привязки коллекций не реализованы.

`dropdown_button(parent, label, items, style)` создаёт кнопку с общей системой
ButtonStyle и иконками; её активация открывает меню. `split_button` возвращает
`SplitButton { container, primary, dropdown }`. Основная часть активирует
команду, стрелка открывает меню. Запрет основной команды не запрещает стрелку;
отключение контейнера запрещает обе части. Каждая часть — отдельная позиция Tab.
Общее оформление частей задаётся через `set_button_style`, иконка основной
части — через `set_button_icon(primary, ...)`.
Основная часть также предоставляет semantic Expand/Collapse для своего меню;
её Activate продолжает выполнять основную команду.

Семантика владельца меню содержит `has_popup`, `expanded`, действия Expand и
Collapse. Примеры проецируют их в AccessKit; toggle-пункты имеют роль
MenuItemCheckBox. Это контракт дерева accessibility, а не результат проверки
конкретного screen reader.

### ToolTip

`set_tooltip(id, text)` задаёт подсказку и возвращает Result. Максимальная длина
4096 UTF-8 байт; пустой или пробельный текст очищает подсказку. При непрерывном
hover над доступным владельцем в течение 500 мс появляется измеренный текст в
отдельном слое Scene. Этот слой не получает focus, не участвует в hit testing и
не блокирует кнопку. Подсказка скрывается при вводе, уходе указателя, blur,
раскрытии меню и потере доступности владельца. Текст доступен через semantic
`tooltip` независимо от показа визуального слоя.

Host должен объединить `tooltip_deadline()` со своими таймерами и вызвать
`tooltip_tick(Instant::now())` при пробуждении; true требует redraw. Когда
deadline отсутствует, отдельный периодический таймер для подсказки не нужен.
`tooltip_bounds()` возвращает bounds нарисованной подсказки. Первый профиль
поддерживает обычный текст и hover; rich content и показ по keyboard focus
не предусмотрены.

Визуальный владелец определяется через hit-testable control и его предков.
У отдельных декоративных Label/Icon/Separator текст ToolTip представлен в
semantics; собственный визуальный hover для них пока не реализован. Если host
перехватывает ввод до `Controls::dispatch`, он должен вызвать `dismiss_tooltip()`
при таком вводе, как делают Ribbon/TreeGrid в галерее.

## Команды

`CommandId` — принадлежащее приложению строковое имя, не WidgetId. Один
`CommandRegistry` используется обычными buttons, Ribbon, QAT и popup items.
`register(id, label)` создаёт обычную команду, а
`register_toggle(id, label, checked)` — переключаемую. Повторная регистрация того
же ID заменяет запись, включая callback и predicate. `set_enabled` задаёт доступность,
`set_can_execute` подключает чистый predicate текущего состояния приложения.
`on_execute` задаёт callback, а `drain_invocations` позволяет обрабатывать
`CommandInvocation { id, checked }` через очередь событий приложения.

`activate(id)` — общий пользовательский путь: один раз проверяет enabled и
`can_execute`, инвертирует `Some(checked)`, вызывает callback и добавляет ровно
один invocation с итоговым checked. `None` обозначает обычное действие.
При отказе или неизвестном ID возвращает false без изменений checked, callback
и событий. Отказ активации control также не создаёт `drain_activated` event и
не закрывает его popup. Успешная активация menu item закрывает меню.

`execute(id)` сохранён как низкоуровневый вызов callback с тем же gate и одним
invocation, **без инверсии checked**. Для действия пользователя используйте
`activate`; ручной `set_checked` перед ним даст лишнее переключение.
`set_checked(id, Some(value))` программно задаёт toggle-режим, а `None` снимает его;
оба варианта не вызывают callback и не создают invocation.

Связанные Button, ToggleButton и MenuItem читают режим непосредственно из registry,
без fallback на старое локальное состояние. Обычный Button с toggle-командой
получает checked-оформление, роль ToggleButton и semantic Toggle; после `None`
снова становится обычной кнопкой. MenuItem сохраняет роль MenuItem и добавляет
отметку только для toggle-команды. Явный `set_role` остаётся приоритетным.
Несвязанные ToggleButton/CheckBox сохраняют собственное состояние; RadioButton
не привязан к CommandId и работает через свою группу.

Для совместимости builder `toggle(..., command, checked, ...)` и
`MenuItem::toggling()` при создании устанавливают checked, только если у команды
его ещё нет. Уже заданное Some не перезаписывается. Дальнейший `set_checked(None)`
не отменяется кадрами или сменой темы. Новое создание такого legacy размещения
может вновь задать toggle-режим; для обычной проекции существующей команды
используйте `button`/`MenuItem::new` и объявляйте режим через `register_toggle`.

`Controls::is_enabled(id)` учитывает текущую команду, собственную доступность,
видимость и всех предков; для отсутствующего/удалённого ID возвращает false.
`command_id(id)` читает связь живого размещения с командой; отсутствие связи
или удалённый ID дают `None`.
Hover, focus и доступность размещения не меняют глобальный checked команды.
Predicates должны быть чистыми: отрисовка и семантика могут опрашивать их несколько
раз, а активация выполняет окончательную проверку до изменения состояния.
Приложение запрашивает update при изменении внешнего состояния predicate.

## TextBox

`TextDocument` хранит committed text и selection в UTF-8 byte offsets,
проверяемых по extended grapheme boundaries. Backspace/Delete не разбивают
составные graphemes. Left/Right используют bidi-aware визуальную навигацию и
сохраняют caret affinity; Ctrl+Left/Right переходят по словам. Up/Down и
PageUp/PageDown используют shaped geometry. Home/End переходят к границам
hard line, Ctrl+Home/End — документа. Shift расширяет selection.

Здесь committed text означает текст после завершённых IME вставок. Для приложения
он остаётся редактируемым черновиком до подтверждения через `commit_edit` или
настроенный переход фокуса. Принятое значение хранится отдельно в `accepted_value`.

`edit_box` использует тот же документ для одной строки. Начальное значение,
`text_input`, `set_text` и IME commit удаляют CR, LF, tab, NEL (`U+0085`),
line separator (`U+2028`) и paragraph separator (`U+2029`). Пустой или состоящий
только из этих разделителей пользовательский ввод сохраняет selection и текст.
Enter не вставляет новую строку. Preedit с разделителями отменяется целиком:
фильтрация изменила бы переданные IME смещения cursor.

`set_read_only(id, true)` запрещает пользовательское изменение, сохраняя focus,
navigation, selection и Copy. Typing, Cut, Paste, undo/redo, IME и accessibility
SetValue не меняют read-only редактор. Переход в read-only отменяет preedit.
`is_read_only(id)` возвращает `Option<bool>`, а `is_text_editable(id)` дополнительно
учитывает disabled state редактора и ancestors. Программный `set_text` разрешён
для read-only и disabled редакторов и проходит проверку ограничений. Он изменяет
черновик с поддержкой undo; для загрузки принятого значения есть `set_editor_value`.

`set_max_length(id, Some(n))` ограничивает committed text по числу extended
graphemes в итоговой строке, включая слияния на границах вставки. `None` снимает
этот лимит; `Some(0)` разрешает только пустую строку. Новый лимит должен допускать
и черновик, и принятое значение, чтобы отмена редактирования могла его восстановить.
Иначе возвращается ошибка, сохраняющая текст и прежний лимит. `max_length(id)` возвращает
`Option<Option<usize>>`: внешний `None` означает, что ID не является редактором.
Существующая undo/redo history сохраняется, но восстановление значения, которое
нарушает текущий лимит, отклоняется.

Каждый редактор также ограничен 1 MiB UTF-8, включая отображаемый IME preedit.
Пользовательская вставка сверх ограничений отклоняется целиком без обрезания;
`set_text` и создание слишком большого редактора возвращают ошибку. Отказ
сохраняет документ, selection и history. Preedit проверяется только по байтам;
предел graphemes применяется при commit. После IME commit композиция заканчивается
даже при отклонённом результате. Это лимит одного документа: отдельные ограничения
shaping, glyph count и общего объёма текста сцены renderer продолжают действовать.
Контракт низкоуровневых операций описан в [документации текста](text.md#bounded-editing).

После обработки ввода и программных обновлений вызывайте `drain_text_changes()`.
Он возвращает последовательность `TextChanged { id, text }` с новым committed
значением для каждого фактического изменения, включая undo/redo и `set_text`.
Создание редактора, selection, preedit, повторное присваивание того же значения
и отклонённые операции событий не создают. После чтения очередь очищается;
события удалённых controls из неё исключаются.

Ctrl/Cmd+A выбирает всё; C/X/V создают `ClipboardRequest::Copy` или `Paste`.
Host выполняет системную clipboard операцию и передаёт результат вставки через
`text_input`. Ctrl/Cmd+Z, Shift+Z и Y управляют undo/redo. Ctrl+Alt трактуется
как возможный AltGr, а не shortcut Ctrl. Системные clipboard errors остаются
ответственностью host; Cut можно восстановить через Undo.

IME preedit хранится отдельно от committed text, отображается и имеет caret
из preedit cursor. Raw editing keys не меняют документ во время композиции.
Commit является одной undoable операцией. Preedit подчёркивается по shaped
geometry; отсутствующий IME cursor скрывает caret. Смена focus отменяет preedit,
нажатие мыши отменяет его перед hit test committed текста. `caret_bounds`
сохраняет bidi affinity и возвращает logical client rectangle
для нативного IME candidate window. Парные native keyboard/IME callbacks требуют
проверки реального input method на каждой платформе.

Shaping, hit testing, выделение и caret используют один контракт `PreparedText`;
ширина grapheme не оценивается по числу символов. Сцена TextBox обрезается по
control и ancestor clips. Вертикальный offset удерживает caret в видимой части
multiline редактора. Rich text, password masking и spellcheck не входят в текущий
TextBox.

## Подтверждение значения и валидация

Начальный нормализованный текст становится принятым значением без проверки
business validation. `accepted_value(id)` возвращает его, а `is_edit_dirty(id)`
сравнивает текущий черновик с ним. Preedit хранится отдельно и сам по себе не меняет
эти значения. Установка новых правил не проверяет существующий текст автоматически.

`set_editor_validation(id, EditorValidation { is_required, message })` задаёт
проверку обязательного значения. `is_required` отвергает строку, для которой
`trim().is_empty()`, но не обрезает пробелы принятого текста. `message` — сообщение
ошибки; по умолчанию используется английский текст. Дополнительный синхронный
предикат подключается через `set_editor_validator(id, Some(EditorValidator::new(
|text| -> Result<(), String> { ... })))`; `None` снимает его. Callback должен быстро
вернуть результат и не запускать вложенный цикл обработки UI. Асинхронная
валидация и проверка всей формы в этот контракт не входят.

`EditorValidation.message` ограничено 4096 байтами UTF-8; слишком большое
сообщение отклоняется до замены правил. Пустое или пробельное сообщение использует
`A value is required.`. Ошибка callback ограничивается 4096 байтами по UTF-8
границе; пустое сообщение заменяется на `The value is invalid.`.

`commit_edit(id)` возвращает `Result<CommitOutcome, ControlError>`:

| Результат | Состояние |
|---|---|
| `Committed` | Проверки пройдены; черновик становится новым принятым значением |
| `Unchanged` | Проверки пройдены; принятое значение уже совпадает с черновиком |
| `Invalid(message)` | Проверка не пройдена; черновик, selection, принятое значение и undo/redo сохранены |
| `CompositionActive` | Активный IME preedit сохранён; подтверждение и business validation отложены |

Успешное подтверждение сохраняет историю документа. Поэтому undo после commit
может снова сделать черновик отличающимся от принятого значения. Ошибка проверки
доступна через `validation_error(id)`; каждый явный неуспешный commit публикует
событие. Изменение самого черновика очищает прежнюю ошибку. Навигация, preedit,
повторная установка той же строки и вставки, отклонённые по MaxLength/byte limit,
её не снимают. Замена правил или callback очищает прежнюю ошибку без проверки
значения и без событий подтверждения.

`cancel_edit(id)` восстанавливает принятое значение, переносит caret в конец,
отменяет preedit, снимает ошибку и полностью очищает undo/redo. Он возвращает
`true`, если были черновик, composition, ошибка или история для сброса. История
очищается и тогда, когда undo уже вернул текст к принятому значению. Повторная
отмена без таких изменений возвращает `false`.
Явные вызовы `commit_edit` и `cancel_edit` доступны приложению также для read-only
и disabled редакторов; ограничения пользовательских клавиш описаны ниже.

`set_editor_value(id, text)` предназначен для загрузки значения приложением.
Он сначала нормализует строку и проверяет ограничения, required и callback;
при успехе устанавливает черновик и принятое значение, отменяет preedit,
сбрасывает ошибку и историю, переносит caret в конец. Метод разрешён для read-only
и disabled редакторов. `Invalid(message)` полностью сохраняет прежнее состояние,
включая уже показанную ошибку, preedit и очереди событий. Нарушение byte/grapheme
лимита возвращает `ControlError::Edit`, также без изменений.

После обработки ввода приложение читает `drain_edit_events()`:

- `Committed { id, value }` — принятое значение фактически изменилось;
- `Canceled { id }` — отмена сбросила состояние редактирования;
- `ValidationFailed { id, message }` — попытка commit завершилась ошибкой проверки.

`TextChanged` отдельно сообщает только о смене черновика. Commit не создаёт его;
cancel/load создают его только при изменении текста. Unchanged commit и загрузка
того же принятого значения не создают `Committed`. `Controls::remove` исключает
события редактора из обеих очередей; WidgetId сохраняет проверку поколения.
Если узел удалён непосредственно через runtime, API сессии отвергает старый ID,
а `drain_edit_events` исключает его уже накопленные события.

Enter без модификаторов подтверждает однострочный EditBox; Ctrl+Enter или
Cmd+Enter подтверждает multiline TextBox. Обычный Enter в multiline вставляет
перевод строки. Автоповтор не повторяет подтверждение. Escape без модификаторов
сначала закрывает popup, затем отменяет активную IME composition, и только при
следующем нажатии отменяет черновик. Автоповтор Escape не откатывает значение.
Пользовательские commit/cancel отключены у read-only и disabled редакторов.

`EditorSettings::commit_on_focus_lost` по умолчанию равен `true`. Переход через
Tab, указатель, semantic Focus, наблюдаемый `Mutation::Focus` или потеря фокуса окна
пытается подтвердить значение один раз. Ошибка сохраняет черновик, не удерживая
фокус. При активной IME composition переход отменяет preedit и пропускает commit;
disabled, hidden и read-only редакторы также не подтверждаются этим путём.
Приложение может установить `false` и вызывать commit/cancel явно. После прямого
`Mutation::Focus` через открытый runtime вызывайте `update`, чтобы Controls
обработал переход. Несколько прямых перемещений между вызовами Controls
не восстанавливаются как отдельные переходы. Программный `set_text` сам по себе
не синхронизирует такие изменения фокуса и не подтверждает другие поля.

## Настройки и оформление редактора

`EditorSettings { is_read_only, max_length, placeholder, commit_on_focus_lost }` можно повторно
использовать для нескольких редакторов. `set_editor_settings(id, settings)`
сначала проверяет черновик и принятое значение по новому лимиту и размер placeholder
(не более 1 MiB UTF-8), затем применяет весь набор. При ошибке прежние настройки,
текст, composition, selection и undo/redo остаются прежними. Успешный переход
в read-only отменяет preedit. Настройки не создают `TextChanged`.
`editor_settings(id)` возвращает текущий набор; прежние `set_read_only`,
`set_max_length` и `set_placeholder` используют тот же путь применения.

`set_edit_box_style(id, EditBoxStyle)` задаёт независимые от поведения overrides.
`EditBoxStyle::default()` снимает их, возвращая актуальную тему. `font_size`
принимает конечное значение `(0, 512]`; `padding`, `corner_radius` и `border_width`
— конечные значения `[0, 16384]` в логических пикселях. Padding задаёт отступы
от внешних границ контрола; чрезмерные отступы скрывают текст, не создавая
отрицательной геометрии. Радиус и толщина рамки ограничиваются фактическим размером
поля. Нулевые радиус, рамка и padding допустимы. На не-Fluent теме фокус выделяет
рамку accent-цветом и толщиной не менее 2 px, сохраняя более толстую заданную рамку.

`normal`, `hovered`, `focused`, `read_only`, `disabled`, `invalid` имеют тип
`EditBoxAppearance`: необязательные background, foreground, placeholder, border,
accent и selection. Цвета — straight-alpha sRGB RGBA, каждый канал конечный `[0, 1]`.
Accent используется для индикации фокуса, каретки и IME underline. Скруглённая рамка
рисуется отдельным кольцом поверх полного фона: её alpha смешивается с фоном,
но цвет рамки не заполняет прозрачную центральную часть поля.

Палитра вычисляется последовательно: обычная тема → `normal` overrides →
подстановки темы для состояния → overrides этого состояния. Hover заменяет фон
на theme hover; read-only — на canvas; disabled — фон на canvas, текст и placeholder
на muted. Чтобы переопределить эти цвета, задайте соответствующий state appearance.
Остальные явно заданные цвета сохраняются при переключении состояния и темы.

`edit_box_visual_state(id)` возвращает Normal, Hovered, Focused, ReadOnly,
ReadOnlyFocused или Disabled. Disabled имеет высший приоритет и учитывает ancestors.
Readonly подавляет hover; ReadOnlyFocused использует палитру `read_only` с видимой
индикацией фокуса, без применения `focused`. У редактируемого поля Focused имеет
приоритет над Hovered. Отключённое поле не рисует каретку или выделение.

Ошибка валидации добавляет поверх активного состояния оформление `invalid`.
По умолчанию рамка и accent используют `ThemeTokens::error`; `invalid` может
переопределить те же шесть цветов. Это не отдельное значение `EditBoxVisualState`:
focus, hover и read-only продолжают определять поведение. Disabled сохраняет
свою палитру, а сообщение ошибки остаётся доступным в семантике.

Стиль не пересоздаёт control/document, не сбрасывает историю, selection или
composition и не создаёт `TextChanged`. Шрифт и padding применяются к общему
PreparedText для рисования, прокрутки, hit-testing и каретки. Изменение стиля
обновляет сцену, не меняя внешние размеры LayoutStyle; высоту поля задаёт приложение.
Размеры контента не гарантируют, что любой текст поместится в эту высоту.
Общие renderer/text budgets остаются в силе. Маски, асинхронная валидация,
общая транзакция формы и интеграция accepted value с TreeGrid остаются отдельными
возможностями.

## Темы и семантика

Light, Dark, Compact, FluentLight и FluentDark задают собственные `ThemeTokens`:
цвета surface/text, hover/pressed/selected/focus, spacing, font size и высоту
controls. `set_theme`
меняет оформление без пересоздания control IDs и checked/document state.
Compact уменьшает default heights; явные размеры приложения сохраняются.

`semantics()` возвращает независимые `SemanticNode`: role, label, value,
checked/selected/disabled/read_only/focused, validation_error, bounds, children
и доступные действия. `value` содержит черновик, а `validation_error` — текущее
сообщение проверки.
Структурные узлы включены как Panel; единственный корень имеет `parent: None`,
а hidden pages исключены вместе с descendants.
`perform_action` выполняет Focus/Activate/Toggle/SetValue/Scroll с проверкой
доступности. Read-only редактор сохраняет Focus, но не предоставляет SetValue;
прямой вызов этого действия также не изменяет его значение. Платформенный адаптер
связывает эти данные с accessibility API.
Наличие semantic nodes не заменяет проверки screen reader и native focus.

`set_custom_scene(id, scene)` вставляет собственную отрисовку приложения на
уровне соответствующего retained node. Координаты global logical, commands
дополнительно обрезаются ancestor clip. Это позволяет таблице находиться ниже
popup backgrounds и текста, сохраняя общий painter order.

Автоматические тесты проверяют activation/cancellation, disabled state,
themes/identity, tabs, scroll, menus, clipboard requests, Unicode/IME document
operations и painter order. Фактические native/GPU результаты учитываются
отдельно в отчётах проекта.
