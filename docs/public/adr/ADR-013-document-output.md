# ADR-013: immutable document pages and explicit output adapters

Date: 2026-10-20. Status: accepted.

Preview, PDF export and printer output need to share page geometry without
giving the platform layer ownership of report evaluation or editor state.

`ui-core` defines a bounded immutable `PrintDocument` over Scene values.
It has no PDF, report, GPU or operating-system dependency. `ui-controls`
owns workbook presentation, the PDF interpretation profile, report evaluation,
preview UI and text-aware output preparation. `ui-platform-winit` only owns
native printer enumeration and transport of prepared snapshots.

The application owns file dialogs, asynchronous I/O, cancellation, stale-result
checks and explicit print commands. It chooses adapters and handles diagnostics.
The window pipeline remains GPU-only. Preparing glyph images for a printer or
file does not add a software UI renderer.

PDF object parsing uses the MIT-licensed Rust `lopdf` library without default
features. The project implements its own bounded Scene interpreter. Unsupported
format features remain visible as errors/diagnostics, not implied compatibility.
Shared pages permit future exporters without duplicating pagination.

The current native printing adapter is Windows-only. Actual device output and
native accessibility require independent acceptance beyond compilation and
page-scene tests.
