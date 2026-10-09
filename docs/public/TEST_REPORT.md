# M0 — отчёт проверок

Дата: 2026-10-09. Область: три пакета workspace, GPU shell и сцена
прямоугольников. Этот отчёт не подтверждает готовность UI-компонентов.

## Исходное состояние и исправления

Исходная заготовка содержала core со сценой и демонстрационным состоянием,
wgpu renderer, winit shell и четыре unit-теста. Не было lockfile, закреплённого
toolchain и CI. Первый `cargo fmt --all -- --check` обнаружил форматирование;
первый `cargo check --workspace --all-targets` завершился с E0063:
`RequestAdapterOptions` для wgpu 30 требовал `apply_limit_buckets`.

Исправлены инициализация API, обработка surface/device lifecycle и ошибок,
resize/DPI/input. Демонстрация перенесена из core в пример. Версии и features
закреплены; добавлены проверки, CI и документация. Точный перечень изменений
описан в [M0_CHANGES.md](M0_CHANGES.md), решения — в
[ADR-001](adr/ADR-001-retained-tree.md) и [ADR-002](adr/ADR-002-scene-wgpu.md).

## Локальный стенд

| Параметр | Значение |
|---|---|
| ОС | Windows 11 Pro, 10.0.26200, build 26200 |
| Target | x86_64-pc-windows-msvc |
| Rust | rustc 1.96.0 (ac68faa20 2026-05-25) |
| Cargo | cargo 1.96.0 (30a34c682 2026-05-25) |
| GPU | NVIDIA GeForce RTX 3090, DiscreteGpu |
| Driver | Windows 32.0.16.1692; Vulkan сообщает NVIDIA 616.92 |
| DPI | 144 DPI, scale factor 1.5 (150%) |
| Python | 3.13.2 |

## Проверки сборки и зависимостей

| Команда | Результат |
|---|---|
| `cargo check --workspace --all-targets --locked` | PASS |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS, предупреждений нет |
| `cargo test --workspace --locked` | PASS, 14 unit-тестов; doc-test targets выполнены, примеров doc-test пока 0 |
| `cargo build --workspace --release --locked` | PASS |
| `cargo doc --workspace --no-deps --locked`, `RUSTDOCFLAGS=-D warnings` | PASS |
| `cargo tree -p rust-desktop-ui-core --locked` | Внешних зависимостей нет |
| `cargo deny --locked check licenses sources bans advisories` | PASS, 15 предупреждений о дублирующихся версиях |

Unit-тесты проверяют half-open geometry, отбор пустых прямоугольников,
ограничение размера сцены viewport, selection/scroll/clamping, дробное колесо
и DPI, zero-size/restore и переходы состояния surface. Они не создают GPU device.
Аудит зависимостей выполнен cargo-deny 0.19.9 по полному metadata graph:
237 внешних crates. `cargo-audit` отдельно не запускался. Подробнее —
[dependencies.md](dependencies.md).

## GPU runtime

Встроенный `gpu-shell --smoke-test` прошёл на **D3D12 и Vulkan**: adapter RTX 3090,
первый кадр — 167 прямоугольников, resize с 1875×1200 до 1440×960 физических
пикселей, три представленных кадра, `tab=1`, `selected=2`, `first_row=12`, exit 0.
Resize выполнен оконной системой; click/wheel в этом сценарии переданы напрямую
в обработчики приложения как synthetic input.

Дополнительный `tests/windows_smoke.py` прошёл на **обоих backend**:
первый кадр, tab click, row click, wheel (first_row=3), resize до 1350×900,
minimize/restore, сохранение tab=1/selected=2/first_row=3, отсутствие непрерывной
перерисовки в трёх idle-интервалах по 0,75 s и закрытие через WM_CLOSE с exit 0.
Это синтетические Win32-сообщения, обработанные настоящим оконным event loop.
Снимки клиентской области через PrintWindow прошли контроль цветовых точек
и визуальную проверку: видна ожидаемая сцена прямоугольников, активная вкладка,
разделители и строки. Текст в сцене отсутствует по текущей реализации.

Первый вариант harness мог выбрать вспомогательное окно Vulkan с пустым
заголовком; выбор исправлен на PID + заголовок приложения. Захват через DC
оказался зависим от перекрывающих окон и заменён PrintWindow с проверкой
содержимого в отдельном процессе с timeout. Финальные результаты выше получены
после исправления harness, а не после изменения GPU rendering.

Отрицательные проверки завершились ожидаемым exit 1 без panic и без CPU fallback:

- `WGPU_BACKEND=metal` на Windows: `No compatible GPU adapter`, выбранный backend
  недоступен в текущей сборке/платформе; представленных кадров 0.
- `WGPU_BACKEND=vulkan` и `VK_DRIVER_FILES`/`VK_ICD_FILENAMES`, указывающие на
  несуществующий driver manifest: `Cannot create GPU surface: Failed to create
  surface for any enabled backend`; представленных кадров 0. Переменные заданы
  только процессу проверки; системный драйвер не менялся.

Последний сценарий проверяет недоступность Vulkan через изоляцию discovery,
а не физическое отключение видеокарты. Механизм описан в
[Vulkan Loader documentation](https://github.com/KhronosGroup/Vulkan-Loader/blob/main/docs/LoaderDriverInterface.md#overriding-the-default-driver-discovery).

## CI и границы подтверждения

Матрица CI на 2026-10-09 завершилась успешно для снимка
[`e5002ce`](https://github.com/S-typy/rust-ui-engine/commit/e5002cebc4634562f8f76bc9569401e6fd5c8aee):
[GitHub Actions run 37929355178](https://github.com/S-typy/rust-ui-engine/actions/runs/37929355178).

| Runner | fmt / clippy / tests / release / rustdoc | GPU окно |
|---|---|---|
| Ubuntu 24.04 | PASS | Не запускалось |
| Windows Server 2025 | PASS | Не запускалось |
| macOS 15 | PASS | Не запускалось |

Использован закреплённый Rust 1.96.0 и Cargo.lock. В CI нет desktop GPU smoke;
он учитывается отдельно по локальному стенду выше.

GPU runtime Linux X11/Wayland и macOS Metal **не проверен**. На Windows не
проверены физический mouse/keyboard input, переходы между мониторами с разным
DPI, IME, accessibility и реальная аппаратная потеря device. 100/125/200% DPI
не проверены. Performance benchmark не проводился.

## Технический долг

- Retained tree, layout, focus, события виджетов и invalidation относятся к M1.
- Текст, glyph rendering, IME, accessibility, Ribbon и TreeGrid не реализованы.
- Scene пока не имеет clips/transforms/images; batch ограничен 16 384 rects.
- Wayland без server-side decorations может показать окно без рамки.
- Реальная потеря GPU и восстановление требуют отдельного стенда и сценария.
- Перед бинарной поставкой остаются license attribution `dispatch 0.2.0`,
  условия Apple SDK и комплект текстов лицензий; см. third-party notices.
