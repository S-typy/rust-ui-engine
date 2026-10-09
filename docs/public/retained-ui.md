# Retained UI: контракт M1

M1 реализует дерево узлов, события, focus, pointer capture, invalidation,
вычисление layout и построение сцены прямоугольников. `NodeProps` описывает
примитив интерфейса; он ещё не является готовым Button, Ribbon или TreeGrid.
Результаты проверок окна и GPU учитываются отдельно в [отчёте](M1_TEST_REPORT.md).
Архитектурное решение — [ADR-003](adr/ADR-003-retained-runtime.md).

## Пакеты и владение

| Пакет | Ответственность |
|---|---|
| `rust-desktop-ui-core` | `WidgetId`, `UiTree`, `UiRuntime`, события, собственные layout-типы и `Scene`; внешних зависимостей нет |
| `rust-desktop-ui-layout` | `TaffyLayout`, действующий адаптер Stack, Overlay и Flex; Taffy-типы остаются внутри пакета |
| `rust-desktop-ui-render-wgpu` | Читает `Scene` в логических пикселях и рисует через GPU |
| `gpu-shell` | Нативное окно, нормализация ввода и состояние демонстрации |

`UiRuntime` владеет деревом, handlers, focus/capture, последним layout и
кэшированной сценой. После передачи `UiTree` в runtime изменения выполняются
через `insert` и `apply`; `tree()` выдаёт только общую ссылку. Оконные и GPU-типы
в API ядра не входят. Runtime и callbacks предназначены для потока event loop.

`WidgetId` — непрозрачный ID с идентичностью дерева, индексом слота и поколением.
Он сохраняется при reparent и меняется при повторном использовании удалённого
слота. Чужие и устаревшие ID отвергаются. При переполнении поколения слот
навсегда выводится из повторного использования. Исчерпание пространства
идентичностей деревьев завершает создание дерева с panic до повторения ID.

Корень постоянен. Его нельзя удалить или переместить. `remove` удаляет поддерево
и возвращает прежние ID в preorder. `reparent` запрещает циклы и добавляет узел
в конец списка детей нового родителя; внутри прежнего родителя он меняет порядок
тем же способом. Ошибки валидации не меняют дерево. Структурные поля недоступны
для прямой записи.

## Минимальный пример без окна

Создайте обычный бинарный crate рядом с каталогом `rust-ui-engine` и добавьте
в его `Cargo.toml` следующие зависимости. Пример использует исходники workspace;
публикация этих пакетов в registry для его запуска не требуется.

```toml
[dependencies]
rust-desktop-ui-core = { path = "../rust-ui-engine/crates/ui-core" }
rust-desktop-ui-layout = { path = "../rust-ui-engine/crates/ui-layout" }
```

Поместите следующий код в `src/main.rs` и выполните `cargo run`:

```rust
use rust_desktop_ui_core::*;
use rust_desktop_ui_layout::TaffyLayout;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut tree = UiTree::new();
    let root = tree.root();
    tree.set_style(root, LayoutStyle {
        kind: LayoutKind::Overlay,
        ..LayoutStyle::default()
    })?;
    let tile = tree.insert(root, NodeProps {
        style: LayoutStyle {
            width: Length::Px(100.0),
            height: Length::Px(40.0),
            offset: Point::new(16.0, 20.0),
            ..LayoutStyle::default()
        },
        paint: Paint {
            background: Some(Color::rgb(50, 80, 120)),
            hover_background: Some(Color::rgb(70, 100, 150)),
            focus_border: Some(Border {
                width: 2.0,
                color: Color::rgb(240, 200, 80),
            }),
            ..Paint::default()
        },
        focusable: true,
        ..NodeProps::default()
    })?;

    let mut ui = UiRuntime::new(tree);
    ui.on(tile, move |event, context| {
        if event.phase == EventPhase::Target
            && matches!(event.input, InputEvent::PointerDown {
                button: PointerButton::Primary, ..
            })
        {
            context.enqueue(Mutation::CapturePointer(Some(tile)));
        }
    })?;
    let mut layout = TaffyLayout::new();
    let viewport = Size::new(320.0, 200.0);
    assert_eq!(ui.update(&mut layout, viewport)?.layout_passes, 1);

    let report = ui.dispatch(InputEvent::PointerDown {
        position: Point::new(20.0, 24.0),
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    });
    assert!(report.errors.is_empty());
    assert_eq!(report.target, Some(tile));
    assert_eq!(ui.focused(), Some(tile));
    assert_eq!(ui.captured(), Some(tile));
    assert_eq!(ui.update(&mut layout, viewport)?.layout_passes, 0);

    ui.dispatch(InputEvent::PointerUp {
        position: Point::new(300.0, 180.0),
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    });
    assert_eq!(ui.captured(), None);
    ui.update(&mut layout, viewport)?;
    assert_eq!(ui.update(&mut layout, viewport)?, FrameStats::default());
    println!("{} rectangles", ui.scene().rectangles.len());
    Ok(())
}
```

