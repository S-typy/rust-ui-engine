# Retained UI: дерево, события и сцена

Runtime реализует дерево узлов, события, focus, pointer capture, invalidation
и построение примитивной сцены. `NodeProps` описывает узел; поведение Button,
TextBox и других компонентов находится в [controls](controls.md), а
[TreeGrid](treegrid.md) строит виртуализированную часть Scene отдельно.
Исторические проверки retained runtime сохранены в [отчёте](retained-runtime-test-report.md); актуальные
платформенные проверки — в [матрице](runtime-matrix.md).
Базовое решение — [ADR-003](adr/ADR-003-retained-runtime.md), расширение текста
и компонентов — [ADR-004](adr/ADR-004-text-controls-platform-treegrid.md).

## Пакеты и владение

| Пакет | Ответственность |
|---|---|
| `rust-desktop-ui-core` | `WidgetId`, `UiTree`, `UiRuntime`, события, собственные layout-типы и `Scene`; внешних зависимостей нет |
| `rust-desktop-ui-layout` | `TaffyLayout`, адаптер Stack, Overlay, Flex и Grid; Taffy-типы остаются внутри пакета |
| `rust-desktop-ui-render-wgpu` | Читает `Scene` в логических пикселях и рисует через GPU |
| `gpu-shell` | Нативное окно, нормализация ввода и состояние демонстрации |

Новый `rust-desktop-ui-platform-winit` используется `controls-gallery` и
предоставляет общий host для текста, IME, clipboard и accessibility.

`UiRuntime` владеет деревом, handlers, focus/capture, последним layout и
кэшированной сценой. После передачи `UiTree` в runtime изменения выполняются
через `insert`, `instantiate`, `apply` и `apply_batch`; `tree()` выдаёт только
общую ссылку. Оконные и GPU-типы
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

## Повторно используемые поддеревья и lifecycle

`SubtreeTemplate::new(Vec<TemplateNode>)` валидирует структурное описание до
монтажа: один корень, родитель каждого следующего узла имеет меньший индекс,
свойства корректны, локальные имена непусты и уникальны. Описание содержит
собственные `NodeProps`, а не `WidgetId`, callback или бизнес-модель.
Это структурный Rust-контракт; XAML styles и шаблоны поведения сюда не входят.

`UiRuntime::instantiate(parent, &template)` атомарно создаёт экземпляр и
возвращает `MountedSubtree`. `root()`, `nodes()` и `named(name)` дают ID его
частей. Несколько экземпляров имеют независимые имена и разные ID.
Обработчики и состояние компонента связываются с полученными ID после монтажа.
`MountedSubtree` — проверяемый handle, не автоматический RAII-владелец:
его уничтожение не удаляет дерево, а `is_live(tree)` проверяет все исходные части.

| Операция | Состояние и идентичность |
|---|---|
| `Mutation::Detach(root)` | Отсоединяет поддерево; ID, props и handlers сохраняются |
| `Mutation::Reparent { node: root, parent }` | Присоединяет его в новом месте, сохраняя ID |
| `Mutation::Remove(root)` | Удаляет присоединённое или отсоединённое поддерево; ID навсегда устаревают, handlers освобождаются |

Отсоединённые узлы не участвуют в `preorder()`, layout, Scene, hit test,
focus, shortcuts и семантическом обходе. `contains(id)` проверяет существование,
`is_attached(id)` — достижимость от постоянного корня. `len()` считает все живые
узлы, включая отсоединённые; число смонтированных узлов равно `preorder().len()`.
Изменения отсоединённого содержимого не планируют кадр; повторный монтаж
инвалидирует геометрию. Старые layout/visual bounds отсоединённых ID недоступны.

Detach очищает связанный focus/capture и отменяет активный drag; повторный
монтаж не восстанавливает ввод автоматически. Controls сохраняет editor draft,
selection и undo вместе со своим состоянием. `Controls::remove` проверяет
целостность составных элементов и освобождает их дополнительные данные.
Если поддерево удалено напрямую через runtime или отложенную команду,
Controls очищает его данные и подписки после dispatch либо в начале следующего
update; отсоединённые живые узлы этой очисткой не затрагиваются. Обязательные
части составного элемента следует удалять вместе с его контейнером.

