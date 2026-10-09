# 0.1.0 alpha candidate

Дата состояния: 2026-10-09. Это описание исходного кандидата M2–M5, а не
объявление бинарного релиза. Контракты alpha 0.1.0 описаны и зафиксированы;
совместимость API до 1.0 не гарантируется. Совместное ручное
тестирование и оставшиеся платформенные проверки предшествуют подтверждению
готовности к выпуску; отсутствие результата не обозначается как PASS.

## Добавлено

- Unicode text: системный font fallback, Parley shaping/bidi/wrapping, Swash
  rasterization и настоящий GPU glyph atlas с сохранением порядка Scene.
- TextDocument и TextBox: grapheme-safe editing, selection, clipboard requests,
  undo/redo, IME preedit/commit/cancel и shaped caret geometry.
- Label, Button, ToggleButton, CheckBox, ScrollView, TabControl, меню/popup;
  собственные темы Light/Dark/Compact и общий CommandRegistry.
- Ribbon: Large/Small/Toggle/Split items, quick access, contextual tabs,
  adaptive collapse/overflow и keyboard KeyTips.
- TreeGrid: stable keys, разреженный hierarchy index, виртуализация rows/columns,
  frozen header, pinned columns, selection/navigation, resize/reorder,
  lazy children/cancellation, source-driven filter/sort, edit validation и
  versioned column persistence.
- Native host: winit events, раздельные key/text/IME paths, clipboard через
  arboard и AccessKit tree/action adapter.
- Layout: Grid tracks/placement/spans и margin поверх Taffy, с валидацией.
- Gallery: Library/Controls/About, 24 колонки, фоновые операции книжного
  источника и редактор ячейки на общем TextBox.

## Запуск и проверка

```sh
cargo run --release --locked -p controls-gallery -- --rows 100000
cargo run --release --locked -p controls-gallery -- --rows 100000 --state-file target/gallery-columns.txt
cargo run --release --locked -p controls-gallery -- --rows 100000 --smoke-test
cargo bench -p rust-desktop-ui-treegrid --bench viewport --locked
```

Требования сборки — в [README](../../README.md). Gallery smoke проверяет реальное
окно, представление кадров и resize. CPU benchmark измеряет построение Scene
и ограниченность материализации для 1k/100k/1M записей; он не является GPU FPS
или native-input benchmark. Результаты и протокол ручной проверки — в
[runtime matrix](runtime-matrix.md).

## Незакрытые критерии и ограничения

Автоматический intrinsic text measure в layout не подключён: текст нужно
измерять и назначать размеры отдельно. Layout пересчитывается целиком при
geometry dirty. TextBox — plain text editor; rich text, password behavior,
spellcheck и большой файловый rope отсутствуют.

Native IME candidate placement, commit/cancel, screen-reader interaction,
межмониторный DPI и GPU runtime Linux/macOS ещё требуют подтверждения.
Наличие соответствующих API и unit tests не закрывает эти критерии. Alpha
не объявляется полностью готовой к production по всем P0 требованиям.

TreeGrid использует фиксированную высоту строк. Variable heights, grouping,
summary rows, advanced editors, Docking и PropertyGrid выходят за текущий scope.
General image/vector drawing, rounded clips, rotation, opacity layers и
software rendering fallback не реализованы. Покрытие письменностей зависит
от установленных шрифтов; часть цветных форматов fonts не поддерживается.

Atlas 2048×2048 ограничивает видимый набор glyphs; renderer возвращает ошибку,
если набор не помещается после очистки. Ограничения text/cache/layout приведены
в [text](text.md) и [layout](layout.md).

Собственный код — Apache-2.0; binary distribution требует выполнения условий
[сторонних лицензий](dependencies.md). Системные шрифты не включены в поставку.
