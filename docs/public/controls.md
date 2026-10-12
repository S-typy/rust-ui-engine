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
необходимости запускать постоянный таймер для остальных controls.

## Компоненты и состояния

| Builder | Поведение |
|---|---|
| `panel` | Контейнер с padding/layout/clip; default layout — вертикальный Stack |
| `label` | Текст, не получающий focus и pointer hit |
| `button` | Primary press захватывает указатель; команда выполняется при release внутри того же control |
| `toggle` | Button с переключаемым checked state, общим с CommandID |
| `checkbox` | Подпись, отдельный квадрат-индикатор и checked state |
| `text_box` | Unicode документ, caret, selection, multiline wrapping, preedit и undo/redo |
| `edit_box` | Однострочный TextBox с горизонтальной прокруткой caret и placeholder |
| `scroll_view` | Viewport с clipped content, ограниченной прокруткой и keyboard navigation |
| `tab_control` | Tab strip и overlay страниц; неактивные страницы не рисуются и не участвуют в focus |
| `popup_menu` | Overlay menu, keyboard navigation, отключённые и toggle items, возврат focus к anchor |

Label контейнера Panel/ScrollViewer используется для accessibility; его
визуальное содержимое задаётся children или custom scene.

`measure_text(text, width, wrap)` возвращает размер содержимого в logical
pixels по тем же системным шрифтам, теме и правилам shaping, что и renderer.
Ширина задаётся без padding; действует лимит 1 MiB UTF-8 и ограничения
TextEngine. Явные размеры controls автоматически не увеличиваются: Label
высотой 20 px может обрезать строку более высокого системного шрифта.
Композиции могут использовать измерение для собственных размеров. Padding
кнопок по вертикали занимает только место, оставшееся после измеренной строки.

Pointer down у Button ещё не выполняет команду. Увод указателя и release снаружи
отменяют активацию; blur также отменяет pressed/capture state. Enter активирует
сфокусированный Button однократно на keydown. Space показывает pressed state и
активирует на keyup, если focus остался на том же control. Disabled controls и
disabled ancestors блокируют pointer, keyboard и semantic activation.

Tab/Shift+Tab обходят доступные controls. У TabControl Left/Right переключают
вкладку и focus. У ScrollViewer Up/Down, PageUp/PageDown и Home/End управляют
scroll offset. Wheel delta имеет смысл желаемого смещения в logical pixels:
положительный Y прокручивает вниз. `set_scroll_extent` меняет высоту content;
при resize и уменьшении extent offset ограничивается заново.

PopupMenu появляется в верхнем overlay-слое и ограничивается размером viewport.
Длинный список прокручивается. Up/Down/Home/End перемещают focus между enabled
items и прокручивают выбранный item в видимую область. Escape, Tab, внешнее
нажатие или выполненная команда закрывают popup; anchor получает focus, если
по-прежнему существует и доступен.

## Команды

`CommandId` — принадлежащее приложению строковое имя, не WidgetId. Один
`CommandRegistry` используется обычными buttons, Ribbon, QAT и popup items.
`register(id, label)` создаёт запись; `set_enabled` задаёт доступность,
`set_can_execute` подключает чистый predicate текущего состояния приложения.
`on_execute` задаёт callback, а `drain_invocations` позволяет обрабатывать
`CommandInvocation { id, checked }` через очередь событий приложения.

Все пути выполняют `can_execute` перед вызовом. Отсутствующая или отключённая
команда не вызывает callback и не попадает в очередь. `set_checked` позволяет
синхронизировать toggle presentation одного CommandID в нескольких местах.
Predicates должны не иметь побочных эффектов; приложение запрашивает update
при изменении внешнего состояния, влияющего на доступность.

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
