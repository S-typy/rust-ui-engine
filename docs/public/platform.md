# Native host и DesktopApp

`rust-desktop-ui-platform-winit` запускает приложение в нативном GPU-окне и
обслуживает window lifecycle, ввод, IME, clipboard и AccessKit. Приложение
реализует `DesktopApp`, владеет своим состоянием и возвращает `Scene`.
Window/device handles не требуются в моделях core, controls и TreeGrid.

## Минимальное приложение с текстом

Для отдельного бинарного crate рядом с исходным workspace:

```toml
[dependencies]
rust-desktop-ui-core = { path = "../rust-ui-engine/crates/ui-core" }
rust-desktop-ui-platform-winit = { path = "../rust-ui-engine/crates/ui-platform-winit" }
```

Пример компилируется как обычный `src/main.rs`; запуск требует desktop session,
hardware GPU и системных шрифтов.

```rust,no_run
use rust_desktop_ui_core::{Color, Rect, Scene, Size, TextRun};
use rust_desktop_ui_platform_winit::{
    DesktopApp, EventResponse, PlatformEvent, RunOptions, run,
};

#[derive(Default)]
struct Hello {
    scene: Scene,
}

impl DesktopApp for Hello {
    fn update(&mut self, viewport: Size, _scale: f32) -> Result<(), String> {
        self.scene.clear();
        self.scene.fill(
            Rect::new(0.0, 0.0, viewport.width, viewport.height),
            Color::rgb(245, 247, 250),
        );
        self.scene.text(TextRun::new(
            "Привет, Rust UI Engine!",
            Rect::new(16.0, 16.0, (viewport.width - 32.0).max(0.0), 40.0),
            Color::rgb(25, 35, 50),
            20.0,
        ));
        Ok(())
    }
    fn scene(&self) -> &Scene { &self.scene }
    fn event(&mut self, _event: PlatformEvent) -> Result<EventResponse, String> {
        Ok(EventResponse::default())
    }
    fn title(&self) -> String { "Text example".into() }
}

fn main() -> Result<(), String> {
    run(Hello::default(), RunOptions::default())
}
```

Host вызывает `update` перед кадром и передаёт logical viewport и scale factor.
Scene уже использует logical pixels: повторно умножать её координаты на DPI
не нужно. `EventResponse.redraw` запрашивает обновление; `wake_after` возвращает
таймер только на время caret animation или активной фоновой работы. При `None`
host ожидает внешние события. Длительные вычисления нужно выполнять вне event loop.
Ошибки приложения и GPU возвращаются явно; hidden CPU renderer отсутствует.

## Текстовый ввод и идентичность редактора

`PlatformEvent::Input` содержит logical keys, pointer и wheel. `Text(String)`
передаёт печатаемые символы клавиатуры, а `ImePreedit`, `ImeCommit` и `ImeCancel`
образуют отдельный путь композиции. Не превращайте `Key::Character` дополнительно
в committed text: это может удвоить ввод. Ctrl+Alt допускает печатаемый AltGr
input, Ctrl/Cmd shortcuts обрабатываются как команды.

Редактируемое приложение предоставляет обе части IME-контракта:

- `ime_target() -> Option<WidgetId>` — стабильный ID текущего TextBox. Он должен
  изменяться при переходе в другое поле; для нетекстового focus возвращается `None`.
- `ime_cursor() -> Option<Rect>` — конечная caret area в logical client pixels.
  Она относится к текущему полю, а не к видимости мигающего caret. При отсутствии
  активного редактора возвращается `None`.
- `ime_composition_active() -> Option<bool>` — состояние composition в документе.
  Возвращайте `Some(true/false)`, чтобы host заметил отмену preedit внутри того же
  редактора, например после перемещения caret мышью. `None` оставляет это состояние
  неизвестным host и не обеспечивает автоматический сброс для такой отмены.

При смене target или разрешённости ввода host очищает приложение через
`ImeCancel`, отключает/включает native IME и не принимает preedit/commit до
`Ime::Enabled`. Window blur отменяет composition. Приложение должно также
очищать pending composition у поля, которое теряет focus, и не подтверждать
cell editor по Enter, пока его TextDocument содержит preedit.
Если host считает composition активной, а приложение возвращает `Some(false)`,
native session сбрасывается даже при прежнем WidgetId. Это предотвращает
применение commit старой composition к новому caret до очередного Enabled.

`Enabled` означает начало доступной IME-сессии, а не синхронное подтверждение
`set_ime_allowed(true)`: Windows и macOS могут прислать его только при начале
композиции. Обычный keyboard text продолжает обрабатываться отдельно.
Нативные IME events не содержат WidgetId или session token; полная гарантия
порядка delayed OS events требует платформенной проверки. Известный контракт
winit описан в [Ime](https://docs.rs/winit/0.30.13/winit/event/enum.Ime.html) и
[set_ime_allowed](https://docs.rs/winit/0.30.13/winit/window/struct.Window.html#method.set_ime_allowed).

Preedit cursor использует UTF-8 byte offsets внутри временной строки. `None`
скрывает визуальный caret, но не означает отмену композиции. Empty preedit
очищает временное отображение; winit отправляет его перед commit. App должен
использовать document composition API, не вставлять каждое preedit обновление
в committed text. Геометрия candidate window обновляется вместе с кадром/DPI.

## Clipboard и accessibility

`EventResponse.clipboard` принимает `Copy(String)` и `Paste`. Host сохраняет
текст через arboard; успешное чтение возвращает `PlatformEvent::Paste(String)`.
Недоступный clipboard диагностируется в stderr без фиктивного paste. Рекурсивная
цепочка clipboard requests ограничена. Clipboard payload не является rich text
или изображением; Wayland доступ зависит от compositor protocols.

`accessibility(scale)` возвращает **полное связное** AccessKit tree со стабильными
IDs и актуальным focus. Bounds выражаются в physical client pixels. Host сравнивает
nodes с предыдущим снимком и отправляет только изменённые; после активации
accessibility cache сбрасывается. Удаление узла отражается в children его parent.
Подача только локального delta вместо полного дерева нарушает этот контракт.

`PlatformEvent::Accessibility(ActionRequest)` передаётся приложению для проверки
и выполнения. Controls предоставляет независимые `SemanticNode`, TreeGrid —
виртуальные row/cell данные; app связывает их с AccessKit IDs. Реализация trait
по умолчанию описывает только Window, поэтому пример выше не является образцом
полного accessibility tree текстового приложения.

## Пределы проверки

Host учитывает resize, нулевой размер, occlusion, DPI и ограниченное GPU recovery.
Состояние app сохраняется при пересоздании renderer. `RunOptions::smoke_test`
проверяет кадры и native resize с ограниченным временем выполнения; это не
тест реального IME, clipboard, screen reader или hardware device loss.

Текущие результаты и ручной протокол — в [runtime matrix](runtime-matrix.md).
Native CJK candidate window, focus changes во время composition и accessibility
на каждой ОС остаются отдельными критериями. Контракты alpha 0.1.0 описаны;
совместимость до 1.0 не гарантируется. Интегрированный пример — `controls-gallery`.
