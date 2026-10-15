# Rust UI Engine

An experimental native desktop UI framework written in Rust for Windows, Linux
(X11 and Wayland), and macOS. It combines a retained core, Unicode text editing,
controls, an adaptive Ribbon, docking panels, and a virtualized TreeGrid. Rendering uses hardware
GPU backends through `wgpu`; core and control APIs have no GPU or window types.

The current source is a **0.1.0 alpha candidate**. M2–M5 functionality is available
for integration and joint testing; this does not establish release readiness on
all platforms. Native IME, screen-reader interaction, monitor DPI transitions and
Linux/macOS GPU execution still need the checks recorded in the
[runtime matrix](docs/public/runtime-matrix.md). No binary release is announced.

## Run the XAML form

The default [MainWindow.xaml](examples/xaml-form/MainWindow.xaml) combines a
command-oriented Ribbon with an editable form. Declare tabs, groups, quick-access
commands and large or small buttons in XAML; register command labels, availability,
toggle state and handlers in Rust. Split buttons and dropdowns support nested
command menus. Alt or F10 activates Ribbon KeyTips. As windows narrow, commands
reduce from large to small or icon-only presentations before groups collapse.

The Ribbon uses a Fluent presentation with large icons above two-line captions,
compact icon buttons in quick access, underlined tabs and a rounded command
surface with group separators. Split buttons share one surface with separate
action and menu areas. KeyTip badges leave command labels and layout unchanged.
The minimize button leaves quick access and tabs visible; selecting a tab opens
the commands temporarily, and Escape closes them. `MinSize` keeps selected commands
from reducing past their declared size.

Light and dark themes distinguish the window, Ribbon, cards, editors and popup
surfaces. Subtle hover and pressed states, aligned small commands and restrained
icon accents keep the Ribbon readable. Text blocks support font size, weight,
secondary text and wrapping; panels can use a theme-relative card surface.

On Windows, `TitleBar="Ribbon"` combines the title and quick-access commands
inside the native window frame. Caption buttons use native system actions with
the host's own vector symbols and retain their reserved
space; narrow headers move quick-access commands into overflow and shorten the
title. Use `--native-title-bar` for the standard title bar. Other platforms keep
their existing native decorations.

The compiled profile also provides nested stack panels,
labels, single-line edit boxes, buttons, toggle buttons, checkboxes and radio groups. Buttons
share theme-relative styling and Fluent hover, press, checked and focus states.
Radio groups use one Tab stop; arrow keys select a visible, enabled option.
Own vector icons work alone or inside buttons; horizontal and vertical separators
divide groups without entering the keyboard focus order.
Editors support light/dark
Fluent styling, placeholder text, Unicode editing and value validation.
Edit the XAML and rebuild:

```sh
cargo run --release --locked -p xaml-form
cargo run --release --locked -p xaml-form -- --dark
cargo run --release --locked -p xaml-form -- --ribbon-minimized
cargo run --release --locked -p xaml-form -- --controls
```

`--controls` opens [ControlsWindow.xaml](examples/xaml-form/ControlsWindow.xaml),
the separate example of individual controls and popup menus.

The build validates the markup and generates a Rust window definition. No .NET
runtime is required. This is a bounded, explicit control-tree profile, with no bindings,
hot reload or general WPF XAML compatibility yet. See the
[supported syntax and example](docs/public/xaml.md).

## Run the docking workspace

[DockWindow.xaml](examples/xaml-form/DockWindow.xaml) declares a document area,
tabbed panels and resizable splits. Panels can move within the workspace or open
in separate native GPU windows. Closing a floating window returns its panels to
the main workspace; closing a panel keeps its document available for restoration.
The same retained editors survive these placement changes.

```sh
cargo run --release --locked -p xaml-form -- --docking
cargo run --release --locked -p xaml-form -- --docking --float-pane Properties
cargo run --release --locked -p xaml-form -- --docking --dark
cargo run --release --locked -p xaml-form -- --docking --layout-file "layouts/workspace.json"
```

