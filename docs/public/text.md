# Text shaping, rasterization and editing

`rust-desktop-ui-text` provides Unicode editing and CPU preparation of text for
the GPU renderer. It has no window-system or GPU dependency. Parley 0.11.1 shapes
and wraps text using system fonts, including bidi ordering and fallback; Swash
0.2.10 rasterizes glyph outlines and supported color glyphs. The renderer uploads
these bitmaps to a GPU atlas. Text is not represented by rectangle placeholders.

## Preparing glyphs

```rust
use rust_desktop_ui_core::{Color, Rect, TextRun};
use rust_desktop_ui_text::TextEngine;

let mut engine = TextEngine::new();
let run = TextRun::new(
    "Hello, Привет!",
    Rect::new(20.0, 30.0, 320.0, 60.0),
    Color::rgb(240, 240, 240),
    18.0,
);
let prepared = engine.prepare(&run, 1.5)?;
assert!(prepared.height > 0.0);
# Ok::<(), rust_desktop_ui_text::TextError>(())
```

`TextRun` bounds, clip and font size use logical pixels. `prepare` accepts the
display scale; `PreparedText` dimensions, glyph positions, clips, hit-test points
and caret/selection rectangles use global physical pixels. Glyph positions include
raster bearings. Effective clipping is the intersection of the run bounds and
explicit clip. Selection and caret geometry remain unclipped, so the host can
scroll them into view.

Each `RasterGlyph` carries an opaque `GlyphKey`, an `Arc<GlyphBitmap>`, and a tint.
Bitmaps use straight RGBA: outline masks contain white RGB and coverage alpha;
color glyphs retain their colors. Mask tint is the text color. Color-glyph tint
is white with the text opacity. Atlas implementations must apply tint once and
use blending appropriate for straight RGBA. Keys include font identity, glyph,
size, variation coordinates and quarter-pixel subposition; they are process-local
identities, not persistent font identifiers.

`TextEngine` caches up to 1,024 shaped layouts and 4,096 glyph entries with a 64 MiB
bitmap budget. Layout keys exclude position, color and clipping, allowing scrolling
without reshaping unchanged text. Fractional motion may require another raster
subposition. `TextStats` distinguishes cache hits from layout and raster work;
`clear_caches` releases the engine's cached layouts and bitmaps. Callers retaining
prepared text also retain its shared resources.

One `PreparedText` additionally limits its retained unique glyph bitmap data to
64 MiB, independently of cache eviction. Instances sharing the same bitmap Arc
count once; a new raster allocation after eviction counts separately. Exceeding
the budget returns `RasterLimit`.

## Editing and IME composition

```rust
use rust_desktop_ui_text::TextDocument;

let mut document = TextDocument::new("Привет, 👩‍💻");
document.backspace(); // removes the complete emoji grapheme
document.insert("мир");
document.select_all();
let clipboard_text = document.selected_text().to_owned();
document.set_preedit("世界", Some((0, "世界".len())))?;
assert_eq!(document.text(), clipboard_text); // preedit is not committed
document.commit("世界");
assert_eq!(document.text(), "世界");
assert!(document.undo());
# Ok::<(), rust_desktop_ui_text::EditError>(())
```

Selections and navigation positions are UTF-8 byte indices at extended-grapheme
boundaries. Invalid explicit selections return an error without changing the
document. Backspace, forward delete and left/right movement preserve combining
sequences, flags and emoji ZWJ sequences. Word movement uses Unicode word
boundaries. Home/End refer to hard lines, including CRLF and Unicode line
separators; they do not refer to wrapped visual lines.

Logical document movement and visual bidi movement are separate APIs. For visual
arrows, retain the complete `TextCaret` returned by `hit_test_caret` and pass it
to `move_caret_visual` and `caret_geometry`. Its affinity distinguishes two visual
positions at a bidi or soft-wrap junction. The simpler byte-index convenience
methods do not preserve that affinity. `selection_rects` returns individual visual
fragments for a logical byte range.

Preedit text replaces the original selection only for display. Its optional
cursor range uses UTF-8 boundaries within the preedit string, as supplied by the
platform. `display_text` and `display_selection` describe the temporary view;
`commit` records one replacement operation, while `cancel_preedit` leaves the
committed text intact. Undo/redo stores up to 128 full document states. New edits
after undo discard redo history. Clipboard integration is host-owned: use
`selected_text`, `cut` and `paste` to exchange strings with a platform clipboard.

## Fonts, dependencies and current limits

Fonts are discovered from the operating system; no font files are bundled.
Linux builds require `pkg-config` and the Fontconfig development library (for
example `libfontconfig1-dev` on Debian/Ubuntu). Install appropriate fonts for the
scripts your application uses. `PreparedText::missing_glyphs` counts shaped
`.notdef` glyphs and therefore reports incomplete font coverage, not a universal
guarantee that every font color format can be rasterized. Swash's outline,
COLR outline and bitmap paths are used; unsupported font/color formats can still
produce no bitmap. Spaces and other nonpainting glyphs also produce no bitmap.

Runs are limited to 1 MiB of UTF-8, 65,536 prepared glyphs and a physical font size
of 1,024 pixels. Invalid or non-finite inputs return `TextError`. The engine uses
system sans-serif selection with weight, size, alignment and wrapping; a public
font-family/custom-font API, rich spans, password masking and vertical writing
are not provided. `TextDocument` is a plain UTF-8 string editor, not a large-file
rope. OS IME events, clipboard access, caret blinking and accessibility belong to
the host/control layer. Shaping tests do not replace interactive IME or GPU tests.

Run `cargo run -p rust-desktop-ui-text --example font_probe` to inspect local
Latin, Cyrillic, Arabic, Hebrew, CJK and emoji coverage. Unit tests exercise real
glyph coverage pixels, cache reuse, bidi caret geometry, wrapping and Unicode
editing. Font availability and resulting metrics vary by machine.

Implementation references: [Parley](https://docs.rs/parley/0.11.1/parley/),
[Swash](https://docs.rs/swash/0.2.10/swash/), and
[Unicode segmentation](https://docs.rs/unicode-segmentation/1.13.3/unicode_segmentation/).
