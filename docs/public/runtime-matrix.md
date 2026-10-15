# Runtime matrix и протокол проверки alpha

## Строки состояния — Windows, 2026-10-15

StatusBar, RibbonStatusBar, ProgressBar, ZoomControl и XAML-адаптеры проверены:
883 workspace tests и 2 doctests PASS, два hardware tests ignored.
fmt, workspace clippy all-targets, release, rustdoc с warnings-as-errors,
лицензии и инвентаризация зависимостей пройдены. Новых зависимостей нет.

NVIDIA GeForce RTX 3090, scale=1.5: галерея `--status` прошла четыре capture
и четыре GPU smoke DX12/Vulkan × Light/Dark. Capture: 387 glyphs, missing=0,
client 1590×1140; снимки Light/Dark просмотрены, пиксели двух backend совпадают.
Основное окно с RibbonStatusBar: ещё два DX12 capture/smoke Light/Dark,
311 glyphs, missing=0. Дополнительный controls-gallery DX12 smoke PASS.
Smoke подтверждает аппаратную отрисовку, native resize и redraw.

Системный ввод и действия фокуса не генерировались. Настоящие клики,
перетаскивание, IME, screen reader, mixed-DPI и Linux/macOS runtime не проверены.
Контракт — [строки состояния](status-bars.md).

## Варианты групп и вкладки — Windows, 2026-10-15

Расширенные GroupBox/Expander и TabbedGroup проверены локально: 845 workspace
tests и 2 doctests PASS; fmt, clippy all-targets, release, rustdoc с запретом
warnings, licenses/inventory PASS. Добавлено 38 регрессий; два hardware unit
tests остаются ignored. Совместимость прежней геометрии TabControl проверена
отдельной регрессией и всеми 18 неизменёнными тестами controls-gallery.

NVIDIA GeForce RTX 3090, scale=1.5: DX12/Vulkan × Light/Dark, четыре capture
и четыре smoke PASS. Capture: 1040 glyphs, missing=0, client 1800×1320.
Снимки обеих тем просмотрены. Дополнительные DX12 smoke для controls-gallery
(100000 строк), основной формы и формы редакторов PASS, resize/redraw пройдены.
Оконные проверки не генерировали мышь/клавиатуру или действия фокуса.
Native input, IME, screen reader, mixed-DPI и Linux/macOS остаются отдельными
проверками. Контракт — [группы и вкладки](groups.md).

## Группы форм — Windows, 2026-10-15

`GroupBox`, `Expander` и пример `xaml-form --groups` проверены локально:
807 workspace tests и 2 doctests PASS; fmt, clippy, release, rustdoc без warnings,
проверки лицензий и зависимостей пройдены. Добавлено 38 регрессий; два отдельных
hardware unit tests остаются ignored.

На NVIDIA GeForce RTX 3090 выполнены DX12/Vulkan × Light/Dark: четыре
capture-прогона и четыре GPU smoke с resize/redraw, все exit 0. В каждом
capture 717 glyphs, missing=0, scale=1.5. Снимки светлой и тёмной форм просмотрены.
Основное окно с Ribbon/GroupBox прошло дополнительный DX12 Light capture/smoke;
форма `--editors` — DX12 Light smoke.

Системный ввод и действия фокуса при оконной проверке не генерировались.
Клики, клавиши, focus/IME lifecycle и accessibility projection покрыты
unit/integration-тестами; native input, экранный диктор, IME и mixed-DPI
остаются отдельными проверками. Linux/macOS этим срезом не проверены.
Контракт — [группы и раскрывающиеся секции](groups.md).

## Формы и данные — Windows, 2026-10-15

Новый пример `xaml-form --editors` проверен локально: 769 workspace tests,
2 doctests, fmt, clippy, release и rustdoc с запретом warnings пройдены.
Два отдельных hardware unit tests остаются ignored.

На NVIDIA GeForce RTX 3090 выполнены DX12/Vulkan × Light/Dark: четыре
оконных capture-прогона и четыре smoke-прогона с native resize/redraw,
все exit 0. Снимки Light/Dark просмотрены; 600 glyphs, missing=0, scale 1.5.
Системный ввод при проверке не генерировался. Native клики, popup keyboard,
IME, screen reader и mixed-DPI для новых редакторов ещё требуют проверки.
Linux/macOS этим срезом не проверены. Контракты — [формы и привязки](forms.md).


Дата состояния: 2026-10-09. Матрица относится к текущему `controls-gallery`.
Исторические Windows GPU и трёхплатформенные CI проверки M1 сохранены в
[M1_TEST_REPORT](M1_TEST_REPORT.md); они не подменяют результаты нового gallery.
«Не проверено» означает отсутствие результата, а не известный отказ.

## Зафиксированное состояние

| Проверка | Windows | Linux X11 | Linux Wayland | macOS |
|---|---|---|---|---|
| Новый source: локальные workspace tests | 141 tests + 1 doctest PASS | Не запускались локально | Не запускались локально | Не запускались локально |
| Полный финальный workspace CI alpha | PASS | PASS | Тот же Linux compile job | PASS |
| Gallery GPU: text/resize/redraw | Vulkan и DX12 PASS, RTX 3090, scale 1.5 | Не проверено | Не проверено | Не проверено |
| Native posted mouse gallery scenario | PASS: pages/Ribbon/theme/resize/close | Не проверено | Не проверено | Не проверено |
| Native IME preedit/candidate/commit/cancel | Не проверено | Не проверено | Не проверено | Не проверено |
| Screen reader и platform accessibility actions | Не проверено | Не проверено | Не проверено | Не проверено |
| Clipboard между приложениями | Не проверено | Не проверено | Не проверено | Не проверено |
| DPI между мониторами, minimize/restore, длительный idle | Не проверено для gallery | Не проверено | Не проверено | Не проверено |
| Физический device loss | Не проверено | Не проверено | Не проверено | Не проверено |

Text/layout/controls unit tests проверяют CPU модели и shaped geometry без окна.
[Публичный CI](https://github.com/S-typy/rust-ui-engine/actions/runs/37947371371)
на source `43c6de1` прошёл полностью на Windows, Linux и macOS: fmt, clippy,
tests, release build и rustdoc. Это не GPU/IME runtime gate.
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
