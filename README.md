# Rust UI Engine

An experimental native desktop UI framework written in Rust, targeting Windows,
Linux (X11 and Wayland), and macOS. Rendering uses `wgpu`; the core scene model
has no graphics or windowing dependencies.

The M0 prototype contains rectangle primitives, an instanced GPU renderer, and
a native window example with selection and scrolling over 100,000 logical row
indices. These are demonstration shapes. Retained widgets, layout, text, IME,
accessibility, Ribbon, and TreeGrid remain future work. The row count is not a
performance benchmark.

Actual build, CI, and GPU runtime results are recorded separately in the
[test report](docs/public/TEST_REPORT.md). A build on an operating system does
not establish GPU runtime support on that system.

## Workspace

| Path | Purpose |
| --- | --- |
| `crates/ui-core` | `Rect`, `Color`, `SolidRect`, and `Scene`; no external dependencies |
| `crates/ui-render-wgpu` | GPU rectangle renderer, native surface integration, typed errors and frame outcomes |
| `examples/gpu-shell` | Window lifecycle, input, demonstration state, and bounded GPU recovery |
| `tests/windows_smoke.py` | Windows window/input/runtime test harness |
| `docs/public` | Product architecture, decisions, dependency audit, and verification reports |

`Scene` contains rectangles in logical pixels. `GpuRenderer` applies DPI scaling
and returns `Presented`, `Skipped`, or `RecoveryRequired` through `RenderOutcome`.
GPU and window types stay outside `ui-core`. The current batch limit is 16,384
rectangles; paths, glyphs, images, and clipping are not implemented.

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

Click the top rectangles to switch the demonstration tab, click a row to select
it, and use the wheel to scroll. Resize the window or press Escape to close it.
The title reports presented frames, selected row, scroll position, and size.
The application logs the GPU adapter, backend, device type, and driver.

Dependencies are locked in `Cargo.lock`, with `wgpu 30.0.1` and `winit 0.30.13`
pinned explicitly. Enabled GPU backends are D3D12, Vulkan, and Metal. OpenGL,
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
resize, applies synthetic input directly to the demonstration handlers, checks
selection/scroll state, and exits:

```sh
cargo run --release --locked -p gpu-shell -- --smoke-test
```

A completed run prints `SMOKE PASS` and exits with code 0. Failure, timeout, or
closing the window before completion returns a nonzero status. This checks GPU
presentation and application state; it does not exercise physical mouse input
or verify rendered pixels.

For Windows, a separate Python 3.10+ harness sends synthetic Win32 mouse
messages through the native event loop and checks resize, minimize/restore,
idle redraw behavior, and closing. It saves logs, a JSON report, and a client
area screenshot when capture is supported:

```powershell
cargo build --workspace --release --locked
python tests/windows_smoke.py --exe target/release/gpu-shell.exe --output-dir target/windows-smoke
```

See the [Windows smoke procedure](docs/public/windows-smoke.md) for backend
selection, screenshot review, and limitations. Neither smoke mode establishes
IME, accessibility, monitor-to-monitor DPI transitions, or recovery from an
actual hardware device failure.

## Documentation

- [Architecture](docs/public/architecture.md) and [requirements](docs/public/requirements.md)
- [Retained tree decision](docs/public/adr/ADR-001-retained-tree.md) and [Scene/wgpu decision](docs/public/adr/ADR-002-scene-wgpu.md)
- [M0 changes](docs/public/M0_CHANGES.md) and [test report](docs/public/TEST_REPORT.md)
- [Dependency audit](docs/public/dependencies.md) and [third-party notices](THIRD_PARTY_NOTICES.md)

## License

The project is licensed under [Apache-2.0](LICENSE). Third-party dependencies
retain their own licenses. Audit scope, unresolved distribution requirements,
and upstream notices are recorded in the linked dependency documentation.