Пример проверяет вычисления и routing на CPU. Он не открывает окно и не проверяет
GPU renderer. В приложении полученная `Scene` передаётся renderer, а результат
`DispatchReport.errors` обрабатывается как ошибка операции.

## Layout и координаты

`LayoutEngine::compute(&UiTree, Size)` возвращает `LayoutSnapshot`: border boxes
в локальных координатах родителя и счётчики измеренных/размещённых узлов.
Для каждого живого узла требуется конечный прямоугольник с неотрицательными
размерами. Корень всегда занимает `(0, 0, viewport.width, viewport.height)`;
его собственные width/height/min/max не переопределяют viewport.

Все координаты core — логические пиксели. Host переводит физические pointer
positions и pixel wheel delta с учётом DPI. `Wheel.delta` выражается в логических
пикселях; выбор коэффициента для событий прокрутки в строках принадлежит host.
Renderer применяет scale factor только на границе GPU. Дробные layout-координаты
не округляются.

| Layout kind | Контракт |
|---|---|
| `Leaf` | Не содержит детей; адаптер возвращает ошибку для непустого Leaf |
| `Stack(Axis)` | Одна линия, padding и gap, без grow/shrink распределения основного размера детей |
| `Flex(Axis)` | Одна линия Taffy Flex с grow/shrink, min/max, align и justify |
| `Overlay` | Дети независимо размещаются в общей внутренней области; `offset` применяется к каждому ребёнку после arrangement |

`Length::Percent(0.5)` означает 50% доступного внутреннего размера родителя.
`Auto` в Overlay заполняет доступную область с учётом min/max. Overlay требует
явного или назначенного родителем размера: дети не формируют его intrinsic size.
`offset` используется только у ребёнка Overlay. `gap` и flex-коэффициенты должны
быть конечными и неотрицательными; положительная бесконечность допустима только
для неограниченного `max_size`. Ограничения и цвета валидируются при изменении
свойств. Нулевой viewport разрешён.

`TaffyLayout` пересоздаёт внутреннее дерево при каждом layout pass и поддерживает
глубину до 128 рёбер от корня. Более глубокое дерево возвращает ошибку перед
запуском рекурсивного backend. Само retained tree обходит и удаляет узлы
итеративно. Grid, flex-wrap, текстовые измерения и собственные intrinsic callbacks
в этот контракт пока не входят.

Глобальная позиция ребёнка равна глобальному origin родителя, минус `scroll`
родителя, плюс parent-local layout position, плюс собственный `translation`.
Translation не меняет layout. Scroll также не меняет layout и применяется только
к потомкам; диапазон прокрутки определяет приложение.

Корень всегда ограничивает вывод viewport. `clip = true` дополнительно обрезает
детей по border box узла. Все прямоугольники физически пересекаются с накопленным
clip перед включением в Scene. Только translation и прямоугольные clips
поддерживаются в M1; rotations, scale transforms и rounded clips отсутствуют.

Видимые siblings рисуются по возрастанию `z_index`; при равенстве сохраняется
порядок детей. Потомки остаются внутри порядка своего поддерева. Hit test идёт
в обратном порядке рисования и использует те же clips. Левая и верхняя границы
включены, правая и нижняя исключены.

`visible = false` сохраняет layout space, но исключает поддерево из paint, hit
test и focus. `enabled = false` сохраняет paint и layout, исключая поддерево
из hit test и focus. `hit_test = false` исключает только сам узел, а не его детей.
Непригодные узлы теряют focus и capture после применения mutation.

## События, focus и capture

`InputEvent` содержит pointer, wheel, key и window focus события.
`Key::Character(String)` представляет logical key; это не committed text,
Unicode editing или IME composition. Keyboard-события направляются focus-узлу,
а при его отсутствии — корню.

