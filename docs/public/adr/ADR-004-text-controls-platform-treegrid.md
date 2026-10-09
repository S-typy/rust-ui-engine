# ADR-004: текст, controls, platform host и TreeGrid

Статус: принято для исходного alpha API. Дата: 2026-10-09.
Дополняет [ADR-003](ADR-003-retained-runtime.md) и
[ADR-002](ADR-002-scene-wgpu.md). Не означает прохождение runtime gates всех ОС.

## Контекст

Retained primitives требуют настоящего текста, редактирования и общего поведения
компонентов. Ribbon и большая иерархическая таблица должны пользоваться этими
механизмами без зависимости моделей от окна или GPU. Plain logical key events
недостаточно для committed text/IME, а semantic payload без платформенного
адаптера не обеспечивает взаимодействие с screen reader.

## Решение

1. `ui-text` хранит TextDocument и подготавливает текст через Parley и Swash.
   Document использует UTF-8 byte offsets на grapheme boundaries, временный
   preedit и ограниченную историю undo. Shaped caret сохраняет bidi affinity.
   Raster output содержит process-local glyph key, physical position/clip,
   shared straight-RGBA bitmap и tint. Системные шрифты не копируются в продукт.
2. Scene сохраняет общий порядок rectangles и TextRun через DrawCommand.
   `ui-render-wgpu` загружает bitmap в GPU atlas и рисует glyph instances.
   CPU shaping/raster preparation не заменяют аппаратную композицию UI.
3. `ui-controls` владеет поведением компонентов и использует retained runtime,
   text и layout. CommandRegistry централизует enabled/can_execute и исполнение.
   Ribbon находится в этом пакете как композиция tabs/groups/command items;
   отдельный пустой пакет для него не создаётся.
4. `ui-platform-winit` владеет event loop, вводом, clipboard, IME и AccessKit.
   DesktopApp передаёт независимую Scene и получает PlatformEvent. Logical keys,
   committed text и IME preedit/commit разделены. Host передаёт platform actions
   приложению; app связывает независимые semantic models с AccessKit nodes.
5. `ui-treegrid` зависит только от core. TreeDataSource обеспечивает индексированный
   доступ к stable keys; разреженный индекс и viewport view не создают retained
   node для каждой записи. Async execution принадлежит источнику/host, модель
   проверяет request tokens и отвергает устаревшие ответы. Cell edit использует
   общий TextBox в приложении. Модель не записывает файлы persistence сама.
6. `controls-gallery` связывает подсистемы: книжные данные, background query,
   lazy children, clipboard, editor и семантику. Эти предметные решения не
   входят в core или generic TreeGrid.

## Последствия и ограничения

Модели и логика доступны unit-тестам без окна/GPU. Платформенный ввод, candidate
window IME, screen readers и качество GPU вывода всё равно требуют отдельных
запусков. Матрица этих проверок находится в [runtime-matrix](../runtime-matrix.md).

Текст и GPU atlas используют ограниченные кэши. Слишком большой видимый набор
глифов возвращает ошибку. Scene поддерживает axis-aligned clips и translation;
general transforms, images и complex masks не добавляются этим решением.

Layout поддерживает Grid и margin, но автоматически измерять текст внутри Leaf
пока не умеет. Grid data view использует фиксированную высоту строк; variable
heights, grouping/summary, Docking и PropertyGrid не включены в alpha scope.
API и persistence развиваются как pre-1.0: бинарный release требует отдельной
проверки совместимости, runtime качества и лицензионной упаковки.

## Рассмотренные варианты

Передача Parley/Taffy/wgpu types в core упростила бы адаптацию, но связала бы
основной API с несколькими независимыми backend lifecycles. Поэтому за границей
остаются собственные geometry/layout/text/semantic контракты.

Полное retained дерево записей таблицы сделало бы память и layout зависимыми
от общего размера данных. Выбран viewport view с индексированным источником.
Собственный editor ячеек дублировал бы Unicode/IME логику; выбран общий TextBox.

Документация реализаций: [text](../text.md), [controls](../controls.md),
[Ribbon](../ribbon.md), [TreeGrid](../treegrid.md), [layout](../layout.md).
