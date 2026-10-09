# Требования к Rust UI Engine

Цель — универсальный настольный UI-фреймворк на Rust с профессиональными компонентами и штатным GPU rendering. Первая полезная версия 0.x объединяет базовые controls, Ribbon и TreeGrid; дополнительные компоненты развиваются после стабилизации основания.

Это целевые требования, а не заявление о готовности текущего прототипа. Архитектурные границы описаны в [architecture.md](architecture.md).

## Платформы и общие ограничения

- Windows, macOS и Linux с X11/Wayland; фактический запуск подтверждается отдельно для каждой проверенной конфигурации.
- Стабильный Rust и Cargo workspace; собственные core, controls и renderer adapter. WGSL и обязательные системные API допустимы.
- GPU-отрисовка через wgpu. Diagnostics показывают adapter/backend; отсутствие подходящего GPU приводит к понятной ошибке без скрытого CPU-only режима.
- Core и controls не импортируют конкретные оконные и GPU типы. Данные приложений подключаются через публичные интерфейсы.
- Оригинальные код, темы, иконки и документация; Apache-2.0 для собственного кода и проверенные лицензии зависимостей.
- Примеры работают офлайн после получения зависимостей. Сетевая телеметрия по умолчанию отсутствует.

## Обязательное основание

| Область | Требуемое поведение | Приёмка |
|---|---|---|
| Окно и GPU | Open/close, resize, DPI, minimize/restore; lifecycle surface/device | Реальные frames и журнал adapter/backend; отдельная матрица runtime |
| Ошибки | Zero-sized surface, occlusion, outdated/lost device и surface | Явные переходы состояния и воспроизводимые сценарии; ожидаемые ошибки не вызывают panic |
| Retained tree | Generational IDs, корректное удаление и жизненный цикл | Unit-тесты слотов, stale IDs, remount и очистки подписок |
| События | Hit test, focus, capture/target/bubble, keyboard и modifiers | Порядок маршрутизации и остановка распространения подтверждены тестами |
| Invalidation | Отдельные measure/layout/paint/semantics изменения | Hover одного узла не вызывает полный layout; доступны счётчики операций |
| Layout | Flex/Grid/Stack/Overlay, constraints, clips, scroll | Детерминированные snapshot-тесты геометрии |
| Масштаб | Логические и физические координаты | Совпадение изображения и hit regions при 100/125/150/200% DPI, где поддерживается |
| Текст | GPU glyph rendering, кириллица, fallback, Unicode | Визуальные проверки на нескольких DPI и тесты границ grapheme/UTF-8 |
| TextBox | Ввод, selection, cursor, clipboard, undo/redo, IME | Keyboard/Unicode tests и реальные IME проверки |
| Controls | Label, Button, Toggle, CheckBox, Panel, ScrollViewer, Tab, Popup/Menu | Hover/focus/pressed/disabled, keyboard activation, команды |
| Доступность | Роли, имена, значения, focus, actions | Семантические тесты и внешний screen reader smoke-test |
| Темы | Light/Dark и затем Compact | Переключение без пересоздания приложения, видимый focus и согласованные состояния |

## Ribbon

Необходимы tabs и groups, большие и малые кнопки, toggle и split button, quick access toolbar, contextual tabs, key tips и адаптивный overflow. Команды остаются доступны при разных размерах окна и полностью вызываются с клавиатуры. Представление не зависит от бизнес-логики приложения.

## TreeGrid

Обязательны стабильные ключи строк, expand/collapse, selection и keyboard navigation; resize/reorder и сохранение ширин колонок; фиксированные headers и pinned columns; редактирование с validation; базовые sort/filter; lazy loading и cancellation устаревших запросов.

Вертикальная и горизонтальная виртуализация ограничивают живые визуальные элементы видимой областью с overscan. Variable row heights, grouping и summary rows добавляются после стабильной базовой виртуализации. Доступность учитывает иерархию и виртуальные строки.

Наборы проверки: 1k, 100k и 1m логических строк, глубина дерева 1–6, 8–30 колонок; фиксированный seed, кириллица, латиница, emoji, длинные и пустые значения. Сценарий включает scroll, expand, resize, filter и сохранение редактора.

## Производительность и устойчивость

Цель для объявленного стенда — scroll 100k строк с p95 кадра ≤ 16,7 ms при 60 Hz, без блокировки UI и неконтролируемого роста памяти. Это целевой показатель для конкретного сценария, а не гарантия для любого оборудования и содержимого ячеек.

Отчёт содержит CPU p50/p95/p99, GPU frame time при доступных timestamp queries, пиковый объём памяти, draw calls, количество визуальных узлов и повторных layout. Указываются ОС, GPU/CPU, разрешение, шрифты, сценарий и версия кода. При отсутствии событий постоянная перерисовка всего интерфейса не выполняется.

Проверяются rapid resize, fast scroll, IME composition и закрытие во время загрузки. `unsafe` локализуется и сопровождается `SAFETY`-объяснением инвариантов. Публичный API использует SemVer и документированную политику MSRV/deprecation после начала releases.

## Этапы готовности

1. Воспроизводимый GPU shell: совместимые зависимости, fmt/clippy/test/build, CI Windows/macOS/Linux и фактический runtime report.
2. Retained core, события, focus, layout и независимая Scene.
3. GPU text, Unicode/IME, основные controls, темы и accessibility.
4. Ribbon alpha с проверенной клавиатурной навигацией и overflow.
5. TreeGrid alpha с benchmark 100k, editing и устойчивой асинхронной загрузкой.
6. Public alpha 0.x: документация API, examples, лицензии, runtime matrix и честный список ограничений.

Docking, PropertyGrid, полный DataGrid и расширенные editors относятся к последующим этапам. Совместимость с WPF/XAML, мобильные ОС, браузер и работа без GPU не входят в первоначальную цель.
