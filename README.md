# Rust UI Engine

An experimental native desktop UI framework written in Rust, targeting Windows,
Linux (X11 and Wayland), and macOS. Rendering uses `wgpu`; the retained core
has no graphics or windowing dependencies.

M1 implements generational widget identity, a retained tree, routed input,
focus, pointer capture, separate invalidation flags, and Stack/Overlay/Flex
layout through an internal Taffy adapter. The default example renders retained
rectangle primitives with hover/focus feedback and clipped scrolling. These
primitives are not finished controls: text, IME, accessibility integration,
Ribbon, and TreeGrid remain future work.

Actual build, CI, and GPU runtime results are recorded separately in the
[M1 test report](docs/public/M1_TEST_REPORT.md). The [M0 report](docs/public/TEST_REPORT.md)
preserves the earlier renderer checks. A build on an operating system does not
establish GPU runtime support on that system.

## Workspace

| Path | Purpose |
| --- | --- |
| `crates/ui-core` | Generational arena/tree, input routing, focus/capture, invalidation, independent layout contracts, geometry, and Scene; no external dependencies |
| `crates/ui-layout` | Taffy adapter for Stack, Overlay, and Flex; no GPU/window dependency |
| `crates/ui-render-wgpu` | GPU rectangle renderer, native surface integration, typed errors and frame outcomes |
| `examples/gpu-shell` | Window lifecycle, input, demonstration state, and bounded GPU recovery |
| `tests/windows_retained_smoke.py` | Retained demo native Windows input/lifecycle checks |
| `tests/windows_smoke.py` | Original rectangle demo checks and shared Win32 helpers |
| `docs/public` | Product architecture, decisions, dependency audit, and verification reports |

`Scene` contains rectangles in logical pixels. `GpuRenderer` applies DPI scaling
and returns `Presented`, `Skipped`, or `RecoveryRequired` through `RenderOutcome`.
GPU and window types stay outside `ui-core`. Runtime resolves translation,
scroll, ancestor clips, and stable z-order before producing the rectangle batch.
Axis-aligned clipping crops rectangle geometry; there is no general GPU clip
stack. The current batch limit is 16,384 rectangles. Paths, glyphs, images,
rotations, and opacity layers are not implemented.

## Development

Install Rust through rustup and the native build tools for your platform:
MSVC build tools with the Windows SDK, Xcode command-line tools on macOS, or
a C linker/toolchain on Linux. The workspace pins Rust **1.96.0** with rustfmt
and clippy in `rust-toolchain.toml`; no lower MSRV is claimed.

On Ubuntu 24.04, the CI configuration installs these build tools and dynamically
loaded platform libraries:

```sh
sudo apt-get update
sudo apt-get install --yes --no-install-recommends \
  build-essential pkg-config \
  libx11-6 libx11-xcb1 libxcb1 libxcursor1 libxi6 libxrandr2 \
  libxkbcommon0 libxkbcommon-x11-0 \
  libwayland-client0 libwayland-cursor0 libvulkan1
```

Running the example also requires an interactive desktop session and a
compatible hardware GPU with its vendor driver. The Vulkan loader package alone
does not provide a hardware driver.

```sh
rustup toolchain install 1.96.0 --profile minimal --component rustfmt --component clippy
cargo run --release --locked -p gpu-shell
```

The top blue, teal, and amber rectangles are identified as A, B, and C in
diagnostics; letters are not rendered. A primary press focuses and activates a
rectangle, increments the activation counter, and captures the pointer until
release. Use Tab/Shift+Tab to move focus and Enter/Space to activate a focused
rectangle. Delete removes C while C is focused. The right panel scrolls with
the wheel; when focused, it also accepts Up/Down, Home, and End. Resize the
window or press Escape to close it.

The title reports presented frames, layout/paint passes, focus, hover, capture,
activation/removal counts, retained node count, scroll offset, and size. Hover,
focus, and scroll reuse layout; an unchanged update does no layout or paint work.
The application logs the GPU adapter, backend, device type, and driver.

