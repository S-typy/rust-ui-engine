# M1 verification report

Date: 2026-10-09. M1 adds the retained tree/runtime and the Taffy layout adapter
to the existing GPU rectangle renderer. The earlier [M0 report](TEST_REPORT.md)
remains a historical record. The [API contract](retained-ui.md) and
[ADR-003](adr/ADR-003-retained-runtime.md) describe the implemented scope.

## Local Rust checks

Windows 11 Pro, Rust/Cargo 1.96.0, MSVC target. Dependencies are locked.

| Check | Result |
|---|---|
| `cargo check --workspace --all-targets --locked` | PASS |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | 61 unit tests PASS |
| `cargo build --workspace --release --locked` | PASS |
| `cargo doc --workspace --no-deps --locked`, `RUSTDOCFLAGS=-D warnings` | PASS |
| `cargo deny`: licenses, sources, bans, advisories | PASS, 15 duplicate-version warnings |

Tests comprise 32 core, 15 layout, 8 shell/demo and 6 renderer tests. Coverage
includes stale/foreign IDs, generation overflow, subtree removal, atomic error
rejection, event route order, deferred mutation, focus traversal, capture cleanup,
clipping/z order/scroll/translation, layout constraints, layout error recovery,
paint-only updates and idle reuse. The runnable API example was compiled and run
with its focus/capture/idle assertions; it produced five rectangles.

The dependency inventory contains 239 external packages, adding Taffy 0.14.0 and
slotmap 1.1.1 to M0. Core has no external dependencies; layout has no wgpu/winit
dependency. See the [dependency audit](dependencies.md) for license evidence,
feature scope and existing binary-distribution limitations.

## GPU and native input

Hardware: NVIDIA GeForce RTX 3090, Windows 11 Pro build 26200, 150% scale
(144 DPI). DX12 driver: `32.0.16.1692`; Vulkan driver: NVIDIA `616.92`.

The retained built-in smoke passed on both DX12 and Vulkan: three presented
frames, native resize to 960×640 logical pixels, two activations, removal of C,
cleared focus/capture and scroll 84. Final counters were three layout passes
and nine paint passes. This mode dispatches normalized synthetic input directly.

The [native retained harness](retained-smoke.md) passed on both backends:
hover and focus repaint without layout; primary down/up capture and release;
Tab/Enter activation; removal of the focused node; wheel without layout; blur
cleanup; native resize/minimize/restore; idle counters and successful close.
The final idle state had 37 nodes, two activations, one removal, scroll 84 and
no focus/capture. Both runs performed five layout passes, including zero-size
and restored viewports; frame/paint counts vary with native event scheduling.

Both 1350×900 client screenshots were captured through PrintWindow, passed
color checkpoints, and were visually inspected: A/B remain, C is absent, the
navigation panel and scrolled rows stay within their clips. The harness
temporarily raises its own window without activation, aligns the OS cursor,
and posts synthetic focus/key/pointer messages. Cursor restoration succeeded
in both runs. It does not prove real foreground focus transfer or physical input.

The original rectangle mode also passed built-in DX12/Vulkan smoke and the
legacy native DX12 harness after the default demo changed to retained mode.
The retained Python harness parser, key-message, capture-validation and cleanup
fixtures passed separately.

## Cross-platform CI

The public [M1 CI run](https://github.com/S-typy/rust-ui-engine/actions/runs/37936243724)
completed successfully on source snapshot
`7059808b3993a94cf2f8980e565f9d1786827d16`:

| Runner | Formatting, Clippy, unit/doc tests, release build, rustdoc |
|---|---|
| Windows Server 2025 | PASS |
| Ubuntu 24.04 | PASS |
| macOS 15 | PASS |

All jobs include the new layout crate and all 61 unit tests. These are build
and CPU test results; the workflow does not run a native GPU window. Subsequent
report-only updates do not change the Rust source tested by this run.

## Limits

GPU runtime has been exercised on the Windows hardware named above. Linux
Vulkan/X11/Wayland and macOS Metal window runs have not been performed; their
runtime part of the cross-platform milestone gate remains open even when CI
builds pass. Native smoke uses synthetic messages, not human input. Device-loss
injection, monitor-to-monitor DPI changes, text, IME and accessibility are not
covered by these results.

M1 supplies primitives and runtime foundations. Layout supports single-line
Stack/Flex and sized Overlay, with a maximum depth of 128. Grid, margins,
intrinsic Overlay sizing, incremental layout and dirty-region rendering remain
future work. Semantics invalidation is not an accessibility backend; the demo
tiles are not finished controls. No large-table performance claim is made.