`apply_batch(&[Mutation])` и `Mutation::Batch(Vec<Mutation>)` выполняют операции
последовательно на изолированном снимке, затем подтверждают результат целиком.
Ошибка любой вложенной операции сохраняет исходное дерево, dirty flags,
focus/capture, обработчики и drag. Во время проверки callbacks не вызываются.
Успешный пакет требует не более одного следующего layout pass при одном `update`.
Проверка пакета копирует дерево: это корректный атомарный контракт, а не
incremental transaction log. Существующая очередь отдельных `enqueue` остаётся FIFO;
используйте один `Batch`, если операции должны быть неделимы.

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
| `Grid` | Auto/Px/Percent/Fr tracks, row-major размещение, явные one-based start и spans |

`Length::Percent(0.5)` означает 50% доступного внутреннего размера родителя.
`Auto` в Overlay заполняет доступную область с учётом min/max. Overlay требует
явного или назначенного родителем размера: дети не формируют его intrinsic size.
`offset` используется только у ребёнка Overlay. `gap` и flex-коэффициенты должны
быть конечными и неотрицательными; положительная бесконечность допустима только
для неограниченного `max_size`. Ограничения и цвета валидируются при изменении
свойств. Нулевой viewport разрешён.

`margin` задаёт конечные внешние отступы, включая отрицательные. Border box
не включает margin. Корневой margin игнорируется; его bounds равны viewport.
Подробности Grid и ограничений tracks — в [layout](layout.md).

`TaffyLayout` пересоздаёт внутреннее дерево при каждом layout pass и поддерживает
глубину до 128 рёбер от корня. Более глубокое дерево возвращает ошибку перед
запуском рекурсивного backend. Само retained tree обходит и удаляет узлы
итеративно. Flex-wrap, автоматические текстовые измерения и собственные intrinsic callbacks
в этот контракт пока не входят.

Глобальная позиция ребёнка равна глобальному origin родителя, минус `scroll`
родителя, плюс parent-local layout position, плюс собственный `translation`.
Translation не меняет layout. Scroll также не меняет layout и применяется только
к потомкам; диапазон прокрутки определяет приложение.

Корень всегда ограничивает вывод viewport. `clip = true` дополнительно обрезает
детей по border box узла. Все прямоугольники физически пересекаются с накопленным
clip перед включением в Scene. Только translation и прямоугольные clips
поддерживаются runtime; rotations, scale transforms и rounded clips отсутствуют.

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
обход. `set_focus_scope(owner, Some(FocusScope { cycle_tab: true, ... }))`
ограничивает Tab/Shift+Tab ближайшим таким поддеревом и зацикливает его пригодные
tab stops. Явный `Focus` может выйти из области. Пространственную навигацию
и keyboard-поведение меню определяют компоненты.
`navigation_order()` возвращает preorder этой области, включая непригодные для
фокуса узлы. Составные элементы фильтруют его по доступности и tab-stop состоянию.
RadioButton и заголовки вкладок используют эту же границу при выходе из группы:
кнопка за пределами cycle scope не заставляет их терять фокус при Tab.

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

## Keyboard shortcuts

`on_shortcut(owner, chord, allow_repeat, handler)` регистрирует один `KeyChord`
на поддерево; повторная регистрация той же комбинации заменяет обработчик.
`remove_shortcut` удаляет её. Модификаторы совпадают точно; `Key::Character`
сравнивается в нижнем регистре. Это logical key, не text input или IME commit.
`has_shortcut(&input)` проверяет маршрут без вызова callback: Controls использует
это перед специальной навигацией ListBox, SpinBox и других составных редакторов.

Shortcut рассматривается после routed key handlers, если `prevent_default`
не установлен. Поиск идёт от focus к корню: ближайшее совпадение выполняется
один раз. `FocusScope::isolate_shortcuts` запрещает переход к внешним областям
при отсутствии совпадения. Отсоединённые, удалённые и неактивные узлы не
участвуют; без focus допускается регистрация постоянного корня.

Обработчик получает `ShortcutEvent` и обычный `EventContext` с очередью mutations.
Совпадение подавляет default behavior. При `allow_repeat = false` повторный
KeyDown также потребляется, но callback не вызывается и внешняя область его
не получает. `DispatchReport.shortcut` содержит владельца вызванного обработчика,
а `default_prevented` учитывает потреблённую комбинацию. Host и Controls должны
соблюдать это значение и не выполнять дополнительное действие редактора.
Controls также подавляет совпадающий text event, непосредственно следующий
за потреблённой символьной комбинацией. Следующее input-событие сбрасывает эту
связь; другой текст, `set_text` и отдельный IME commit не подавляются.

