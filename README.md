# Rust UI Engine

An experimental native desktop UI framework written in Rust for Windows, Linux
(X11 and Wayland), and macOS. It combines a retained core, Unicode text editing,
controls, an adaptive Ribbon, and a virtualized TreeGrid. Rendering uses hardware
GPU backends through `wgpu`; core and control APIs have no GPU or window types.

The current source is a **0.1.0 alpha candidate**. M2–M5 functionality is available
for integration and joint testing; this does not establish release readiness on
all platforms. Native IME, screen-reader interaction, monitor DPI transitions and
Linux/macOS GPU execution still need the checks recorded in the
[runtime matrix](docs/public/runtime-matrix.md). No binary release is announced.

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

Ubuntu 24.04 dependencies used by the build workflow:

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
| `crates/ui-controls` | Controls, themes, command registry, popups and adaptive Ribbon |
| `crates/ui-treegrid` | Indexed data-source contract, hierarchy/selection/edit state and viewport Scene generation |
| `crates/ui-render-wgpu` | Rectangle batches, glyph atlas, ordered GPU drawing and surface lifecycle |
| `crates/ui-platform-winit` | Window/input normalization, IME, clipboard and AccessKit host |
| `examples/controls-gallery` | Integrated controls and book-library demonstration |
| `examples/gpu-shell` | Earlier retained and rectangle rendering diagnostics |

`Scene` carries ordered rectangles and logical-pixel text runs. The renderer
prepares real glyphs, uploads an RGBA atlas and applies DPI at its boundary.
Supported backends are D3D12, Vulkan and Metal. Set `WGPU_BACKEND=dx12`, `vulkan`
or `metal` where supported. CPU/Noop adapters are rejected; there is no software
UI rendering fallback. Inspect the reported adapter for virtual GPU environments.

The retained runtime skips layout for paint/focus/scroll changes and reuses its
scene at idle. TreeGrid generates only viewport rows/columns with overscan; it
does not create a widget for every record. Its documented benchmark measures
CPU frame construction, not GPU FPS.

## Development and verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked
cargo doc --workspace --no-deps --locked
cargo bench -p rust-desktop-ui-treegrid --bench viewport --locked
```

CI uses Rust 1.96.0 on Windows/Linux/macOS and treats rustdoc warnings as errors.
A configured workflow is not a test result; compilation does not demonstrate
GPU, IME or accessibility behavior. Dependency versions are locked; the latest
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
- [Native host / DesktopApp](docs/public/platform.md)
- [Alpha candidate notes](docs/public/alpha-release-notes.md) and [runtime matrix](docs/public/runtime-matrix.md)
- [Subsystem decision](docs/public/adr/ADR-004-text-controls-platform-treegrid.md), [retained decision](docs/public/adr/ADR-003-retained-runtime.md)
- [Historical M1 checks](docs/public/M1_TEST_REPORT.md), [M0 checks](docs/public/TEST_REPORT.md), [requirements](docs/public/requirements.md)
- [Dependency audit](docs/public/dependencies.md) and [third-party notices](THIRD_PARTY_NOTICES.md)

Automatic intrinsic text measurement, incremental subtree layout and general
vector/image rendering remain incomplete. TreeGrid uses fixed row heights;
variable heights, grouping/summaries, Docking and PropertyGrid are outside this
alpha scope. Alpha 0.1.0 contracts are documented; API compatibility before 1.0
is not guaranteed.

## License

Project code is [Apache-2.0](LICENSE). Dependencies retain their own licenses;
system fonts are used without copying or bundling font files. Binary distribution
requires the applicable third-party notices and platform SDK terms.
