# Printing, preview and reports

`ui_core::pages::PrintDocument` is an immutable snapshot of `PrintPage` values:
page size and ordered Scene commands, in logical pixels (96 per inch).
Construction rejects invalid geometry/command indices, protected text and
resource excess: 512 pages, 100,000 commands, 16 MiB text and 128 MiB unique
bitmap payload. `PageViewer::print_preview`, PDF export and native printing
consume the same snapshot. See [ADR-013](adr/ADR-013-document-output.md).

`printing::from_rich_document` adapts paginated rich documents.
`ReportEngine::build` produces pages from a typed table. The Spreadsheet example
provides a separate tabular print adapter for at most 1,000 rows and 12 columns;
it rejects merged sheets and does not reproduce spreadsheet viewport styling.

## Export and Windows printing

`printing::export_pdf` writes page vectors and raster images. Text is shaped
with the normal text engine and exported as glyph images at 192 DPI with
`ActualText` marked content. Font binaries are not embedded. This preserves
visual glyphs but does not promise outline-text quality at arbitrary zoom or
text extraction in every external reader. Unsupported rounded geometry,
cropped image sources or transparent vector ink returns an error.
Cancellation returns no partial byte buffer.

`printing::prepare_for_printer` replaces text with glyph bitmaps.
`ui_platform_winit::printing::printers` enumerates configured printers without
opening one. `PrintTicket` selects an explicit printer, zero-based page indices,
copy count, fit-to-printable-area and an optional absolute output filename.
`print` uses Windows GDI/spooler APIs on a worker, validates the ticket and
driver capabilities before starting a job, and calls AbortDoc when cancelled
or failed. Cancellation cannot retract pages a device has already printed.
The current image transport requires driver pixel-alpha support; vectors must
be opaque and image source coordinates integral. Other platforms return
`Unsupported`.

Run `xaml-form --print-preview` or `--report`. Select “Принтеры” to enumerate,
enter/accept a printer name, then explicitly press “Печатать”. No job is
submitted automatically. Driver behavior, physical output, printer preferences,
native interaction and accessibility still require separate acceptance.
PDF generation and preview checks do not establish successful Windows printing.

## Report definitions

`ReportDefinition` contains page geometry, top/right/bottom/left margins,
typed parameters, parameter filters, a QueryRequest and seven bands:
report header, page header, group header, detail, group footer, report footer
and page footer. `ReportItem` supports text, rectangles and bitmaps.

Text values can be literals, typed row expressions, parameters, group keys,
group/global aggregates, page number or page count. QueryEngine owns filtering,
sorting, grouping, expression types and aggregate semantics. Diagnostics name
the band and item index. Row expressions are restricted to detail; invalid
references, types and group context are rejected.

The engine measures and wraps text, grows bands, and breaks before a band that
does not fit. A band larger than a printable body fails the whole build.
Page numbers/counts are resolved after pagination. Results are repeatable for
the same data, definition and font environment. Totals are computed over the
selected data, not truncated page slices.

Limits include 20,000 result rows, 256 definition items, 64 parameters and
512 pages, with incremental command/text budgets. Cancellation returns an
error rather than a misleading partial report. Query evaluation itself has its
own bounded execution; cancellation is observed before/after query execution
and while generating rows/pages.

Definitions are available through Rust and strict versioned JSON. The
[report designer](composition.md) edits accepted definitions with undo/redo and
matching page preview. Subreports, charts/barcodes as report items, repeating
group headers and widow/orphan rules are not provided.
