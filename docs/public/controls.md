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
multiline редактора. Rich text, password masking, spellcheck и программируемые
editing transactions не входят в текущий TextBox.

## Темы и семантика

Light, Dark и Compact задают собственные `ThemeTokens`: цвета surface/text,
hover/pressed/selected/focus, spacing, font size и высоту controls. `set_theme`
меняет оформление без пересоздания control IDs и checked/document state.
Compact уменьшает default heights; явные размеры приложения сохраняются.

`semantics()` возвращает независимые `SemanticNode`: role, label, value,
checked/selected/disabled/focused, bounds, children и доступные действия.
Структурные узлы включены как Panel; единственный корень имеет `parent: None`,
а hidden pages исключены вместе с descendants.
`perform_action` выполняет Focus/Activate/Toggle/SetValue/Scroll с проверкой
доступности. Платформенный адаптер связывает эти данные с accessibility API.
Наличие semantic nodes не заменяет проверки screen reader и native focus.

`set_custom_scene(id, scene)` вставляет собственную отрисовку приложения на
уровне соответствующего retained node. Координаты global logical, commands
дополнительно обрезаются ancestor clip. Это позволяет таблице находиться ниже
popup backgrounds и текста, сохраняя общий painter order.

Автоматические тесты проверяют activation/cancellation, disabled state,
themes/identity, tabs, scroll, menus, clipboard requests, Unicode/IME document
operations и painter order. Фактические native/GPU результаты учитываются
отдельно в отчётах проекта.