Drag tabs to reorder or dock beside another group; drag splitters to resize.
On Windows, tabs can also move directly between existing native windows.
Escape, focus loss and pointer capture loss cancel an unfinished drag.
The panel header and Ribbon provide floating, redocking and panel restoration.

Ribbon and QAT Save/Restore commands explicitly save or load placement JSON.
The default Windows file is `%LOCALAPPDATA%/rust-ui-engine/docking-layout.json`;
`--layout-file PATH` selects another file. Startup and close perform no layout
file I/O. The file preserves tabs, splits and floating bounds, without editor
contents; errors appear in the status bar.

Auto-hide and main-window geometry persistence remain outside this profile.
Exact placement restoration across differing monitor DPI is not guaranteed.
Direct native dragging, mixed-DPI behavior and the new Save/Restore window flow
still require runtime validation; Linux/macOS cross-window hit testing is not
implemented. See the [docking contract](docs/public/docking.md) for model,
host lifecycle, strict JSON limits and current validation boundaries.

## Run the gallery

Install Rust 1.96.0 and the native build tools: MSVC with Windows SDK, Xcode
command-line tools, or a Linux C toolchain. An interactive desktop, a compatible
hardware GPU, its driver and system fonts are required.

```sh
rustup toolchain install 1.96.0 --profile minimal --component rustfmt --component clippy
cargo run --release --locked -p controls-gallery -- --rows 100000
```

The gallery has Library, Controls and About pages. Library uses 24 columns,
indexed book data, lazy child loading, background filter/sort, pinned columns,
cell editing and versioned column state. Controls demonstrates buttons, toggles,
checkboxes, text editing, scrolling, menus and Light/Dark/Compact themes.

- Tab/Shift+Tab moves focus; Enter/Space activates controls.
- Alt or F10 opens Ribbon KeyTips; choose a tab and then a command.
- Grid arrows navigate and expand/collapse; Shift extends selection and Ctrl/Cmd
  modifies it. F2/Enter opens a cell editor, Enter commits, Escape cancels.
- Click a header to sort; drag it to reorder; drag its right edge to resize.
  Shift+wheel scrolls columns horizontally.
- TextBox supports selection, Ctrl/Cmd+A/C/X/V/Z, Ctrl+Y and Cmd+Shift+Z.

Choose the column-state file explicitly, or run the bounded GPU smoke scenario:

```sh
cargo run --release --locked -p controls-gallery -- --rows 100000 --state-file target/gallery-columns.txt
cargo run --release --locked -p controls-gallery -- --rows 100000 --smoke-test
```

Smoke opens a real window, presents frames, requests native resize and exits.
It checks presentation/lifecycle, not native text entry, screen-reader behavior or
visual correctness. See the [runtime matrix](docs/public/runtime-matrix.md).

Reference dependencies for Ubuntu 24.04 builds:

```sh
sudo apt-get update
sudo apt-get install --yes --no-install-recommends \
  build-essential pkg-config libfontconfig1-dev fonts-dejavu-core fonts-noto-core \
  libx11-6 libx11-xcb1 libxcb1 libxcursor1 libxi6 libxrandr2 \
  libxkbcommon0 libxkbcommon-x11-0 \
  libwayland-client0 libwayland-cursor0 libvulkan1
```

Install additional fonts for CJK/emoji as needed. The Vulkan loader alone does not
install a hardware driver. Wayland clipboard support depends on compositor
protocols; client-side window decorations are not implemented.

## Workspace

| Path | Responsibility |
|---|---|
| `crates/ui-core` | Generational tree, routed input, focus/capture, invalidation, geometry and ordered Scene; no external dependencies |
| `crates/ui-layout` | Taffy Stack/Flex/Grid/Overlay adapter and margin/padding constraints |
| `crates/ui-text` | Parley shaping, Swash glyph bitmaps, Unicode document editing and caret geometry |
| `crates/ui-controls` | Controls, themes, command registry, popups, adaptive Ribbon and docking model/view |
| `crates/ui-treegrid` | Indexed data-source contract, hierarchy/selection/edit state and viewport Scene generation |
| `crates/ui-render-wgpu` | Rectangle batches, glyph atlas, ordered GPU drawing and surface lifecycle |
| `crates/ui-platform-winit` | Single/multiple native windows, input, IME, clipboard and AccessKit host |
| `crates/ui-xaml` | Independent compiler for the Window and control-tree XAML profile |
| `examples/xaml-form` | Native Fluent form built from MainWindow.xaml |
| `examples/controls-gallery` | Integrated controls and book-library demonstration |
| `examples/gpu-shell` | Earlier retained and rectangle rendering diagnostics |

