# PDF documents and viewing

`ui_controls::pdf::PdfDocument` opens PDF bytes, exposes page scenes, extracted
text, diagnostics, bookmarks, text fields and notes, and saves the original
object graph with supported edits. It uses `lopdf` for PDF structure and stream
decoding. Page interpretation and retained presentation are implemented by this
library; rendering uses the normal Scene/GPU pipeline.

The supported profile includes unrotated pages, axis-aligned transforms,
RGB/gray rectangles, compound polygon fills, cubic paths and straight strokes,
rectangular clips, basic text
operators, RGB/gray 8-bit images (raw/Flate/JPEG), soft alpha masks, direct-page
bookmarks, flat writable text widgets and text annotations. Search returns
page/item indices, UTF-8 byte ranges and item bounds. Unicode field edits
regenerate an appearance; adding/removing a note and saving retains unrelated
original objects.

Invisible text mode (`Tr 3`) participates in extraction without adding painted
Scene text. `ActualText` replaces the marked content once; horizontal text
scaling contributes to extraction bounds. Visible scaled text still uses
system fallback metrics and reports that limitation. This supports the
exporter's line-level Unicode layer without painting duplicate glyphs.

Compound fills preserve nonzero/even-odd winding, implicit closure and nested
holes. Cubic `c`, `v` and `y` operators are flattened after transformation with
a 0.125 logical-pixel chord-distance bound, at most 16 subdivision levels and
256 points per current path. Excess complexity returns an error. The path is
independent of the saved graphics state; clipping changes after painting.
Uniform rectangular inset rings use the normal border primitive, allowing
dense worksheet borders without spending the general-path frame budget.
General filled paths still share the renderer's 512-path frame limit, including
the main page and thumbnails composed in that frame.
Normal-blend constant stroke/fill opacity is preserved across color changes,
saved/restored by `q`/`Q`, and applied to images as well as vector ink. Images
honor `Interpolate`. GPU compositing uses the normal linear-light UI pipeline;
colors can differ from readers compositing directly in DeviceRGB.

Form XObjects support an axis-aligned `Matrix`, `BBox` clipping, nested calls
and local resource dictionaries, including repeated names in different forms.
Absent form resources inherit the caller's dictionary. Used resources are
renamed internally before interpretation without changing the original object
graph. Named marked-content properties support Unicode `ActualText`.
Cycles, malformed scopes and excessive expansion fail before returning pages.
Limits are 16 form levels, 4,096 invocations and used resources, 50,000 expanded
operations, and 32 MiB decoded page data including repeated forms and images.
Each individual content stream remains bounded to 4 MiB.

The reader is bounded to 16 MiB input, 20,000 objects, 512 pages, 4 MiB per decoded
content stream, 50,000 expanded page operators and 128 MiB total decoded content.
Page-tree cycles/shared nodes, invalid dimensions, encrypted input, unsupported
transforms and exhausted budgets return errors. The cancellation callback is
polled between bounded operations, not during every underlying parser call.
Post-load object checks and per-stream limits are not process isolation.
Applications accepting hostile documents should isolate parsing in a worker
process with OS resource limits.

Rendering is deliberately limited. Embedded-font metrics are approximated
using system fallback, with a diagnostic. Unsupported painting operators
generate diagnostics. Page rotation and
non-default UserUnit are rejected. A CropBox differing from MediaBox is
diagnosed; the media area is displayed. Color-management, nonrectangular clips,
transparency groups, non-Normal blend modes, graphics-state soft masks,
signatures, encryption, JavaScript and hierarchical
AcroForm editing are not implemented. Original unsupported objects may survive
save without becoming visible or editable. Loading is not a guarantee of
pixel-identical rendering of an arbitrary PDF.

Stroke caps/joins currently use round geometry; other requested styles,
hairlines and nonuniform stroke scaling produce diagnostics. Transparent
strokes above 16 segments and combined transparent fill/stroke return an error
instead of introducing extra alpha blends. Forms invoked while a path remains
unfinished are outside the current profile.

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
