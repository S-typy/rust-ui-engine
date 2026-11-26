# Property inspector

`PropertyGrid` inspects one or more rows of a registered `DataSourceHandle`.
It reuses `FieldSchema`, `CellValue`, `DataFormField` and `DataFormFactories`;
it does not inspect Rust memory or use reflection. A descriptor adds a category,
a static expandable group path and an optional typed default.

The retained wrapper is mounted beside `Controls`:

```rust,no_run
# use rust_desktop_ui_controls::*;
# use rust_desktop_ui_core::*;
# fn example(ui: &mut Controls, source: DataSourceHandle) -> Result<(), ControlError> {
let mut count = PropertyDescriptor::new("count");
count.category = "Layout".into();
count.groups = vec!["Dimensions".into()];
count.default = Some(rust_desktop_ui_data::CellValue::Integer(10));
let parent = ui.root();
let mut inspector = PropertyGrid::mount(
    ui, parent, source, &[1, 2],
    PropertyGridSettings { properties: vec![count], ..Default::default() },
    &DataFormFactories::default(), LayoutStyle::default(),
)?;
inspector.update(ui)?;
# Ok(()) }
```

Call `input` before pointer and key dispatch. An opening click can then place
the caret in the value or press a button inside the cell. Call `update` after
dispatch and before `Controls::drain_activated` to handle row actions and
focus changes. Escape cancels an ongoing composition first. A second Escape
restores the accepted value. Expanders and editors provide Tab/ShiftTab, Space/Enter, editor
navigation and clipboard behavior. Collapsing a group retains editor IDs,
selection, draft and history. The wrapper's `group` accessor exposes the retained
expander for programmatic expansion.

## Browsing and editing

The inspector opens as a two-column list of property names and accepted values.
For one selected object, click a standard single-line value to place the caret
at that position. F2 or Enter opens the focused value from the keyboard.
The name, divider and row height stay in place. Enter or leaving the value cell
submits a changed draft. Escape restores the previous value. A rejected draft
can be reopened for correction. This applies to plain text, numeric input,
masked text, password and text with a button using the standard value display.

Typing into the focused display cell selects the existing value and replaces it
with the committed input. Hosts can call `prepare_text_input` before
delivering text or an IME preedit and use `pending_text_editor` to identify
the target and its password policy before the editor receives focus.

A standard Boolean cell for one object contains its check box or switch
throughout the interaction. Clicking it or pressing Space posts the new state
in one source transaction. For several selected objects, Apply confirms the
value for the entire selection. The reset icon restores the declared default.

Set `Scrollable="True"` in a height-constrained container to keep the toolbar visible
while the rows scroll. The default intrinsic layout can be hosted inside a form.
The toolbar switches between collapsible categories and a single alphabetical
list. Search matches a label, field name or category path without case
sensitivity. Matching groups expand during search. Clearing the query restores
their previous expansion. Filtering and sorting reuse the original widgets.
Rust callers can access the search field and both view selectors via `toolbar_parts`.

## Accepted values and transactions

`accepted` returns `PropertyValue::Uniform(CellValue)` or `Mixed`. Mixed is not
NULL, an empty string or false. A mixed row displays “Разные значения” in its value cell. Activating the cell starts editing. Its hidden editor has a deterministic default or neutral
value selected within the recipe's bounds; no object's value is chosen as the
representative. `draft` returns `None` until editing begins.

`set_draft` prepares a typed value without saving. Apply calls `commit`; Reset
loads the descriptor's default and follows the same path. All selected objects,
including targets that already have the proposed value, participate in one
optimistic `DataSource::write_batch` with expected values. The adapter must
support atomic batches. There is no sequential fallback.

Validation, composition, source conflicts and setter failures do not change the
inspector's accepted business value or emit a successful `PropertyChanged`.
The prepared draft remains available for correction and retry. Local editor
validation/commit may precede the source attempt: its accepted editing buffer
is distinct from `PropertyGrid::accepted`. Pending asynchronous validation
requires a later explicit Apply. Success emits one property change containing
the selected stable object keys; drain it with `take_changes`.

NULL uses an explicit toggle for nullable text and typed editors; nullable Bool
uses its three-state checkbox. Empty text remains a string. Integer recipes use
exact signed 64-bit values, and Decimal uses exact decimal storage. Readonly
metadata protects editor actions, embedded typed buttons, Apply, Reset and NULL.