## Drag/drop

`DragDropService` задаёт общий lifecycle одного drag, а `UiRuntime` связывает
его с деревом, capture, hit test и событиями окна. `begin_drag(source, payload,
effects, position)` вызывается после выбранного приложением порога жеста.
`DragPayload` содержит именованный формат и байты, `DropEffects` разрешает
Copy/Move/Link. Native drag между приложениями, clipboard transport и конкретные
docking targets относятся к платформе и соответствующим компонентам.
Размер payload ограничен `MAX_DRAG_PAYLOAD_BYTES` (1 MiB), имя формата —
`MAX_DRAG_FORMAT_BYTES` (256 UTF-8 байт), без NUL и пустого имени. Некорректный
payload отклоняется до начала сессии. `DragSession.payload` хранит `Arc<DragPayload>`:
снимки сессии при pointer movement не копируют массив байт.

`on_drop_target(id, handler)` регистрирует принимающую область. Pointer capture
остаётся на source, а target определяется отдельным hit test и ближайшим
зарегистрированным предком. Отказ ближайшего target не передаёт drop предку.
Callback получает `DragEvent` и payload. Enter/Over обновляют feedback;
результат Over выбирает эффект из разрешённых source. Неподдерживаемый эффект
означает отказ. Leave убирает feedback прежнего target. PointerUp повторно
проверяет target и вызывает Dropped ровно один раз только после согласования.
Менять прикладные данные следует на Dropped, а не на Enter/Over.

`drag_session()` показывает актуальное согласование, `take_drag_events()`
возвращает события для source/cursor и освобождает очередь. У каждого успешного
begin ровно одно терминальное событие: Dropped или Cancelled с причиной.
Очередь ограничена `MAX_DRAG_EVENTS` (4096), последовательные Over одной области
объединяются. При заполнении промежуточная история hover заменяется явным
`Resynchronized` с актуальными target/effect/position; такой snapshot следует
применить целиком вместо воспроизведения промежуточных переходов. Прямые target
callbacks не получают Resynchronized. Started и terminal result сохраняются;
если для новой сессии недостаточно места, begin возвращает `EventCapacity`.
Освобождайте журнал через `take_drag_events()` и обрабатывайте отказ begin.
Escape, blur, смена capture, detach/удаление/отключение source либо target
отменяют drag без drop. `cancel_drag(id)` позволяет отменить его явно;
устаревший `DragId` не затрагивает следующую сессию. При отказе на PointerUp
результат — `Cancelled(Rejected)`. После завершения source capture освобождается.

Маршрут обычного PointerUp сначала завершается и применяет mutations. Поэтому
удаление target его обработчиком отменяет операцию до вызова Dropped. Неуспешный
атомарный пакет не вызывает ни cancellation callbacks, ни иных событий drag.
Изменение transform в этом маршруте пересчитывает hit test до drop. Если mutation
требует нового layout, которого ещё нет, операция завершается
`Cancelled(GeometryUnavailable)`; прежние bounds не используются для drop.
При удалении target терминальный callback вызывается после commit удаления,
но до освобождения его регистрации, поэтому feedback может быть очищен.
PointerLeft очищает присутствие pointer: последующее изменение геометрии
не возвращает его к старому target до нового pointer события.
Эти проверки lifecycle выполняются без генерации системного ввода; проверка
native capture и оконного routing проводится отдельно в host.

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

Scene поддерживает упорядоченные rectangles, rounded rectangles, `StrokePath`
и `TextRun`: методы `fill`, `rounded_fill`, `rounded_stroke`, `stroke_path`, `text`
и `append` сохраняют их общий порядок. Сам UiRuntime рисует backgrounds/borders;
Controls и TreeGrid дополняют сцену текстом и содержимым. Текстовая подготовка и
GPU glyph atlas находятся в отдельных пакетах. Поля `FrameStats` runtime не
считают работу shaping, glyph uploads и пользовательского view — для них нужны
собственные счётчики. IME и AccessKit остаются обязанностью платформенного host.
