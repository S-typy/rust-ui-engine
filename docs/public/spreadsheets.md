# Spreadsheet

`ui_data::spreadsheet::SpreadsheetSession` owns a Workbook, active sheet, range
selection and bounded undo/redo history. `ui_controls::spreadsheet::Spreadsheet`
mounts a retained formula editor, toolbar, format picker and virtual cell viewport. The host
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
Wheel down advances to lower rows; Shift+wheel down advances to right columns.
Horizontal wheel deltas also move columns. Each nonzero axis advances by three
cells, and zero deltas leave both the viewport and pending formula draft alone.
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

`Spreadsheet::auto_fit` sizes a selected inclusive range of columns or rows.
The toolbar's «Автоширина» and «Автовысота» apply it to the current selection.
Columns measure unwrapped formatted text, including trailing spaces; rows measure
line breaks and wrapping at the current combined column width. Measurement uses
the same text shaping, 14-DIP typography, bold/italic and number formatter as the
worksheet. Merged anchors contribute their whole text; any additional space is
distributed among selected axes only. Unselected dimensions remain unchanged.
The «Размеры» example sheet contains long, wrapped, multiline and merged cells.

The standalone `auto_fit_edits` planner takes a reusable `TextEngine` and returns
an atomic `WorkbookEdit` batch without modifying the workbook. It scans sparse
stored cells, including offscreen values. Empty axes use the sheet default as
their minimum; nonempty axes can shrink to their content, with minimum widths
of 32 and heights of 28 logical pixels. At most 16384 axes can be requested;
required sizes above 4096 pixels and unsupported formats return an error.
The workbook's dimension metadata limit still applies to the resulting batch.

A changed formula draft and the size batch are prepared together before live
publication, then recorded as separate undo steps. Repeating an unchanged fit
adds no history. Errors preserve the workbook, draft and history; compositions
in either formula or format-code editor block the operation. The format-code
draft remains staged, and both retained editors keep their identities. Sizing is
explicit; editing a value does not automatically resize its row or column.
Double-click header sizing and hidden rows/columns are not provided.

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

Duration formats use one total field: `[h]`/`[hh]`, `[m]`/`[mm]` or `[s]`/`[ss]`.
Numeric values count days: `1.5` with `[h]:mm:ss` displays `36:00:00`.
Following `m`/`mm` and `s`/`ss` fields show remainders in decreasing unit order;
seconds can have one to nine fractional digits, such as `[ss].000`. A doubled
total field pads to at least two digits without limiting the total. Typed time
values retain their clock duration; typed dates do not become durations.
Sign/zero sections, quoted literals and supported color/locale directives apply.
Omitted units and fractional precision truncate toward zero. Integer/Decimal
conversion stays exact; magnitudes exceeding the `u128` nanosecond budget produce
a diagnostic. The decimal separator for fractional seconds is the literal dot
in the format, as for typed clocks.

Numeric fractions support mixed forms (`# ?/?`, `# ??/??`) and improper forms
(`??/??`). A denominator with one to three placeholder digits chooses the nearest
fraction with denominator at most 9, 99 or 999. Equal errors prefer a smaller
denominator, then the larger value. A literal denominator from 1 to 999999 keeps
that denominator without reducing it: `1.875` with `# ?/8` displays `1 7/8`.
Fixed-denominator ties round away from zero. A rounded whole fraction carries
into the integer part of a mixed format.

`0` requires a digit, `#` suppresses insignificant zeroes, and `?` reserves
ordinary spaces. Leading `#` may precede a run of `0` or `?`; mixing required
zeroes with question marks within one field is unsupported. The numerator mask
is limited to six placeholders; extra significant digits remain visible.
A zero fractional part disappears unless its numerator requires zeroes; question
marks preserve spaces in its place. A mixed format also retains its literal
space separator. Sign, color, locale and quoted-literal sections still apply.
Integer/Decimal values retain exact whole parts and decimal precision; Real
uses its shortest decimal representation. This is display formatting: input,
formulas, clipboard and XLSX storage remain numeric. Fraction masks with percent,
comma scaling/grouping or decimal points are outside this profile. Question-mark
spacing does not guarantee alignment in a proportional font.

