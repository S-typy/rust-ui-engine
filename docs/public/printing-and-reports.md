# Printing, preview and reports

`ui_core::pages::PrintDocument` is an immutable snapshot of `PrintPage` values:
page size and ordered Scene commands, in logical pixels (96 per inch).
Construction rejects invalid geometry/command indices, protected text and
resource excess: 512 pages, 100,000 commands, 16 MiB text and 128 MiB unique
bitmap payload. `PageViewer::print_preview`, PDF export and native printing
consume the same snapshot. See [ADR-013](adr/ADR-013-document-output.md).

`printing::from_rich_document` adapts paginated rich documents.
`ReportEngine::build` produces pages from a typed table. `printing::from_workbook`
produces worksheet pages with the same static typography as the Spreadsheet:
bold/italic/wrap, explicit foreground/background and merged cells. Selection and
focus decoration are excluded. Formula cells use the current evaluated value.
Number-format metadata is not yet applied by the worksheet presentation.

`SheetPrintOptions` defaults to a 794×1123 logical-pixel page, 48-pixel margins,
112-pixel columns and 28-pixel rows. Optional fit-width uniformly shrinks
columns and typography; it never enlarges them or shrinks below 5%. The used
extent includes empty merged ranges and is bounded to 1,000 rows and 12 columns.
Page breaks move before a merge that crosses the boundary. A connected merged
block taller than one page is rejected; its text is never silently dropped or
duplicated. Invalid options, unknown sheets, exhausted budgets and cancellation
return an error without changing the workbook or returning partial pages.

## Export and Windows printing

`printing::export_pdf` writes page vectors and raster images. Text is shaped
with the normal text engine and exported as glyph images at 192 DPI. Each
shaped line also has invisible text with Unicode `ActualText`; the visible
glyph images are marked as artifacts. Font binaries are not embedded. This preserves
visual glyphs but does not promise outline-text quality at arbitrary zoom or
text extraction in every external reader. Rounded rectangles and ellipses use
cubic vector paths; inset borders use outer/inner even-odd contours. Transparent
vector ink uses isolated Normal-blend opacity states, bounded to 1,024 per page.
Image sources support integral and fractional crops, preserving the source
bitmap and clipping its transformed placement to destination bounds. Placements
beyond the explicit 10,000,000-unit coordinate budget return an error.
Cancellation returns no partial byte buffer.

`FillPath` contours retain their `NonZero` or `EvenOdd` rule and rectangular
clip in PDF output. The exporter writes closed vector paths without rasterizing
them. Opaque contours use the same winding rule in Windows GDI printing.
Windows `Auto` printing composites transparent polygon ink with the GPU.
The explicit `Native` profile rejects transparent vector ink before starting a job.
This geometry support does not establish physical printer output acceptance.

`printing::prepare_for_printer` replaces text with glyph bitmaps for the explicit
native profile. The default transport accepts original text runs directly.
`ui_platform_winit::printing::printers` enumerates configured printers without
opening one. `PrintTicket` selects an explicit printer, zero-based page indices,
copy count, fit-to-printable-area, rendering policy, raster DPI and an optional
absolute output filename.
`print` uses Windows GDI/spooler APIs on a worker, validates the ticket and
driver capabilities before starting a job, and calls AbortDoc when cancelled
or failed. Cancellation cannot retract pages a device has already printed.
`PrintRenderMode::Auto` keeps opaque rectangle/polygon fill pages in GDI. Pages
containing text, images, rounded shapes, ellipses, strokes or transparency use
the shared windowless GPU renderer;
the driver receives an opaque bitmap through `StretchDIBits`, without requiring
pixel-alpha support. This preserves the UI's linear-light blending. Each page
has a white background and a bounded raster resolution: `raster_dpi` defaults
to 192, accepts 96..=600, and must fit 8192 pixels per dimension and 64 MiB RGBA.
Resolution is never silently reduced. Only one completed page bitmap is cached;
GPU preparation or later page failures abort the outstanding job. A successful
receipt means submission to the spooler, not physical paper delivery.

`PrintRenderMode::Native` requires prepared text, opaque vectors, integral
image source coordinates and driver pixel-alpha support for images. Ellipses
are rejected before submission. Other platforms return `Unsupported`.

The semantic PDF layer supports line-level search/copy, not precise per-character
selection geometry or a complete tagged PDF accessibility tree. Partly clipped
lines retain their full Unicode text; clipping is not redaction. Direct PDF
DeviceRGB transparency and GPU linear-light compositing can differ in color.

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
