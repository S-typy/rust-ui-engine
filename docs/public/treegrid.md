# TreeGrid alpha

`rust-desktop-ui-treegrid` содержит независимые data source, модель состояния
и viewport view. Crate зависит только от `rust-desktop-ui-core`; wgpu, winit,
база данных и async runtime не входят в его API. Текст и прямоугольники view
попадают в общий упорядоченный `Scene`.

## Подключение

Источник реализует `TreeDataSource`: количество и индексированный доступ к
детям, parent/position для стабильного `RowKey`, текст ячейки по `ColumnId`.
Ключ строки не меняется при сортировке, фильтрации или раскрытии другой ветки.
`child_at`, `position`, `parent` должны опираться на индекс источника, а не
линейный поиск среди всех данных.

```rust,ignore
use rust_desktop_ui_core::Rect;
use rust_desktop_ui_treegrid::{ColumnId, GridColumn, TreeGrid};

let mut title = GridColumn::new(ColumnId(1), "Название", 280.0);
title.pinned = true;
title.editable = true;
let mut grid = TreeGrid::new(vec![title,
    GridColumn::new(ColumnId(2), "Автор", 180.0)])?;
grid.refresh(&mut source)?;
let frame = grid.paint(&source, Rect::new(0.0, 0.0, 900.0, 600.0))?;
scene.append(&frame.scene);
```

`TreeGrid::paint` принимает актуальные bounds и ограничивает scroll по viewport.
`GridFrame` содержит Scene, материализованные строки, видимые колонки, hit regions,
семантику и счётчики работы. View не создаёт retained widget для каждой строки.

## Виртуализация и иерархия

Модель хранит разреженные интервалы между раскрытыми ветками. Плоский миллион
строк представлен одним интервалом; раскрытая ветка с миллионом индексированных
детей тоже не требует разворачивания всего списка в UI nodes. Поиск строки
по viewport index использует binary search интервала и `child_at` источника.

Каждый кадр обрабатывает строки viewport плюс небольшой overscan, а также
видимые колонки. Горизонтальный поиск использует prefix widths и binary search.
При изменении структуры пересобирается индекс раскрытых веток; при resize/reorder
колонок — их prefix widths. Эти операции отделены от обычного scroll.

- `set_expanded` раскрывает/сворачивает ветку. Вложенное expansion state
  сохраняется при сворачивании предка; выбранные скрытые строки сохраняют ключи.
- Header остаётся на фиксированной вертикальной позиции. Pinned columns
  закреплены слева; scrolling cells обрезаются по границе pinned области.
- Высота строки в alpha фиксированная (`GridConfig::row_height`, default 28).
  Header — 32, font size — 14, overscan — 2 строки с каждой стороны.
- Циклы и глубина выше 256 при expansion отвергаются. Источник обязан соблюдать
  согласованность parent/child/position и уникальность RowKey.

Доступ к источнику и построение кадра не обходят все строки. Явное выделение
большого диапазона или `select_all` может однократно посетить выбранные строки
и сохранить их стабильные ключи; это не работа каждого кадра.

## События, выделение и колонки

Передавайте pointer events в `pointer_down`, `pointer_move`, `pointer_up`,
используя последний `GridFrame`. Пока `pointer_captured()` возвращает true,
move/up должны поступать даже за пределами bounds. При потере window focus
вызовите `cancel_pointer()`.

Click по ячейке выбирает строку и колонку. `SelectionModifiers` поддерживает
Shift range и Ctrl toggle; Ctrl+Shift добавляет диапазон. `navigate` принимает
Up/Down/PageUp/PageDown/Home/End/Left/Right: вправо раскрывается ветка или
выбирается первый ребёнок, влево ветка сворачивается или выбирается parent.
Ctrl navigation перемещает focus без изменения выбранных ключей.

Header click переключает сортировку по колонке, drag перемещает колонку,
последние четыре пикселя header cell служат resize handle. Публичные методы
`resize_column`, `reorder_column`, `pin_column` доступны и без pointer input.
Width ограничивается положительными min/max текущей колонки.

`save_columns()` возвращает versioned text с ID, порядком, width и pinning.
Приложение выбирает файл и момент сохранения. `restore_columns()` сначала
проверяет весь документ; ошибочный документ не применяет частичные изменения.
Неизвестные исторические ID игнорируются, новые колонки добавляются в конец,
width ограничивается актуальными constraints. Persistence не записывает
пользовательские файлы сама.

## Запросы данных

`child_count(Some(row)) == None` означает ещё не загруженных детей;
`Some(0)` — leaf. Количество roots всегда известно. При раскрытии незагруженной
ветки вызывается `request_children(parent, RequestId)`. Источник может сразу
вернуть `ChildrenLoad::Ready(keys)` или `Pending` и выполнить работу своим worker.

