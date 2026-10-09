# Alpha candidate verification

Date: 2026-10-09. Version: 0.1.0. This report distinguishes executable source
checks from native acceptance; it does not announce a binary release.

## Local checks

The locked workspace passes formatting, clippy with warnings denied, 141 unit
and integration tests, and one doctest. The tests include Unicode/IME document
state, bidi carets, layout geometry, Ribbon overflow/keyboard/commands,
virtualized grid editing and stale async responses, connected accessibility,
ordered rectangle/text drawing and atlas memory limits.

Dependency audit: 349 registry packages; `cargo deny` advisories, bans, licenses
and sources pass. Multiple dependency versions remain warnings; license evidence
and distribution limitations are recorded in [dependencies](dependencies.md).

The optimized workspace release build and rustdoc with warnings denied also
pass. Platform CI is running. CI compilation does not establish successful
native window or IME operation.

## Native Windows evidence

Windows, NVIDIA GeForce RTX 3090, 144 DPI (150%): both Vulkan and DirectX 12
present real GPU text with zero missing glyphs in the exercised gallery scenes.
The native message harness switches Library → Controls, selects the View Ribbon
tab and Dark theme, resizes the client from 1800×1200 to 1440×1020 physical
pixels and closes with exit code 0. Five client-only PrintWindow captures per
backend verify nonempty raster text and canvas colors. The Library, Light/Dark
Controls and resized Dark captures were visually reviewed for readability,
group-label clipping and grid-header order.

The same native scenario also passes for the optimized release executable on
both backends, including the final host IME-state changes. It does not exercise
native IME composition itself.

Initial checks found unavailable triangle glyphs, clipped group captions, a
panel label over the grid header and a late press after window blur. These are
fixed. Capture sequencing and redraw checks were corrected in the harness.
Cross-platform CI also exposed a system-font line-height mismatch: Ribbon now
measures captions and buttons using the active font. Regression tests cover
all three themes. Accessibility Click now shares the Ribbon/menu activation
path. Pending queries, repeated edit commands and viewport resizing preserve
cell-edit drafts and validation errors; hidden editors expose no IME caret or
accessibility node, and restoring the view does not take focus from other controls.
Some long Ribbon captions still clip to their bounded controls; visual hover
tooltips are not implemented.

These are posted events directed only at the test application's own HWND, with
temporary cursor positioning restored afterwards. They are not physical IME,
global keyboard, user clipboard or screen-reader tests.

```powershell
$env:WGPU_BACKEND = 'dx12' # repeat with vulkan
python tests/windows_gallery_smoke.py --exe target/release/controls-gallery.exe --output-dir target/gallery-smoke-dx12
```

## Performance and pending acceptance

The [TreeGrid benchmark](treegrid.md) measures CPU viewport Scene construction
for 1k/100k/1M rows. Each case keeps at most 29 materialized rows, 10 visible
columns and 250 source cell reads in the measured viewport. These numbers do
not represent GPU frame latency or end-to-end input p95/p99.

Native IME candidate placement/commit/cancel, external clipboard exchange,
screen readers, monitor DPI transitions and Linux/macOS GPU runtime remain
unverified. The complete next-test protocol is in the [runtime matrix](runtime-matrix.md).
Fixed row heights, plain text editing and the remaining rendering/API bounds
are documented in the [alpha notes](alpha-release-notes.md).
