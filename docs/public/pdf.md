# PDF documents and viewing

`ui_controls::pdf::PdfDocument` opens PDF bytes, exposes page scenes, extracted
text, diagnostics, bookmarks, text fields and notes, and saves the original
object graph with supported edits. It uses `lopdf` for PDF structure and stream
decoding. Page interpretation and retained presentation are implemented by this
library; rendering uses the normal Scene/GPU pipeline.

The supported profile includes unrotated pages, axis-aligned transforms,
RGB/gray rectangles and straight strokes, rectangular clips, basic text
operators, RGB/gray 8-bit images (raw/Flate/JPEG), soft alpha masks, direct-page
bookmarks, flat writable text widgets and text annotations. Search returns
page/item indices, UTF-8 byte ranges and item bounds. Unicode field edits
regenerate an appearance; adding/removing a note and saving retains unrelated
original objects.

The reader is bounded to 16 MiB input, 20,000 objects, 512 pages, 4 MiB decoded
content per page, 50,000 page operators and 128 MiB total decoded content.
Page-tree cycles/shared nodes, invalid dimensions, encrypted input, unsupported
transforms and exhausted budgets return errors. The cancellation callback is
polled between bounded operations, not during every underlying parser call.
Post-load object checks and per-stream limits are not process isolation.
Applications accepting hostile documents should isolate parsing in a worker
process with OS resource limits.

Rendering is deliberately limited. Embedded-font metrics are approximated
using system fallback, with a diagnostic. Form XObjects, unsupported painting
operators and nonrectangular fills generate diagnostics. Page rotation and
non-default UserUnit are rejected. A CropBox differing from MediaBox is
diagnosed; the media area is displayed. Color-management, arbitrary curves,
complex transparency, signatures, encryption, JavaScript and hierarchical
AcroForm editing are not implemented. Original unsupported objects may survive
save without becoming visible or editable. Loading is not a guarantee of
pixel-identical rendering of an arbitrary PDF.

`PageViewer::pdf` mounts page navigation, four-page thumbnail groups, fit/zoom,
scrolling, search highlighting, bookmarks, an extracted-text editor and a
field/note editor. `begin_load`/`complete_load` discard outdated load tokens.
Unapplied field drafts block replacement. Only the extracted-text preview is
truncated above 1 MiB; the document and search stay intact.

Run `cargo run --locked -p xaml-form -- --pdf`. Open/Save and explicit printer
commands run in the host. The example never opens a file or submits a print
job merely by starting. Canvas text hit-selection, link actions, all form types
and native keyboard/screen reader acceptance remain outstanding. The viewer is
a Rust-mounted control, not a compiled `<PdfViewer>` element.

The `document_output_codec` example provides explicit-path `sample`, `inspect`
and `edit` operations without opening windows or printing. PDF export is
described in [printing and reports](printing-and-reports.md).