Numeric conditions support `<`, `<=`, `=`, `<>`, `>=` and `>` with a signed
decimal or scientific threshold, such as `[>100][Green]0.0;[<-100][Red](0.0);0.0`.
One condition may precede each of the first two sections, alongside color and
locale modifiers in any order. Conditions in the third or text section, duplicate
conditions and malformed thresholds produce a diagnostic. Exponents are bounded
to -1024..1024; thresholds use a dot independent of the display culture.

Conditions compare the original signed value before rounding, percent or comma
scaling. Integer and Decimal comparisons are exact; Real uses its shortest decimal
representation. Typed dates and clocks do not implicitly become numeric serials
for condition evaluation. The fourth section still formats text independently.

The first matching condition wins. In a three-section numeric format, an
unconditional first section matches nonnegative values and an unconditional
second section matches negative values; the third section handles the remainder.
With two sections, an unconditional second section handles every value rejected
by the first. A lone unmatched condition falls back to General. Two unmatched
conditions without a third section produce a raw-value diagnostic. An explicit
General section uses the selected culture and color.

A condition that accepts only strictly negative values consumes the implicit
minus; write a literal minus or parentheses when needed. The same rule applies
to a two-section unconditional fallback whose complement is strictly negative.
Other matching conditions and the third fallback preserve the sign. Selection
and sign are not reconsidered after rounding: `-0.001` with `[<=0]0.00;0` retains
the minus in its rounded zero. Fractions and durations use this same selection.
These directives choose a number-format section; they do not implement workbook
conditional-formatting rules such as color scales or formula-driven styles.

Alignment directives remain outside this profile. Unsupported formats retain their metadata and display a raw value
with a diagnostic, rather than a guessed result. Printing uses the same formatter
and rejects unsupported formats explicitly. Formatting does not alter typed
clipboard values or formulas.

The format picker shows the active cell's format, including an existing custom
format that is not among its presets. Choosing a preset only stages the choice;
«Применить» applies it to every selected anchor. A mixed-format selection shows
the active cell's format until application. Presets include general, integer,
two decimal places, percent, scientific/engineering notation, date/time, durations and
rubles and fractions. General restores default presentation. The picker uses the common
ComboBox keyboard and accessibility contract.

The separate format-code editor accepts custom strings. Typing and leaving it do
not change the workbook. Its Apply button or Enter calls `apply_format_code`;
Escape restores the active cell's accepted format. Applying a preset refreshes
both the picker and the code editor. A rejected code or formula preserves both
drafts and the workbook/history. Unsupported but valid format syntax retains the
metadata and produces the diagnostic described above. A code-editor IME preedit
blocks format application and workbook replacement until it finishes. These are
separate retained editors: the formula editor keeps its identity and draft.

`SpreadsheetSession::set_number_format` updates only `CellStyle.number_format`
and returns whether anything changed. Values, formulas, other styles and
selection remain intact. A range is one undo step; repeating the current format
adds no history. Merged children are not materialized. A newly formatted empty
cell counts against the workbook cell limit. The format is bounded to 256 UTF-8
bytes and must satisfy the workbook XML character checks. Unsupported format
metadata remains available with the presentation diagnostic described above.
`Spreadsheet::set_number_format` first validates a candidate containing the
current draft and format. A bad formula, invalid format or capacity failure
preserves the live workbook, draft and history. An active IME composition blocks
application. A changed draft and a format change are separate undo steps when
both succeed. Undo/redo, cell navigation and sheet changes refresh the picker.
Numeric values are not converted to typed dates or times by choosing a format.
The example's «Форматы» worksheet shows date names, clocks, locale overrides,
engineering rounding, durations, fractions and numeric conditions alongside their format strings.

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