Runtime выбирает target и фиксирует маршрут. Затем он вызывает handlers предков
от корня к target в фазе `Capture`, handler target в фазе `Target` и handlers
предков в обратном порядке в фазе `Bubble`. У target нет дополнительных
Capture/Bubble вызовов. `on(id, handler)` устанавливает один handler узла;
повторный вызов заменяет прежний. `DispatchReport.deliveries` считает вызванные
handlers, а не все узлы маршрута.

`EventContext::stop_propagation()` останавливает последующие вызовы, сохраняя
default behavior. `prevent_default()` отменяет стандартные mouse focus и Tab.
После завершения маршрута runtime выполняет default actions, затем применяет
`enqueue(Mutation)` в FIFO-порядке. Поэтому явная callback-команда focus может
переопределить default focus. Ошибка одной команды попадает в `report.errors`;
следующие команды всё равно выполняются. Набор команд не является транзакцией.
Удаление или reparent внутри callback не меняет уже выбранный маршрут.

Primary pointer down фокусирует ближайший пригодный `focusable` узел среди target
и его предков. Tab/Shift+Tab циклически обходят пригодные узлы в логическом
preorder, независимо от `z_index`. Ctrl/Alt/Super+Tab не запускают стандартный
обход. В M1 нет отдельных focus scopes или пространственной навигации.

Pointer capture включается явно через `Mutation::CapturePointer(Some(id))`.
Он направляет pointer/wheel события захватившему узлу вне его hit bounds. Hover
продолжает соответствовать фактическому hit test. Primary pointer up всегда
освобождает capture, включая случаи `prevent_default` и повторного захвата
из обработчика этого up. Window blur очищает focus, capture и hover. Удаление
поддерева удаляет его handlers и очищает связанные состояния; устаревшие IDs
не получают последующие события.

Это логический capture внутри runtime. Доставка событий за границами окна
зависит от платформенного адаптера. Модель рассчитана на один основной pointer;
touch IDs, multi-pointer gestures и OS capture API в core отсутствуют.

## Invalidation, update и Scene

| Изменение | Invalidation |
|---|---|
| Структура или layout style | Measure/layout/paint; геометрические flags распространяются к предкам |
| `Paint` | Paint только изменённого узла |
| Visible/enabled/focusable/z/clip/scroll/translation | Paint и semantics; layout не вызывается |
| Hover/focus | Paint feedback; focus также отмечает semantics |

`MEASURE` подразумевает `LAYOUT` и `PAINT`; `LAYOUT` подразумевает `PAINT`.
Повторная запись идентичных props/style/paint сохраняет чистое состояние.
Текущий runtime пересчитывает всё layout tree при геометрической invalidation
и пересобирает Scene целиком при paint invalidation. Это ещё не incremental
layout или dirty-region rendering.

Вызывайте `update(&mut engine, viewport)` после изменений геометрии, включая
resize, перед следующим hit test или pointer dispatch. Новые узлы получают
визуальные bounds только после успешного layout. `bounds` и `hit_test` читают
последнюю разрешённую геометрию. Перед рисованием также нужен `update`, чтобы
включить focus/hover и callback mutations в Scene. `needs_update()` учитывает
pending изменения runtime, но не может обнаружить новый host viewport до вызова
`update`.

`update` возвращает delta `FrameStats`; `stats()` хранит суммарные счётчики.
`measured_nodes` и `arranged_nodes` берутся из layout adapter, `painted_nodes`
считает посещённые видимые узлы, включая узлы без фона или с пустым clip.
`semantics_nodes` считает invalidated узлы, а не вызовы screen reader backend.
При idle нет обхода дерева, вызова layout или пересборки Scene; delta равна нулю.
Кэш сцены сохраняется.

Background и border представлены заполненными прямоугольниками. Необязательные
`hover_background` и `focus_border` дают primitive feedback. Изменение scroll,
translation или layout пересчитывает hover для последней позиции pointer даже
без нового PointerMoved.

Ошибки layout и переполнение глобальных координат возвращаются явно. Предыдущая
Scene остаётся доступной, pending изменения не подтверждаются как успешный
кадр. Приложение должно обработать ошибку и исправить состояние или остановить
операцию. Неконечные координаты входного события отвергаются без routing.

M1 не реализует text shaping, GPU glyph rendering, IME, accessibility backend,
полноценные controls или виртуализацию больших наборов данных. Scene по-прежнему
содержит только rectangle batch; производительность будущих компонентов требует
отдельных измерений.