Host передаёт ответ в `complete_children(&source, request, result)`. Token
содержит идентичность модели, epoch запроса и последовательный номер. Ответ
чужой модели, предыдущего query или отменённого запроса возвращает `Ok(false)`
и не меняет дерево. Некорректные duplicate/ancestor keys отвергаются.

Pending ветка показывает loading row; ошибка загрузки — сообщение ошибки.
Повторный `set_expanded(row, true)` позволяет повторить неуспешный запрос.
Сворачивание отменяет запросы всей скрываемой ветки через `cancel_request`.
`refresh` отменяет прежние запросы и сбрасывает lazy cache после обновления
индекса источника; активный editor при этом закрывается. Host должен
согласовать refresh с сохранением или отменой draft. При отсоединении grid от долгоживущего источника вызовите
`cancel_pending_requests`. Источник отвечает за фактическое завершение workers;
модель дополнительно отвергает поздние ответы.

`GridQuery` содержит multi-sort descriptors и filter string. `apply_query`
делегирует исполнение источнику; framework не загружает всю БД ради сортировки.
Источник может устанавливать подготовленный индекс асинхронно, после чего host
вызывает `refresh`. Стабильное выделение сохраняется при сортировке и фильтрации;
если выбранная запись действительно удалена приложением, приложение очищает
соответствующее selection state.

## Редактирование и семантика

Enter/F2 или `begin_edit(row, column)` открывают editor state, если и колонка,
и источник разрешают изменение. `set_edit_text` меняет draft; `commit_edit`
сначала вызывает `validate_edit`, затем `set_cell` источника. При ошибке draft
и validation error остаются доступными, исходное значение не подменяется моделью.
`cancel_edit`/Escape удаляет draft, focus остаётся на ячейке.

Сам grid не реализует второй текстовый редактор. Host размещает общий TextBox
поверх `frame.cell_bounds(row, column)`, передаёт в него keyboard/IME/clipboard,
а затем возвращает draft модели. Scene также рисует draft и error border.
Источник обязан выполнять `set_cell` атомарно при ошибке.

Gallery не открывает cell editor во время фонового поиска или сортировки:
F2/Enter, команда Ribbon и accessibility SetValue ждут завершения запроса.
Новый поиск при уже открытом editor откладывается до успешного commit или
Escape. Ошибка validation сохраняет draft и продолжает удерживать поиск;
поздний ответ источника не должен молча удалять несохранённый ввод.
Пока draft открыт, повторное редактирование и команды изменения grid view
возвращают пользователя к текущему editor. При уменьшении viewport overlay
временно скрывается с сохранением документа и validation error; IME
composition отменяется, скрытый editor исключается из accessibility tree.
Восстановление размера возвращает overlay без смены focus другого control.

`GridSemantics` содержит общее число строк/колонок, видимые виртуальные строки
со стабильными keys, depth, selection/focus, expanded/busy state и cell labels.
OS accessibility adapter назначает этим keys свою namespace и реализует
focus/scroll/expand/edit actions через публичные методы модели. Эта payload
сама по себе не является OS accessibility bridge.

## Проверки и CPU benchmark

```text
cargo test -p rust-desktop-ui-treegrid --locked
cargo clippy -p rust-desktop-ui-treegrid --all-targets --locked -- -D warnings
cargo bench -p rust-desktop-ui-treegrid --bench viewport --locked
```

Benchmark создаёт индексированный book source без массива всех записей,
прокручивает 50 колонок в viewport 1280×720, пропускает 30 прогревочных кадров
и измеряет 500 построений Scene на размер набора. Проверяются верхние границы
материализации. Измеряется CPU построение/освобождение кадра, не GPU и не FPS.

Один локальный release-прогон Windows x86_64 от 2026-10-09:

| Логических строк | p50, µs | p95, µs | p99, µs | Max materialized rows | Max visible columns | Max cell reads | Index segments |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 000 | 67.7 | 136.4 | 181.4 | 29 | 10 | 250 | 1 |
| 100 000 | 53.2 | 106.9 | 171.6 | 29 | 10 | 250 | 1 |
| 1 000 000 | 53.3 | 113.8 | 160.6 | 29 | 10 | 250 | 1 |

Числа относятся к этому запуску и не являются гарантией задержки на другой
машине. Native GPU/input проверяется отдельным gallery protocol.

Groups/summary rows и variable row heights с индексом сумм высот остаются
отдельным этапом. View пока рисует фиксированные строки и прямоугольные cells;
его нельзя считать реализацией этих расширений.
