# Spreadsheet

`ui_data::spreadsheet::SpreadsheetSession` owns a Workbook, active sheet, range
selection and bounded undo/redo history. `ui_controls::spreadsheet::Spreadsheet`
mounts a retained formula editor, toolbar and virtual cell viewport. The host
routes `input`, `text_input`, `activate` and clipboard requests, then calls
`refresh` after layout. During a captured gesture, schedule `interaction_tick`
while `needs_interaction_tick` is true and call `cancel_interaction` on capture
loss. Run the example with `cargo run --locked -p xaml-form -- --spreadsheet`.

The formula bar accepts typed values and formulas. Enter/F2 or a double click
moves its retained TextBox into the active cell. Typing on the focused worksheet
starts a replacement draft. Enter/Apply commits; Escape restores the selected
cell's value. The formula preview returns the same draft to the bar. Both
placements share the WidgetId, text selection, undo stack and IME target.
Tab or blur alone does not accept a workbook value; a worksheet command or
navigation commits the draft first. A leading apostrophe forces
text during explicit entry. External TSV paste keeps formulas and leading
apostrophes as text by default. Existing IME composition blocks commands that
would replace the draft. Resize retains the editor and its draft.

Arrow keys move the active cell, Shift extends the selection, PageUp/PageDown
move through rows, and the wheel scrolls (Shift selects the horizontal axis).
Click selects a cell; Shift-click extends the range. Drag selects a rectangle
and autoscrolls at the viewport edges. Intersected merged ranges are included
in full and editing targets their anchor. Escape, capture loss, disabling or
resizing cancels a gesture; stale gestures never restore an old document state.

Merge rejects overlapping ranges and nonempty non-anchor cells. Unmerge
preserves the anchor. Copy snapshots cells and merges; typed paste and fill
translate relative formula references while preserving absolute references.
Fill down/right and the fill handle reject merged selections until explicitly
unmerged. Dragging the handle repeats the immutable source pattern in any
direction, translating each formula relative to its source cell. Preview does
not change workbook cells; release publishes one undoable edit.
All workbook changes publish atomically and participate in undo/redo, bounded
to 64 snapshots and an estimated 64 MiB.

Ctrl+C/V requests host-owned text clipboard I/O. Ctrl+Shift+V uses the session's
typed copy, including formulas, styles and merges. A delayed clipboard response
is rejected after the selection or document generation changes, or a newer
formula draft/composition appears. Text paste is
bounded to 4 MiB and 16,384 cells. File I/O belongs to the application; the
example loads/saves the [XLSX profile](workbooks.md) on workers and rejects stale
loads after newer edits.

The viewport draws only visible cells, with sparse row heights/column widths,
bold/italic text, wrapping, foreground/background and merged geometry. Drag a
header edge to resize; Escape cancels its preview and release commits once.
`set_row_height` and `set_column_width` use the same undo history; `None` restores
the sheet default. Rendering, hit testing, merged bounds and accessibility use
the same sparse geometry, including the last worksheet row and column.
Dragging clamps sizes to 14..4096 logical pixels; the data API permits 1..4096.
The viewport rejects more than 16,384 visible cells instead of allocating an
unbounded scene. Empty viewport space moves the editor back to its formula slot
without losing the draft. There is no autosize or hidden-axis UI.

`format_cell`/`format_value` return `CellDisplay` with text, optional foreground
and a diagnostic. `Spreadsheet::set_culture` selects Russian (default), English
or Arabic separators without changing stored values. The bounded format profile
supports `0`/`#` decimal placeholders, grouping, percent, comma scaling,
scientific and engineering notation, up to four sign/text sections,
quoted/escaped literals, `@` and eight named colors. Integer/Decimal rounding
does not convert through floating point. Scientific masks allow one to six
integer placeholders, with optional `#` before required `0`. Their count sets
the exponent step: `##0.00E+00` uses powers of three. Rounding can advance the
exponent; `e` and `E` retain their case.

Typed dates support numeric year/month/day fields, `mmm`/`mmmm` month names,
`mmmmm` initial letters and `ddd`/`dddd` weekday names. Names use the selected
culture and the Gregorian calendar. Russian full month names use the genitive
form when a numeric day is present. Arabic short names use the full name.
Typed times support hour/minute/second, fractional seconds and `AM/PM` or `A/P`
markers in either case. An unquoted marker switches hours to 1..12; quoted
text does not change the clock. Fractional seconds retain the requested leading
nanosecond digits. Defaults remain DD.MM.YYYY and HH:MM.

A leading `[$-en-US]`, `[$-ru-RU]` or `[$-ar-SA]` selects the culture for that
section. Hexadecimal aliases `409`/`0409`, `419`/`0419` and `401`/`0401` are also
accepted. The directive may accompany one named color. It changes presentation
only, with Gregorian dates and Latin numeric digits; currency-bearing directives,
other locale IDs and alternative calendars are not supported.

Conditional directives, fractions, elapsed-time formats and alignment
directives are outside this
profile. Unsupported formats retain their metadata and display a raw value
with a diagnostic, rather than a guessed result. Printing uses the same formatter
and rejects unsupported formats explicitly. Formatting does not alter typed
clipboard values or formulas. There is no format-picker UI yet.
The example's «Форматы» worksheet shows date names, clocks, locale overrides
and engineering rounding alongside their format strings.

The formula editor uses the shared accessible text control.
`Spreadsheet::semantics` supplies bounded
visible cells, merged spans, logical counts, row/column headers and selection.
The example projects this snapshot through the common Windows UIA Grid/Table
provider. Accessible selection preserves a pending formula draft or IME composition.
ItemContainer enumerates virtual cells without scrolling; Realize or an explicit
Grid.GetItem request can reveal a bounded viewport. Neither operation changes
selection, focus or accepted values. An active formula draft or IME composition
blocks realization, as do captured gestures and an active in-cell editor.
Merged coordinates resolve to their anchor; stale workbook
generations are rejected. Ordinary property queries do not materialize the full
sheet. Native keyboard, IME, clipboard and editing acceptance remain separate
from the table provider. See [the virtual table contract](adr/ADR-016-virtual-table-accessibility.md).

These are Rust composition APIs. There is no `<Spreadsheet>` compiled-XAML
element yet; the example mounts it inside a compiled XAML host.
