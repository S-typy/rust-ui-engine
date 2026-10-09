# Архитектура Rust UI Engine

Rust UI Engine проектируется как независимый retained-mode фреймворк настольного интерфейса для Windows, macOS и Linux с X11/Wayland. Цель — универсальные компоненты для приложений с большим объёмом данных: Ribbon, TreeGrid, DataGrid, Docking, PropertyGrid и editors.

M1 добавляет retained tree, маршрутизацию событий, focus/capture, invalidation и layout adapter к сцене прямоугольников и renderer M0. Текущий пример использует эти подсистемы для интерактивных примитивов. Текст, IME, доступность и готовые компоненты остаются дальнейшими этапами. Фактические результаты сборки, CI и GPU runtime приводятся в [отчёте M1](M1_TEST_REPORT.md); [отчёт M0](TEST_REPORT.md) и [его изменения](M0_CHANGES.md) сохраняют историю предыдущего состояния.

## Реализованные пакеты

| Пакет | Содержимое и зависимости |
|---|---|
| `rust-desktop-ui-core` | Arena/WidgetId, UiTree, UiRuntime, события, focus/capture, dirty flags, собственные layout-контракты, геометрия и Scene; внешних зависимостей нет |
| `rust-desktop-ui-layout` | Stack/Overlay/Flex через внутренний Taffy adapter; зависит только от core и Taffy |
| `rust-desktop-ui-render-wgpu` | GPU instancing, WGSL, surface/device lifecycle; зависит от core, wgpu, winit и bytemuck |
| `gpu-shell` | Event loop winit, RetainedDemo, нормализация ввода/DPI, диагностика и восстановление renderer; pollster используется для инициализации |

Retained demo находится в `examples/gpu-shell/src/retained.rs`: три focusable-примитива A/B/C, Stack слева и область с clipping/scroll справа. Имена A/B/C используются в диагностике, текст в GPU сцене не рисуется. Нажатие активирует примитив и назначает capture до отпускания; Tab/Shift+Tab перемещают focus, Enter/Space активируют выбранный примитив, Delete удаляет C при его focus. Wheel прокручивает правую область, а при её focus работают Up/Down/Home/End. Это пример механизмов ядра, а не реализация Button или ScrollViewer.

Исходная демонстрация `DemoState` из `demo.rs` доступна через `--rectangles`. Её 100 000 строк — диапазон логических индексов, а не 100 000 retained nodes, полноценная таблица или benchmark.

`Scene` содержит `Vec<SolidRect>` в логических пикселях. `Scene::fill` добавляет конечные прямоугольники с положительными размерами; порядок добавления задаёт порядок рисования. Runtime вычисляет глобальные координаты, translation, scroll и пересечение ancestor clips, затем обрезает прямоугольники геометрически. Border и focus border также состоят из прямоугольников. Порядок z-index и стабильный порядок siblings одинаковы для рисования и обратного hit test. GPU handles и оконные типы в сцену не входят. Renderer переводит координаты в физические пиксели с учётом scale factor; один batch ограничен 16 384 прямоугольниками. GPU clip stack, маски, rotation и произвольные transforms не реализованы.

`GpuRenderer::new(Arc<Window>)` создаёт renderer и возвращает `Result` с типизированным `GpuError`. Пакет совмещает GPU backend и адаптер surface к winit. `resize` сохраняет физический размер и проверяет GPU limits; конфигурация surface откладывается до кадра ненулевого размера. `render` возвращает `Result<RenderOutcome, GpuError>`:

- `Presented { rectangles }` — кадр отправлен и передан на present;
- `Skipped` — нулевой размер, timeout, occlusion или повторная настройка surface;
- `RecoveryRequired` — потеря surface или device, требующая пересоздания renderer.

Shell пропускает рисование при нулевом размере и occlusion, планирует повтор после timeout/reconfiguration и ограничивает восстановление тремя попытками подряд без показанного кадра. Успешный кадр сбрасывает счётчик. Состояние демонстрации переживает пересоздание renderer. При простое event loop ожидает события, не запрашивая постоянную перерисовку. Ошибки создания окна, GPU и event loop завершают приложение с диагностикой и ненулевым кодом.

Подробный контракт и альтернативы зафиксированы в [ADR-002](adr/ADR-002-scene-wgpu.md). Наличие обработки потери device в коде не означает проверки реального аппаратного сбоя.

## Сборка и платформенные зависимости

