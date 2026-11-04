# ADR-017: windowless Scene output for printer transport

Date: 2026-11-04. Status: accepted.

Printer drivers can reject alpha-blended glyph images even when their spooler
accepts ordinary opaque bitmaps. Immutable page content must remain usable
without depending on driver text shaping or changing document layout.

`ui-render-wgpu` owns `OffscreenRenderer` and a common internal frame renderer
shared with window surfaces. Scene validation, glyphs, image caches, winding,
clipping and blending remain in this layer. It renders into an opaque sRGB
bitmap and performs bounded GPU readback on a worker. No Window or CPU drawing
backend is introduced. `ui-core` and `ui-controls` gain no GPU dependency.

This extends ADR-013: the Windows transport may pass original immutable page
Scenes to `OffscreenRenderer` before sending opaque pixels to the driver.
The existing platform dependency on the GPU renderer supplies this capability;
the platform still owns neither pagination nor mutable document/editor state.
Simple opaque vector pages can retain the GDI path. An explicit native policy
preserves the previous prepared-image profile and its capability checks.

Raster DPI and memory limits are explicit. A GPU failure returns an error;
there is no silent software fallback or resolution reduction. The first GPU
page is rendered before StartDoc, later failures abort the current job. Driver
calls remain potentially blocking and cancellation cannot retract pages already
printed. Independent PDF-file and virtual-printer checks are separate from
physical printer, other-platform and accessibility acceptance.