`refresh` reads the current source snapshot without adapter I/O. Clean properties
load new values; prepared drafts retain their baselines and report conflicts.
Refresh applies per property: if a later editor rejects loading, earlier clean
refreshes remain valid. Explicit `cancel` restores the inspector baseline;
refresh afterwards to accept an externally changed value. Object keys and schema
are fixed for the mounted inspector; removal or schema change requires explicit
reselection/remount. Dropping the wrapper does not remove its controls.

## Compiled markup

```xml
<PropertyGrid DataSource="Objects" ObjectKeys="1,2" LabelWidth="180">
  <PropertyGrid.Properties>
    <Property FieldName="name" Label="Name" Category="Document"
              DefaultValue="Untitled"/>
    <Property FieldName="count" Category="Layout" Group="Size/Bounds"
              Editor="Numeric" DefaultValue="10"/>
    <Property FieldName="enabled" Category="Document" DefaultIsNull="True"/>
  </PropertyGrid.Properties>
</PropertyGrid>
```

The source must be registered before mounting. `ObjectKeys` are exact signed
64-bit keys, not row indices. Defaults use canonical storage text (ISO dates,
exact decimal strings and lowercase Bool); `DefaultIsNull` explicitly selects
NULL. Default metadata is validated against the source schema before mounting.
The compiled profile accepts the built-in Auto/Text/Numeric/Date/Time/CheckBox
recipes and the cell editors below. Rust callers can supply named `DataFormFactories` recipes. Common
geometry, visibility and enabled bindings are supported; property values belong
to the typed source rather than a parallel string binding model.

## Choices and display templates

A text property can declare a finite list of distinct choices. The inspector
shows the selected text in the value column and opens a selection editor when
activated. Each choice is an allowed source value. The usual Apply, Cancel,
nullable and multi-object rules also apply to these fields.

```xml
<Property FieldName="status" Label="Статус" Category="Документ"
          Choices="Черновик|На согласовании|Утверждён">
  <Property.DisplayTemplate>
    <StackPanel Orientation="Horizontal" Spacing="4">
      <Icon Symbol="Edit" Width="14" Height="14"/>
      <TextBlock x:Name="PART_Value"/>
    </StackPanel>
  </Property.DisplayTemplate>
</Property>
```

A display template accepts one noninteractive layout, text, icon or image root.
An optional TextBlock named `PART_Value` receives the accepted display value,
including mixed and null captions. Each template has its own namescope. Use
`FullWidth="True"` to extend the value across both columns, for example for a
wrapping document note. The normal typed editor still handles changes.
Static group paths, finite text choices and display templates are supported.
Editable collection children and interactive controls inside display templates
are outside this profile.

## Value previews and selected property

Use `LabelWidth="44%"` to reserve a proportion of each ordinary row for its name.
Percentages must be greater than zero and less than 100. A number sets a fixed
width in logical pixels. Long property names wrap as the window narrows.

| Preview | Source value | Appearance |
| --- | --- | --- |
| Auto | Boolean, nullable Boolean | Centered check box for an editable single object. A nullable field supports an indeterminate state. Multiple differing values have their own caption. |
| Switch | Boolean | A switch at the end of the editable single-object cell with its state on the left. |
| Color | Text | A swatch beside a value in `#RRGGBB` or `#AARRGGBB` format. Editing uses the text field. Other text remains visible verbatim. |
| Progress | Integer | A bar and a percentage for 0 through 100 inclusive. Other integers retain their ordinary numeric display. Use `IsReadOnly="True"` for a progress indicator. |

An explicit preview requires a compatible field and is exclusive with `Choices`
and `Property.DisplayTemplate`. A plain choice row has a disclosure arrow.
The preview reads the accepted source value after Apply, Cancel and refresh.

Give a property a `Description` to enable a panel below the rows. It shows the
selected property name and its description as keyboard focus or pointer selection
moves. A read-only property with a description can be selected for inspection.
Run `cargo run -p xaml-form -- --property-editors` for the editor catalog.
Use `--property-publication` to inspect one publication or `--properties` for two documents together.

## Cell editors

