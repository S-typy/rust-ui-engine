# ADR-003: retained runtime, события и адаптер layout

Дата: 2026-10-09. Статус: принято и реализовано в M1.

## Контекст

[ADR-001](ADR-001-retained-tree.md) определил retained tree и независимое ядро;
[ADR-002](ADR-002-scene-wgpu.md) установил границу Scene/GPU. Для следующего этапа
нужны конкретные правила идентичности узлов, mutation во время routing,
геометрии и invalidation. Эти правила должны проверяться без нативного окна.

## Решение

`rust-desktop-ui-core` остаётся пакетом без внешних зависимостей. Он содержит
generational arena, `UiTree`, `UiRuntime`, нормализованные события, собственные
layout-типы и Scene. Новый `rust-desktop-ui-layout` реализует `LayoutEngine`
через Taffy 0.14.0; публичные типы ядра не зависят от Taffy, winit или wgpu.
В workspace добавляется действующий адаптер, а не заготовка будущей подсистемы.

`WidgetId` включает identity дерева, slot и generation. Удалённый slot можно
переиспользовать только с новым generation; переполнение generation выводит
slot из обращения. Чужие IDs не разрешаются даже при совпадении slot/generation.
Корень постоянен, cycles запрещены, structural links меняются только методами
дерева. Проверяемые ошибки возникают до изменения структуры.

Runtime владеет деревом и его вычисленными данными. Обработчики получают
`RoutedEvent` и `EventContext`, через который запрашивают отложенные mutations.
Маршрут capture → target → bubble фиксируется до вызовов. После завершения
маршрута выполняются default actions, затем FIFO-команды callbacks. Отклонённая
команда не откатывает предыдущие и не отменяет следующие; ошибки возвращаются
в `DispatchReport`.

Focus и capture хранят `WidgetId`, а не ссылки на узлы. Default primary down
выбирает ближайшего focusable предка. Tab/Shift+Tab обходят логический preorder.
Capture включается явно; primary up, blur, удаление или утрата пригодности
освобождают его. Hover вычисляется по фактической геометрии независимо от capture.
Native capture за пределами окна остаётся обязанностью платформенного слоя.

`LayoutSnapshot` хранит parent-local border boxes отдельно от свойств узлов.
Runtime разрешает translation, scroll, унаследованные visibility/enabled, clips
и стабильный sibling z-order. Painting и hit testing используют один порядок
и одни clips. Scene сохраняет rectangle ABI: прямоугольники обрезаются
геометрически до передачи существующему GPU renderer.

Stack и Flex используют одну линию; Stack отключает grow/shrink детей, Flex
выполняет распределение через Taffy. Overlay назначает детям общую внутреннюю
область и не выводит intrinsic size из детей. Backend-типы и промежуточные
wrapper nodes не выходят наружу. Layout остаётся в дробных логических пикселях.
Root size определяется viewport. Глубина адаптера ограничена 128 рёбрами с
явной ошибкой перед рекурсивным layout; retained tree обходится итеративно.

Разделяются `MEASURE`, `LAYOUT`, `PAINT` и `SEMANTICS`. Геометрические flags
распространяются к предкам. Paint/hover/focus/scroll не требуют нового layout.
В M1 геометрическая invalidation пересчитывает layout tree целиком, paint
invalidation пересобирает Scene целиком. Idle использует готовую Scene без
обхода узлов. Delta и cumulative counters позволяют отличать эти случаи.

## Последствия и ограничения

Логика и геометрия тестируются без GPU, а shell нормализует нативный ввод и
передаёт готовую Scene renderer. GPU lifecycle не становится частью `UiRuntime`.
После geometry mutation и resize host вызывает `update` до следующего hit test;
перед render он обновляет Scene после событий. Ошибки не считаются успешным
кадром, предыдущая Scene сохраняется.

Текущая реализация ориентирована на небольшие retained trees. Пересоздание
внутреннего Taffy tree, полный paint traversal и линейный hit test имеют цену
O(N) и не доказывают готовность больших таблиц. Пространственный индекс,
инкрементальный layout и виртуализация требуют отдельных измерений и решений.

Модель содержит один pointer, один focus и по одному callback на узел. Focus
scopes, multitouch, произвольные transforms, GPU text, IME и accessibility
backend отсутствуют. Semantics flags задают границу invalidation, не заменяя
семантическое дерево платформы. Primitive hover/focus feedback не означает
реализацию готовых controls.

## Альтернативы

- Прямой mutable доступ к дереву из callbacks нарушает неизменность маршрута
  при удалении и reparent; выбран явный FIFO mutation buffer.
- Taffy-типы в core API связывают будущие controls с конкретной версией backend;
  выбран собственный ограниченный контракт и отдельный адаптер.
- Полностью собственный Flex solver увеличивает объём новой layout-логики;
  используемый адаптер уже выполняет реальное распределение и ограничения.
- GPU clip stack изменил бы текущий Scene ABI. Для axis-aligned rectangles
  достаточно пересечения геометрии в core; более сложная графика потребует
  следующего решения.
- Инкрементальный layout и spatial index сразу усложнили бы проверку foundation;
  исходный полный пересчёт ограничен явно и наблюдаем через счётчики.

## Проверка

Unit-тесты покрывают устаревшие/чужие IDs, generation overflow, invariants дерева,
ошибки и dirty flags; порядок routing и deferred mutations; focus/capture
cleanup; clips/z/scroll/translation; layout constraints и размеры viewport;
paint без layout и нулевую работу в idle. Примеры API и их запуска приведены
в [retained-ui.md](../retained-ui.md).

Компиляция, тесты layout, нативный ввод и GPU runtime имеют отдельные результаты
в [M1_TEST_REPORT.md](../M1_TEST_REPORT.md). Наличие реализации или тестового сценария
само по себе не подтверждает прохождение GPU проверки на конкретной платформе.
