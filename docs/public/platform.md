# Native host и DesktopApp

`rust-desktop-ui-platform-winit` запускает приложение в нативном GPU-окне и
обслуживает window lifecycle, ввод, IME, clipboard и AccessKit. Приложение
реализует `DesktopApp`, владеет своим состоянием и возвращает `Scene`.
Window/device handles не требуются в моделях core, controls и TreeGrid.

## Уведомления и жизненный цикл окна

`DesktopApp::set_waker(Some(AppWaker))` получает handle текущего native окна.
Его можно клонировать в worker или callback коллекции. `wake()` ставит один
сигнал в очередь: повторные вызовы до обработки объединяются. На UI thread
host снимает pending-флаг, доставляет `PlatformEvent::Tick` и запрашивает
кадр. `update` принимает накопленные данные. Обновление во время обработки
сигнала может поставить следующий сигнал; периодический опрос не требуется.

Подписка на коллекцию должна только вызвать `wake`, не заимствовать изменяемый
источник и не обновлять controls внутри callback. `Controls::set_data_waker`
подписывает текущие data sources и GridControl; новые регистрации подключаются
автоматически, адаптер источника можно заменить. Для коллекций DataContext
используйте `BindingContext::subscribe_collection_wake` и храните возвращённый
guard вместе с формой. Несколько форм получают независимые уведомления.

При закрытии или suspend host передаёт `set_waker(None)` и деактивирует старый
handle. Очередь старого native окна не адресуется новому окну с тем же ключом
приложения или переиспользованным системным дескриптором: wake и accessibility
callbacks проверяют идентичность экземпляра окна перед доставкой. Уже начавшийся
одновременно с закрытием `wake` может завершиться,
но не доставляет работу закрытому view. `wake_after` предназначен для нужных
приложению таймеров; подключать его ради polling коллекций не нужно.

`DesktopApp::frame_presented(RendererStats)` вызывается только после успешного
GPU Present. Он не вызывается для skipped/occluded/error frames. Если этот
callback меняет состояние для следующего кадра, приложение вызывает waker.

`RunOptions::activate = false` показывает одно окно без первоначального
перехвата фокуса. Для нескольких окон это задаёт `WindowSpec::activate`.
Пример `xaml-form --no-activate` позволяет пассивный просмотр формы.

## Контракт дерева доступности

`DesktopApp::accessibility` возвращает полный снимок с `TreeInfo`, ROOT tree ID,
уникальными NodeId, связными children и существующим focus. Bounds передаются
в физических пикселях. Один живой контрол сохраняет ID при перестановке или
временном скрытии; удалённый контрол не отдаёт ID новому контролу.

`AccessibilitySnapshot` проверяет снимок до публикации: отсутствующие дети,
несколько родителей, циклы, посторонние узлы, некорректные bounds, ссылки на
отсутствующие описания и некорректная text selection отклоняются атомарно.
При ошибке host сохраняет последний принятый снимок и завершает запуск с
диагностикой. Для корректного снимка AccessKit получает только изменённые узлы.
После deactivation или запроса initial tree сравнение начинается заново.

Native action допускается только для существующего узла и объявленного
действия, с учётом hidden/disabled предков и read-only значений. Адаптер контрола
проверяет текущее состояние повторно: значение и видимость могли измениться
между публикацией и доставкой запроса. Изменение фокуса, имени или значения,
строки виртуального Grid и скрытые dock panes проверяются независимо от
отрисовки. Проверки дерева и чтение UI Automation не заменяют проверку
объявлений и навигации реальным экранным диктором.

Набор нативных UI Automation patterns зависит от платформенного адаптера.
В используемом `accesskit_windows` 0.35.1 DataGrid предоставляет SelectionPattern,
но не GridPattern/GridItemPattern; наличие виртуальных Row/Cell в AccessKit
не добавляет эти patterns. Подробнее — [доступность GridControl](grid-control.md#доступность-и-границы).

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

## Интегрированный заголовок Windows

`DesktopApp::title_bar() -> Option<TitleBar>` по умолчанию возвращает `None`.
`Some` до создания окна включает собственное содержимое заголовка в нативной
Windows-рамке. `TitleBar` задаёт высоту, Light/Dark и `drag_regions` в логических
клиентских координатах. В эти области нельзя включать интерактивные контролы
или открытые меню. После каждого layout host получает актуальные области.

Перед `update` вызывается `window_chrome_changed(WindowChrome)`: приложение
получает `custom_title_bar`, фактические `caption_buttons` и `resize_border_top`, `maximized` и
`focused`. Оно должно включать свою шапку только при `custom_title_bar=true`.
Если DWM недоступен либо на другой платформе, остаётся обычный заголовок.
Системные кнопки используют нативные hit-test и действия, не требуют обработчиков в CommandRegistry
и не дублируются в AccessKit tree приложения. Доступность настоящих кнопок
требует отдельной проверки с экранным диктором. Собственные векторные glyphs
и состояния host рисует поверх Scene: opaque GPU surface перекрывает обычное
DWM-изображение кнопок в расширенной клиентской области. Исходная Scene
приложения остаётся неизменной, декоративный слой не принимает отдельный ввод.

Host сохраняет resize borders и системные оконные команды. `HTCAPTION`
возвращается только для свободных областей шапки, DWM первым обрабатывает
системные кнопки. Drag, double-click maximize, Snap и оконное меню используют
системный маршрут; программный hit-test не заменяет проверку этих действий.
Подробности границы — [ADR-006](adr/ADR-006-native-ribbon-title-bar.md).

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

Host разрешает композицию только для поля со стабильным `ime_target` и корректной
caret area: конечные координаты, неотрицательная ширина и положительная высота.
Некорректная область или отсутствующий target отключают IME; такие bounds не
передаются системному API. Каждое окно хранит независимую IME session. При suspend
single-window host очищает focus/modifiers до уведомлений приложения; retained
модель приложения остаётся у владельца. Проверки этого протокола без окна не
заменяют реальную композицию, mixed-monitor DPI или native drag/Snap.

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
`RunOptions::smoke_device_recovery` вместе с `smoke_test` включает контролируемое
уничтожение GPU device после resize. Single/multi-window host использует обычную
ветку восстановления и требует новый успешный Present перед завершением.
Состояние приложения и исходные bitmap остаются у владельца Scene. Пример
`cargo run -p rust-desktop-ui-platform-winit --example recovery_smoke -- --multi`
проверяет два неактивируемых окна; без `--multi` — одно. Это проверка lifecycle,
а не имитация физического сбоя драйвера. Подробности — [отрисовка](rendering.md).
Состояние app сохраняется при пересоздании renderer. `RunOptions::smoke_test`
проверяет кадры и native resize с ограниченным временем выполнения; это не
тест реального IME, clipboard, screen reader или hardware device loss.

Текущие результаты и ручной протокол — в [runtime matrix](runtime-matrix.md).
Native CJK candidate window, focus changes во время composition и accessibility
на каждой ОС остаются отдельными критериями. Контракты alpha 0.1.0 описаны;
совместимость до 1.0 не гарантируется. Интегрированный пример — `controls-gallery`.