Set `Editor` on a property to choose the control that prepares its typed value.
Rows with explicit confirmation use the checkmark to apply the draft to all selected objects in one source transaction.
Escape restores the accepted value. A popup first handles its own selection or cancellation.
For one object, accepting a color or brush in the palette writes the accepted
value once. Dismissing the palette discards the pending selection. Typing a
color directly into its text field and editing several objects still use the
row's explicit confirmation.

| Editor | Source and interaction |
| --- | --- |
| Text, NumericText, MaskedText | Text input, digit text or integer, and masked text. `Mask="00000-000-0000"` stores digits. `Mask="\d{5}-\d{3}-\d{4}"` validates the complete formatted value. |
| Password, ButtonText, Memo | Masked entry, text with a clear button, and a multiline popup. Password storage belongs to the application. The visible cell and its accessibility caption use a mask. |
| AutoComplete, AutoSuggest | AutoComplete selects a listed key. AutoSuggest offers suggestions and accepts a new string. |
| Lookup, SearchLookup, ListBox | Map stable text or integer keys to labels with `Choices` and `ChoiceKeys`. SearchLookup filters the popup. ListBox presents small sets of options using radio buttons. |
| Date, DatePicker, Font | Date input with calendar, day/month/year picker, and font family picker. Dates use a typed source field. Fonts use a text family name. |
| Calculator, Spin | Decimal calculator popup and numeric stepper. Spin accepts an integer or decimal source. Set `CurrencySymbol="₽"` for a grouped amount with a suffix. The symbol stays outside the input and the source stores the exact number. |
| Color, Brush | The palette supports color channels and transparency. A brush can also contain an angle and ordered gradient stops. Accepted text keeps its original spelling until edited. |
| Image | Displays a thumbnail of an encoded binary image. The popup supports working-image edits. The sample handles open/save requests with the platform file dialog. |
| Slider, ZoomSlider, RangeSlider, ProgressBar | Numeric slider, zoom with step buttons, two bounds in a text value such as `20..75`, and read-only progress. Drag either thumb to change one bound or drag the selected track between the thumbs to move both together. Escape restores the pair at the start of the gesture. Use `Minimum`, `Maximum` and `Step` on supported numeric controls. |
| CheckBox, Switch preview | Boolean state with a checkbox or `Preview="Switch"`. Nullable fields can represent an unset value. |
| Rating, Barcode, Hyperlink | Editable star rating, Code 128 B bars from printable ASCII or integer input, and an HTTP(S) navigation request. Optional `UriFormat` substitutes the percent-encoded caption for `{0}`. The host chooses how to open the address. |
| Flags, TokenList | Independent bits of an integer, or an ordered JSON array of selected string keys. Flags preserve bits outside the declared choices. |
| StringCollection, Point, Sparkline | Add, edit, remove and reorder strings in a JSON array. Edit two coordinates in `x,y` form. Edit a JSON sequence of numbers with a plotted preview. |

```xml
<Property FieldName="category" Label="Раздел" Editor="SearchLookup"
          Choices="Архитектура|Графика" ChoiceKeys="11|25"/>
<Property FieldName="accent" Label="Цвет" Editor="Color"/>
<Property FieldName="interval" Label="Диапазон" Editor="RangeSlider"
          Minimum="0" Maximum="100" Step="1"/>
```

A linear brush can be stored as `linear(90;0=#28769B;1=#D5F2E6)`.
StringCollection holds up to 64 single-line items. Each item may occupy 512 UTF-8 bytes,
and the encoded collection may occupy 16 KiB. Nested application object graphs require their own descriptors.

### Application-defined editors

Register a `PropertyCellEditorFactory` under a name such as `Custom.PaperSize` in
`PropertyCellEditors`. Assign that name to a property's `Editor` and pass the registry to
`PropertyGrid::mount_with_editors`. The factory validates each selected object's typed value
and mounts one editor inside the supplied value cell. Its `PropertyCellEditor` loads and
reads drafts. The grid applies optimistic source writes, including multi-object changes.

## Bounds and verification

The profile supports 1–64 objects, 1–128 properties, at most eight nested group
levels, 256 expandable groups, 32 KiB descriptive metadata and 256 undrained
successful change events. Static paths are presentation groups over schema
fields; they do not promise arbitrary recursive object graphs.

Regression tests cover mixed/NULL/default separation, exact integer limits,
all-target optimistic expectations, one rejected setter without partial writes,
retained nested editors, readonly actions, malformed metadata and hostile
validation messages. The example opens with `xaml-form --properties`.