The original M0 shapes remain available separately:

```sh
cargo run --release --locked -p gpu-shell -- --rectangles
```

In that mode, click the top rectangles to switch tabs, click a row to select it,
and use the wheel to scroll through 100,000 logical row indices. It does not
instantiate 100,000 retained nodes and is not a TreeGrid or performance benchmark.

Dependencies are locked in `Cargo.lock`, with `wgpu 30.0.1`, `winit 0.30.13`, and
`taffy 0.14.0` pinned explicitly. Taffy enables only `std`, `taffy_tree`, and
`flexbox`; external layout types do not enter the core API.
Enabled GPU backends are D3D12, Vulkan, and Metal. OpenGL,
WebGPU, Vello, and software rendering are not provided. CPU and Noop adapters
are rejected; adapter details must still be inspected for virtual GPUs.
For a backend-specific run, set `WGPU_BACKEND` to `dx12`, `vulkan`, or `metal`
where supported by the current platform and driver.

X11 and Wayland are enabled on Linux. Adwaita window decorations are disabled;
Wayland compositors without server-side decorations may display a window without
a frame or title-bar buttons. Custom client-side decorations are not implemented.

Validation commands, matching the Windows/Linux/macOS CI workflow:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked
cargo doc --workspace --no-deps --locked
```

CI treats rustdoc warnings as errors through `RUSTDOCFLAGS="-D warnings"`.
It runs unit/documentation tests and builds the application; GPU window tests
are separate.

## GPU smoke tests

The built-in smoke mode opens a real window, presents frames, requests a native
resize, and dispatches synthetic input directly to the retained demonstration.
It checks two activations, removal of C, and a scroll offset of 84 logical pixels
before exiting:

```sh
cargo run --release --locked -p gpu-shell -- --smoke-test
```

A completed run prints `SMOKE PASS mode=retained` and exits with code 0. Failure, timeout, or
closing the window before completion returns a nonzero status. This checks GPU
presentation and application state; it does not exercise physical mouse input
or verify rendered pixels.

For Windows, a separate Python 3.10+ harness sends synthetic Win32 pointer and
keyboard messages through the native event loop. It checks focus, capture,
activation, removal, scrolling, layout/paint counters, resize, minimize/restore,
idle behavior, and closing. It saves logs, a JSON report, and a client-area
screenshot when capture is supported:

```powershell
cargo build --workspace --release --locked
python tests/windows_retained_smoke.py --exe target/release/gpu-shell.exe --output-dir target/windows-retained-smoke
```

See the [retained smoke procedure](docs/public/retained-smoke.md) for backend
selection, screenshot review, and limitations. The earlier renderer scenario uses
`gpu-shell --rectangles --smoke-test`; `tests/windows_smoke.py` selects
`--rectangles` automatically. Its [procedure](docs/public/windows-smoke.md) is
documented separately. Neither smoke mode establishes
IME, accessibility, monitor-to-monitor DPI transitions, or recovery from an
actual hardware device failure.

## Documentation

- [Architecture](docs/public/architecture.md) and [requirements](docs/public/requirements.md)
- [Retained API and layout contracts](docs/public/retained-ui.md)
- [Retained tree decision](docs/public/adr/ADR-001-retained-tree.md) and [Scene/wgpu decision](docs/public/adr/ADR-002-scene-wgpu.md)
- [Retained runtime decision](docs/public/adr/ADR-003-retained-runtime.md) and [M1 test report](docs/public/M1_TEST_REPORT.md)
- [M0 changes](docs/public/M0_CHANGES.md) and [historical test report](docs/public/TEST_REPORT.md)
- [Dependency audit](docs/public/dependencies.md) and [third-party notices](THIRD_PARTY_NOTICES.md)

## License

The project is licensed under [Apache-2.0](LICENSE). Third-party dependencies
retain their own licenses. Audit scope, unresolved distribution requirements,
and upstream notices are recorded in the linked dependency documentation.
