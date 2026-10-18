# Workbook

`rust_desktop_ui_data::workbook` provides a bounded, nonvisual workbook model.
A workbook owns ordered, uniquely named worksheets, sparse cells, styles,
formula results and a dependency graph. Spreadsheet presentation is a separate
component; this module does not create a grid or a window.

```rust
use rust_desktop_ui_data::{CellValue, workbook::*};

let mut book = Workbook::new("Data")?;
book.apply(&[
    WorkbookEdit::Set {
        sheet: 0, address: CellAddress::parse("A1")?,
        cell: WorkbookCell::value(CellValue::Integer(12)),
    },
    WorkbookEdit::Set {
        sheet: 0, address: CellAddress::parse("B1")?,
        cell: WorkbookCell::formula("=A1*2"),
    },
])?;
assert_eq!(book.value(0, CellAddress::parse("B1")?)?.display(), "24");
let bytes = book.to_xlsx()?;
let imported = Workbook::from_xlsx(&bytes)?;
# Ok::<(), rust_desktop_ui_data::DataError>(())
```

## Model and transactions

`CellAddress` uses zero-based Rust coordinates and conventional A1 strings.
`CellRange::new` validates inclusive ordered boundaries. `Workbook::apply`
accepts Set, Clear, Copy and AddSheet edits. A batch validates and recalculates
on a candidate; failure preserves all cells, sheet identities, results and revision.
An empty batch does not advance revision. Each successful batch recalculates
the bounded workbook once. `dependencies` exposes referenced cells, including
both branches of conditional expressions.

Copy reads a snapshot, so overlapping ranges are deterministic. Relative
references translate on both axes, `$A1` fixes the column, `A$1` fixes the row
and `$A$1` fixes both. A translated reference outside a worksheet becomes
`#REF!`. Copying to an invalid destination rejects the whole transaction.
Sheet deletion, renaming, row/column insertion and merged cells are not supported.

Limits: 64 sheets, 16,384 stored cells across the workbook, 16 MiB of cell and
formula text, 32 KiB per text cell, 4 KiB per formula and 262,144 dependency
edges. Coordinates cover 1,048,576 rows and 16,384 columns. A single enumerated
range is limited to 16,384 cells. Limits bound work; they do not allocate a dense
worksheet. The `to_table` bridge validates evaluated cells against an application
`TableSchema`; its result uses ordinary typed queries and stable row coordinates.
Formula errors are reported instead of converted to null.

## Formula profile

Supported expressions: numbers, strings with doubled quotation marks, TRUE/FALSE,
A1 references, quoted or unquoted sheet references, ranges, parentheses, unary
signs, percent, `+ - * / ^`, `&`, and comparisons `= <> < <= > >=`.
Functions: SUM, AVERAGE, MIN, MAX, COUNT, ABS, ROUND and lazy IF.
Function names are case-insensitive. A range aggregates numeric cells and ignores
text, booleans and empty cells. Direct arithmetic converts empty cells to zero
and booleans to zero/one; it does not parse numeric text. Numeric expressions use
finite IEEE-754 `f64`; exact integer/decimal storage does not imply exact formula
arithmetic. ROUND accepts integral digit counts from -15 through 15 and rounds
ties away from zero. String comparisons are lowercase Unicode comparisons,
not locale collation. Text concatenation uses the scalar display representation.

Errors are values: division by zero, incompatible types, invalid references,
unknown functions, nonfinite arithmetic, cycles and exhausted evaluation limits.
Errors propagate through dependent cells. The evaluator limits parsing to 512
tokens and 32 levels of AST depth, combined expression/cell evaluation to 64
nested expressions, cell chains to 128 and total evaluation steps
to two million per recalculation. Cyclic references produce `#CYCLE!`; breaking
a cycle recalculates affected values. IF evaluates only its selected branch.
Network functions, macros, external workbook references, array formulas and
named ranges are not executed.

## XLSX interchange

`to_xlsx` and `from_xlsx` read/write bytes. Applications own paths, dialogs,
worker threads and explicit persistence. The format follows the package and
SpreadsheetML structure described by [ECMA-376](https://ecma-international.org/publications-and-standards/standards/ecma-376/)
and the [SpreadsheetML document structure](https://learn.microsoft.com/en-us/office/open-xml/spreadsheet/structure-of-a-spreadsheetml-document).

The supported profile contains multiple sheets, inline/shared strings, booleans,
finite numbers, dates, times, normal cell formulas and basic cell styles:
bold, italic, opaque RGB foreground/background, wrapping and number-format code.
Exports use inline strings and cached formula results; imports always recalculate
formulas with this evaluator. Numeric storage distinguishes integral and fractional
values by their lexical representation on import. Dates and times use ISO cell
values on export; recognized numeric date/time formats are imported using the
1900 or 1904 system. The fictional 1900-02-29 is rejected. Combined timestamps
are not a separate workbook value type.

Integers exceeding 15 decimal digits and exact decimal values cannot be exported
as spreadsheet numbers without an explicit application conversion to number or
text. A failed export does not change the workbook. Formula cycle/limit results
use the portable cached errors `#REF!`/`#NUM!`; formulas remain intact and regain
their own errors on import.

`WorkbookImport::diagnostics` reports unsupported formatting, parts and metadata.
Unsupported shared/array formulas, invalid references, malformed data, macros,
DTD and encrypted packages are rejected. Optional charts, drawings, validation,
print settings and similar parts are not retained: check diagnostics before
overwriting an imported file. Import is a conversion to the declared profile,
not a lossless archive editor.

ZIP preflight rejects duplicate part names, unsafe paths, inconsistent local
headers, ZIP64 and multidisk packages before the archive reader allocates its
directory. Both compressed and expanded packages are limited to 64 MiB, one
part to 16 MiB and the directory to 1,024 entries. No package path is extracted
to disk; relationships never trigger network requests. XML DTD is disabled and
node counts are bounded.

Run the explicit-path example with `cargo run -p rust-desktop-ui-data --example
workbook_codec -- sample sample.xlsx`, `inspect input.xlsx` or `roundtrip input.xlsx
output.xlsx`. Model and codec tests cover atomicity, reference translation,
recalculation, cycles, typed query projection, supported XLSX round-trips and
package validation. Native office application validation is separate from these
tests.
