# Runtime matrix и протокол проверки alpha

Дата состояния: 2026-10-09. Матрица относится к текущему `controls-gallery`.
Исторические Windows GPU и трёхплатформенные CI проверки M1 сохранены в
[M1_TEST_REPORT](M1_TEST_REPORT.md); они не подменяют результаты нового gallery.
«Не проверено» означает отсутствие результата, а не известный отказ.

## Зафиксированное состояние

| Проверка | Windows | Linux X11 | Linux Wayland | macOS |
|---|---|---|---|---|
| Новый source: локальные workspace tests | 141 tests + 1 doctest PASS | Не запускались локально | Не запускались локально | Не запускались локально |
| Полный финальный workspace CI alpha | Результат ожидается | Результат ожидается | Тот же Linux compile job | Результат ожидается |
| Gallery GPU: text/resize/redraw | Vulkan и DX12 PASS, RTX 3090, scale 1.5 | Не проверено | Не проверено | Не проверено |
| Native posted mouse gallery scenario | PASS: pages/Ribbon/theme/resize/close | Не проверено | Не проверено | Не проверено |
| Native IME preedit/candidate/commit/cancel | Не проверено | Не проверено | Не проверено | Не проверено |
| Screen reader и platform accessibility actions | Не проверено | Не проверено | Не проверено | Не проверено |
| Clipboard между приложениями | Не проверено | Не проверено | Не проверено | Не проверено |
| DPI между мониторами, minimize/restore, длительный idle | Не проверено для gallery | Не проверено | Не проверено | Не проверено |
| Физический device loss | Не проверено | Не проверено | Не проверено | Не проверено |

Text/layout/controls unit tests проверяют CPU модели и shaped geometry без окна.
Локальный Windows font probe получил `.notdef = 0` для Latin/Cyrillic/Arabic/
Hebrew/CJK/emoji. Это покрытие конкретных системных шрифтов, не гарантия для всех
машин и не оценка качества каждого glyph raster.

Vulkan smoke представил три кадра с настоящими text runs и подтвердил native
resize. Дополнительный native harness прошёл Vulkan/DX12; снимки Light/Dark и
resize просмотрены. OS text entry и IME этим не проверялись. Подробности и
воспроизведение — в [alpha test report](M2_M5_TEST_REPORT.md).
TreeGrid CPU benchmark 1k/100k/1M с фиксированным viewport подтверждает
ограниченную материализацию; числа и условия приведены в [TreeGrid](treegrid.md).

## Воспроизводимый запуск

```sh
cargo build --workspace --release --locked
cargo run --release --locked -p controls-gallery -- --rows 100000 --smoke-test
cargo run --release --locked -p controls-gallery -- --rows 100000 --state-file target/gallery-columns.txt
```

Для выбора backend установите `WGPU_BACKEND=dx12`, `vulkan` или `metal` согласно
ОС. Сохраняйте exit code, adapter/backend/device type, драйвер, DPI, размер окна,
revision исходников и stderr. `SMOKE PASS` относится только к bounded GPU
scenario. Timeout, раннее закрытие или ошибка должны давать ненулевой exit code.

## Ручная проверка после интеграции

1. Library: пролистать 1k, 100k и 1M; проверить frozen header, pinned columns,
   горизонтальный scroll, hit test после resize и отсутствие роста UI nodes
   пропорционально числу записей. Зафиксировать frame/work counters отдельно
   от субъективной плавности.
2. Данные: раскрыть lazy ветку, свернуть до ответа, повторить раскрытие; изменить
   filter/sort пока запрос активен. Старый ответ не должен возвращать прежний view.
   Проверить empty/error/loading состояния и stable-key selection.
3. Ячейки и колонки: click/Ctrl/Shift selection, keyboard navigation, F2/Enter,
   validation error, commit/cancel, reorder/resize/pin. Сохранить state, перезапустить,
   проверить восстановление; повреждённый файл не должен частично менять колонки.
4. Controls/Ribbon: disabled/can_execute, press-drag-release, Tab/Shift+Tab,
   tabs, popup Escape/outside click, scroll; Alt/F10 KeyTips, split menu, QAT,
   contextual tab, collapse/overflow при узком окне и переключение всех тем.
5. Текст/clipboard: Cyrillic/Latin, emoji ZWJ, combining marks, mixed bidi,
   multiline selection, visual arrows, undo/redo, copy/cut/paste с другим приложением.
   Не сохранять чувствительные clipboard contents в отчёт.
6. IME: системный CJK input method, preedit selection, candidate window у caret,
   commit/cancel, смена focus/окна, resize и DPI во время composition. Проверить
   отсутствие удвоения текста между keyboard и IME paths.
7. Accessibility: NVDA/Narrator на Windows, VoiceOver на macOS, Orca на Linux.
   Проверить роли/имена/состояния/focus/value/actions, доступ к Ribbon/menu,
   row/column counts и visible virtual grid rows; убедиться, что дерево connected.
8. Окно/GPU: minimize/restore, occlusion, resize до минимального размера,
   перенос между DPI, удержание pointer за границей окна, длительный idle,
   закрытие с активным background query. Настоящий device loss учитывается
   отдельно от synthetic recovery/unit tests.

Каждый выполненный пункт фиксируется с ОС, backend, сценарием, ожидаемым и
фактическим поведением. Только реально выполненные пункты меняются на PASS.
Эта матрица не разрешает бинарный release и не объявляет неизвестные проверки
успешными.
