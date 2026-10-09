# Зависимости

`Cargo.lock` фиксирует версии для воспроизводимой сборки. Проверки нужно запускать
с `--locked`: требования вида `1.23` и `0.4` сами по себе не фиксируют patch/minor
версию. Собственный код распространяется по Apache-2.0; зависимости сохраняют
свои лицензии, перечисленные в [THIRD_PARTY_NOTICES](../../THIRD_PARTY_NOTICES.md).

| Прямая зависимость | Требование | Назначение | Декларация лицензии |
|---|---|---|---|
| wgpu | `=30.0.1` | GPU API и перевод WGSL через Naga | MIT OR Apache-2.0 |
| winit | `=0.30.13` | Нативное окно и события ОС | Apache-2.0 |
| bytemuck | `^1.23`, `derive` | Проверяемое представление данных для GPU buffers | Zlib OR Apache-2.0 OR MIT |
| pollster | `^0.4` | Ожидание инициализации GPU в примере | Apache-2.0 OR MIT |
| taffy | `=0.14.0` | Расчёт layout за независимым API | MIT |

Версии и лицензии сверены с опубликованными пакетами:
[wgpu](https://docs.rs/crate/wgpu/30.0.1/source/Cargo.toml),
[winit](https://docs.rs/crate/winit/0.30.13/source/Cargo.toml),
[bytemuck](https://docs.rs/crate/bytemuck/1.25.2/source/Cargo.toml),
[pollster](https://docs.rs/crate/pollster/0.4.0/source/Cargo.toml),
[taffy](https://docs.rs/crate/taffy/0.14.0/source/Cargo.toml).
`pollster` использует старую запись `Apache-2.0/MIT`, эквивалентную выбору одной
из двух лицензий. Это преобразование отмечено отдельно в inventory.

## Features и платформенные границы

Для wgpu отключены default features и включены `std`, `parking_lot`, `dx12`,
`metal`, `vulkan`, `wgsl`. Обычные desktop backends — Direct3D 12, Metal и Vulkan.
OpenGL, WebGPU, ANGLE, MoltenVK и статическая поставка DXC не включены. Отсутствие
подходящего GPU должно приводить к явной диагностике приложения.

Для winit отключены default features и включены `rwh_06`, `x11`, `wayland`,
`wayland-dlopen`. Adwaita client-side decorations не поставляются. На Wayland
compositor без server-side decorations возможен запуск без рамки и кнопок
заголовка; собственная реализация декораций остаётся отдельной задачей.
`tiny-skia`, используемая необязательной Adwaita-рамкой winit, является отдельной
библиотекой на Rust, а не C++ Skia; выбранная конфигурация не включает ни её, ни
Adwaita. Описание feature flags:
[winit](https://docs.rs/crate/winit/0.30.13/source/Cargo.toml),
[wgpu](https://docs.rs/crate/wgpu/30.0.1/features).

`ui-core` не имеет внешних зависимостей. `ui-layout` зависит от него и Taffy:
включены только `std`, `taffy_tree`, `flexbox`, default features отключены.
Grid, Block, float layout, parser, serde и дополнительные алгоритмы Taffy не
включены. Активная ветка layout содержит `arrayvec`, `slotmap` и build dependency
`version_check`; wgpu/winit принадлежат renderer и примеру. Ни GPU-типы, ни
типы Taffy не входят в независимые API core.

Taffy и slotmap написаны на Rust. Taffy не имеет build script; `slotmap 1.1.1`
использует build.rs только для определения версии Rust и вывода Cargo cfg.
Native source/archive files в этих двух пакетах не обнаружены. Taffy не включает
готовые controls или визуальные ресурсы. Его crate-архив не содержит отдельного
LICENSE; текст MIT проверен по
[точной исходной ревизии](https://raw.githubusercontent.com/DioxusLabs/taffy/77f385683c1d698c91a23a259f87fdddf26925fb/LICENSE).
Slotmap декларирует Zlib и содержит
[LICENSE](https://docs.rs/crate/slotmap/1.1.1/source/LICENSE) в архиве.

## Native code и системные библиотеки

Реализация renderer и транслятор WGSL написаны на Rust. Нативный FFI остаётся
необходимым для окон, GPU и системных сервисов:

| Платформа | Интерфейс | Внешняя реализация |
|---|---|---|
| Windows | `windows`, `windows-sys`, `ash` | Win32, DXGI/D3D12, системный shader compiler, установленный Vulkan loader и driver |
| Linux X11 | `x11-dl`, `x11rb`, `xkbcommon-dl`, `ash` | X11/XCB, xkbcommon, Vulkan loader и driver |
| Linux Wayland | `wayland-*`, `smithay-client-toolkit`, `xkbcommon-dl`, `ash` | Wayland client, xkbcommon, Vulkan loader и driver |
| macOS | `objc2-*`, `block2`, Core Foundation/Graphics bindings | AppKit/Foundation, Metal, QuartzCore и Objective-C runtime |

Это системные API/драйверы, а не включение готового UI-фреймворка в renderer.
`wgpu-hal` также содержит Rust bindings `renderdoc-sys` для работы с уже
загруженным инструментом захвата GPU; RenderDoc не скачивается и не включается
в поставку. При использовании внешнего shader compiler или инструмента захвата
их собственные условия поставки действуют отдельно.

All-target dependency graph может содержать Android, iOS, WebAssembly и Redox
пакеты. Их присутствие в lockfile не означает поддержку этих платформ или
включение их кода в desktop binary. Native source files в inventory — результат
поиска файлов пакета, включая tests/examples; наличие файла не доказывает его
компиляцию. Пакет `cc` — сборочная утилита, а не доказательство C++ UI renderer.

В проверенных исходниках `android-activity 0.6.1` содержит C/C++ glue для
Android GameActivity; feature `game-activity` не включён, а Android не является
целевой платформой проекта. C-файлы `wayland-backend 0.3.17` используются
неактивным feature `log`; Objective-C shim `objc-sys 0.3.5` — неактивным
`unstable-exception`. Windows target crates содержат штатные import libraries.
Это результат просмотра manifest/build scripts, а не измерение состава binary.

## Воспроизводимая проверка

```text
cargo metadata --locked --format-version 1
cargo tree --locked --workspace --target all -e features
cargo tree --locked --target all -p rust-desktop-ui-layout -e features
cargo deny --locked check licenses sources bans advisories
```

Полный список crates, исходные SPDX-декларации, SHA-256 лицензий и features из
`cargo metadata` содержатся в [dependency-inventory.json](dependency-inventory.json).
Metadata описывает разрешённый граф всех платформ. Активные features конкретной
сборки проверяются отдельно через `cargo tree`: наличие опционального ребра в
metadata не доказывает компиляцию этой зависимости в выбранном пакете.
Проверка по `deny.toml` выявляет запрещённые лицензии, registries, известные
уязвимости и отдельные запрещённые зависимости. Список запрещённых имён не
заменяет проверку назначения новой библиотеки; успешная автоматическая проверка
не равна проверке всех исходников и условий бинарной поставки.

На 2026-10-09 `cargo-deny 0.19.9` завершил проверки licenses, sources, bans и
advisories без ошибок: 239 сторонних crates, 15 предупреждений о нескольких
версиях транзитивных зависимостей. Проверена актуализированная RustSec database
на revision `7eebec69c352c7191b1f13eb95dd510eeca5d1de`. `cargo-audit` отдельно
не запускался. Из 239 пакетов у 204 найдены license/notice files в архиве,
для ещё 34 материалы сверены с точным upstream revision. Для
`dispatch 0.2.0` доступна только декларация MIT; отсутствие отдельного текста
отмечено в notices. Также остаются условия Apple SDK, перечисленные upstream
objc2: эти вопросы должны быть закрыты перед распространением macOS binaries.

Upstream wgpu 30.0.1 декларирует Rust 1.87, winit 0.30.13 — Rust 1.70,
taffy 0.14.0 — Rust 1.71, slotmap 1.1.1 — Rust 1.58. Это
не MSRV всего workspace: например, resolved `ordered-float 5.5.0` требует
Rust 1.90. Workspace устанавливает `rust-version = "1.96"`, toolchain закреплён
на 1.96.0 в `rust-toolchain.toml`. Более низкий MSRV не заявляется.