Toolchain закреплён на Rust 1.96.0, edition — 2024, Cargo resolver — 3. `rust-version = "1.96"` обозначает заявленную версию проекта; более низкий MSRV не проверен. `Cargo.lock` фиксирует разрешённые версии зависимостей. Команды из корня workspace:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked
cargo doc --workspace --no-deps --locked
cargo run --release --locked -p gpu-shell
```

В CI rustdoc запускается с `RUSTDOCFLAGS="-D warnings"`. Матрица включает Ubuntu 24.04, Windows Server 2025 и macOS 15; состояние выполненных проверок отражается в отчёте, а не выводится из наличия workflow.

`wgpu 30.0.1` включён с `std`, `parking_lot`, `dx12`, `metal`, `vulkan`, `wgsl`; `winit 0.30.13` — с `rwh_06`, `x11`, `wayland`, `wayland-dlopen`. `taffy 0.14.0` использует только `std`, `taffy_tree`, `flexbox`. У всех трёх отключены default features. Обычные GPU backend — D3D12/Vulkan на Windows, Vulkan на Linux, Metal на macOS. `WGPU_BACKEND=dx12`, `vulkan` или `metal` ограничивает выбор для отдельного запуска. Требуются совместимые GPU, драйвер и desktop session; CPU и Noop adapters отвергаются. VirtualGpu/Other оцениваются по фактической диагностике запуска.

Стандартная Adwaita-рамка отключена. На Wayland compositor без server-side decorations окно может отображаться без рамки и кнопок заголовка; собственная client-side рамка ещё не реализована. OpenGL, WebGPU и Vello не включены. Состав features, лицензии, системный FFI и ограничения поставки описаны в [аудите зависимостей](dependencies.md).

## Проверка окна и GPU

```text
cargo run --release --locked -p gpu-shell -- --smoke-test
```

Встроенный сценарий открывает retained окно, показывает кадры, запрашивает нативный resize и отправляет синтетические input events непосредственно в демонстрацию. Успех требует двух активаций, удаления C и scroll=84 после следующего кадра; приложение печатает `SMOKE PASS mode=retained` и завершает работу с кодом 0. Ошибка, timeout или раннее закрытие дают ненулевой код. Этот сценарий не проверяет физическую мышь и содержимое пикселей. Прежний сценарий выполняется с `--rectangles --smoke-test`.

Отдельный [retained Windows smoke test](retained-smoke.md) отправляет синтетические Win32 pointer/keyboard сообщения через нативный event loop, проверяет routing feedback, focus/capture, удаление, scroll, счётчики layout/paint, resize, minimize/restore, idle и close. [Прежний harness](windows-smoke.md) выбирает `--rectangles`. Оба сохраняют JSON, логи и доступный снимок клиентской области; визуальная оценка снимка выполняется отдельно. Ни один из этих сценариев не подтверждает IME, accessibility, смену DPI между мониторами или аппаратную потерю device. Результат относится только к указанным ОС, adapter и backend.

## Слои и зависимости

```text
Application
    -> Controls, commands, themes, data sources
    -> Retained runtime: tree, state, events, focus, invalidation
    -> Layout / text / accessibility + renderer-independent Scene
    -> Platform adapter (winit) + GPU renderer (wgpu)
    -> Native operating system and GPU drivers