`Scene` carries ordered rectangles, rounded fills and logical-pixel text runs. The renderer
prepares real glyphs, uploads an RGBA atlas and applies DPI at its boundary.
Supported backends are D3D12, Vulkan and Metal. Set `WGPU_BACKEND=dx12`, `vulkan`
or `metal` where supported. CPU/Noop adapters are rejected; there is no software
UI rendering fallback. Inspect the reported adapter for virtual GPU environments.

Colors are straight-alpha sRGB at the scene boundary, decoded for linear GPU
blending and encoded once by the sRGB surface. Rounded fills have a uniform
corner radius and a rectangular ancestor clip; they do not clip descendants.

The retained runtime skips layout for paint/focus/scroll changes and reuses its
scene at idle. TreeGrid generates only viewport rows/columns with overscan; it
does not create a widget for every record. Its documented benchmark measures
CPU frame construction, not GPU FPS.

## Development and verification

As of 2026-10-14, GitHub Actions are temporarily paused. Development and
verification currently run locally on Windows with Rust 1.96.0. Run the following
commands in PowerShell; rustdoc warnings remain errors:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked
$env:RUSTDOCFLAGS = "-D warnings"
cargo doc --workspace --no-deps --locked
cargo bench -p rust-desktop-ui-treegrid --bench viewport --locked
```

Windows, Linux and macOS remain target platforms, with separate validation
required for releases on each platform. Historical cross-platform results remain
recorded for their source revisions. Compilation does not demonstrate GPU, IME
or accessibility behavior. Dependency versions are locked; the latest
recorded audit and distribution limitations are in [dependencies](docs/public/dependencies.md).

Earlier diagnostic examples remain available:

```sh
cargo run --release --locked -p gpu-shell
cargo run --release --locked -p gpu-shell -- --smoke-test
cargo run --release --locked -p gpu-shell -- --rectangles --smoke-test
```

The Windows M1 [retained harness](docs/public/retained-smoke.md) and M0
[rectangle harness](docs/public/windows-smoke.md) test those earlier scenarios.
Their historical success does not validate the newer gallery.

## Documentation

- [Architecture](docs/public/architecture.md), [retained runtime](docs/public/retained-ui.md), [layout](docs/public/layout.md)
- [Text](docs/public/text.md), [controls](docs/public/controls.md), [Ribbon](docs/public/ribbon.md), [TreeGrid](docs/public/treegrid.md)
- [Docking and floating windows](docs/public/docking.md)
- [Native host / DesktopApp](docs/public/platform.md)
- [Alpha candidate notes](docs/public/alpha-release-notes.md) and [runtime matrix](docs/public/runtime-matrix.md)
- [Subsystem decision](docs/public/adr/ADR-004-text-controls-platform-treegrid.md), [retained decision](docs/public/adr/ADR-003-retained-runtime.md)
- [Historical M1 checks](docs/public/M1_TEST_REPORT.md), [M0 checks](docs/public/TEST_REPORT.md), [requirements](docs/public/requirements.md)
- [Dependency audit](docs/public/dependencies.md) and [third-party notices](THIRD_PARTY_NOTICES.md)

Automatic intrinsic text measurement, incremental subtree layout and general
vector/image rendering remain incomplete. TreeGrid uses fixed row heights;
variable heights, grouping/summaries, advanced docking and PropertyGrid remain
outside this alpha scope. Alpha 0.1.0 contracts are documented; API compatibility before 1.0
is not guaranteed.

## License

Project code is [Apache-2.0](LICENSE). Dependencies retain their own licenses;
system fonts are used without copying or bundling font files. Binary distribution
requires the applicable third-party notices and platform SDK terms.