```

Core и публичные controls не зависят от `wgpu::Device`, `winit::Window` или конкретного backend. Renderer принимает собственную модель Scene/DisplayList. Код демонстрационных приложений и работа с их базами данных не входят в ядро.

Собственный runtime и компоненты реализуются на стабильном Rust. WGSL используется для GPU shaders; системные API и необходимый FFI относятся к платформенной границе. Готовые GUI-фреймворки и C++ графические движки не используются как основа интерфейса.

Cargo workspace содержит `crates/ui-core`, `crates/ui-layout`, `crates/ui-render-wgpu` и `examples/gpu-shell`. Адаптер layout выделен в отдельный действующий пакет, чтобы core сохранял нулевые внешние зависимости.

## Retained runtime M1

Решения зафиксированы в [ADR-001](adr/ADR-001-retained-tree.md) и [ADR-003](adr/ADR-003-retained-runtime.md); использование API — в [retained-ui.md](retained-ui.md).

`WidgetId` — opaque идентификатор со slot, generation и identity дерева. Слот после удаления переиспользуется с новым поколением; при переполнении generation он больше не используется. ID чужого дерева и устаревший ID не разрешаются. `UiTree` обеспечивает parent/children consistency, запрещает циклы и удаление root. Удаление subtree выполняется целиком; структурные поля не выдаются через mutable references.

`UiRuntime` владеет деревом, handlers, focus/hover/capture и cached Scene. Нормализованное событие проходит hit test и capture → target → bubble по снимку маршрута. Handler может остановить распространение, отменить default action и поставить `Mutation` в очередь. Очередь применяется после маршрута; удаление узла очищает handlers и устаревшие interaction IDs. Primary down выбирает ближайшего подходящего focus ancestor, Tab/Shift+Tab обходят доступные узлы, keyboard events адресуются focus. Pointer capture явно назначается и очищается при primary up, blur или потере допустимости узла.

`DirtyFlags` разделяют measure/layout/paint/semantics. M1 пересчитывает всё layout-дерево при изменении структуры, layout style или viewport; paint-only, hover/focus и scroll не вызывают layout. Когда paint необходим, Scene пока перестраивается целиком. При неизменном состоянии `update` возвращает нулевые счётчики и повторно использует Scene. Семантический счётчик отражает invalidation; платформенного accessibility tree ещё нет. Счётчики доступны через `FrameStats`, а layout/paint passes показаны в заголовке примера.

UI state используется в потоке event loop. Публичный API не даёт callback одновременно изменять структуру при обходе. Многопоточный renderer и интеграция фоновых задач ещё не являются отдельными подсистемами.

## Layout M1

Core определяет собственные `LayoutStyle`, `LayoutKind`, `LayoutEngine` и `LayoutSnapshot`. Snapshot хранит parent-local border boxes; root всегда равен viewport. `TaffyLayout` строит временное backend-дерево только при layout pass и возвращает дробные logical coordinates без pixel rounding. Внешние типы Taffy не входят в API core.

Stack задаёт одну горизонтальную или вертикальную последовательность: grow/shrink детей отключены. Flex распределяет свободное место с учётом grow/shrink и min/max. Overlay размещает детей в общей padded area с offsets; auto dimensions заполняют доступную область, percentages считаются относительно её размера. Overlay требует собственного или назначенного родителем размера: intrinsic размер по максимуму детей не вычисляется. Taffy wrappers, используемые для Overlay, не появляются в retained tree или snapshot.

Поддерживаются Auto/Px/Percent, min/max, padding, gap, align и justify. Leaf не принимает детей. Глубина layout ограничена 128 рёбрами от root; более глубокое дерево возвращает ошибку до вызова рекурсивного backend. Неконечные размеры/координаты и переполнение вычисленной геометрии отвергаются. Grid, margin, wrap, baseline/text measurement и incremental subtree layout остаются расширениями, поэтому полный набор целевых layout требований ещё не закрыт.

## Развитие графики и платформы

Основной backend использует wgpu и доступные нативные GPU API. При отсутствии подходящего GPU возвращается содержательная ошибка. CPU-подготовка layout, текста и команд допустима; скрытого переключения на CPU-only rendering нет.

Текущее API Scene ограничено заполненными прямоугольниками; прямоугольные clips и translation разрешаются runtime до renderer. В дальнейшем сцена сможет описывать пути, текстовые runs, изображения, сложные clips, transforms и opacity layers без ссылок на GPU device. Texture/glyph atlases и сложная GPU-композиция пока не реализованы.

Surface/device lifecycle должен учитывать нулевой размер, occlusion, resize, DPI, устаревшую surface и device loss. Размеры интерфейса выражаются в логических единицах; перевод в физические пиксели выполняется на границе платформы и renderer.

Продвинутый векторный backend, включая Vello GPU, требует отдельной проверки зрелости, совместимости, качества и лицензий. Он не является обязательной зависимостью начального renderer.

## Текст, доступность и расширение панелей

Дальнейшие layout-панели — Grid, Dock, SplitView и виртуализированные области. Нынешний hit test учитывает axis-aligned clips, translation, z-order и вложенный scroll; произвольные transforms потребуют нового геометрического контракта.

Текстовая модель отделена от TextBox. Shaping/layout через Parley и совместимые библиотеки дополняется GPU-отрисовкой глифов. Необходимы font fallback, Unicode grapheme navigation, bidi, кириллица, выделение, clipboard, undo/redo и IME composition.

Семантическое дерево доступности описывает роли, подписи, состояния, focus и действия. AccessKit служит адаптером к платформе; проверка включает реальные screen readers и виртуальные строки таблиц.

## Компоненты и данные — целевая модель

Ribbon строится из tabs, groups и command items. Реестр команд независим от конкретных кнопок; адаптивное схлопывание не должно скрывать доступ к командам с клавиатуры.

TreeGrid разделён на источник данных, model/view state и визуальное представление. Источник предоставляет стабильные ключи, детей, значения колонок и асинхронные результаты. View state хранит раскрытие, selection, sort/filter, viewport и редактирование.

Материализация строк и колонок ограничена viewport с overscan. Сложные sort/filter допускают делегирование источнику данных. Для переменной высоты строк используется индекс сумм высот; полный линейный обход всех строк на каждый scroll недопустим. Устаревшие асинхронные запросы отменяются или их результаты отклоняются.

Docking, PropertyGrid и расширенные editors развиваются поверх общих команд, layout и редакторов. Темы Light, Dark и Compact используют единые design tokens и собственные визуальные ресурсы.

## Качество

Проверки включают unit-тесты ядра и моделей, детерминированную геометрию layout, реальные GPU окна, keyboard/IME, accessibility и benchmark больших наборов данных. Compile CI и runtime matrix учитываются отдельно. Объявленные показатели производительности сопровождаются описанием оборудования и сценария.

Собственный код распространяется под Apache-2.0. Зависимости, шрифты и иконки проходят проверку лицензий и происхождения. API, схемы сохранения состояния и ошибки должны иметь явные контракты.
