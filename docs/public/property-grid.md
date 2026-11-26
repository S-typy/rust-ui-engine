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

Call `update` after dispatch and before `Controls::drain_activated`. It consumes
only this inspector's Edit, Apply and Reset actions. Other activations remain
available to the application. `input` is an optional pre-dispatch Escape route:
composition is cancelled first; the next Escape restores the property's accepted
value. Existing expanders and editors provide Tab/ShiftTab, Space/Enter, editor
navigation and clipboard behavior. Collapsing a group retains editor IDs,
selection, draft and history. The wrapper's `group` accessor exposes the retained
expander for programmatic expansion.

## Browsing and editing

The inspector opens as a two-column list of property names and accepted values.
Click an editable value or activate its button with the keyboard to open the
inline editor. Apply or Enter in a text field submits the draft. Escape cancels
editing. The reset icon restores the declared default. Its tooltip and accessible
name identify the property. Only one editor is shown at a time. A prepared draft
in another row is retained and can be reopened.

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
recipes. Rust callers can supply named `DataFormFactories` recipes. Common
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
| Auto | Boolean, nullable Boolean | Check mark and caption. Multiple differing values and NULL have distinct captions. |
| Switch | Boolean | A switch with a state caption. Activation opens the switch editor. |
| Color | Text | A swatch beside a value in `#RRGGBB` or `#AARRGGBB` format. Editing uses the text field. Other text remains visible verbatim. |
| Progress | Integer | A bar and a percentage for 0 through 100 inclusive. Other integers retain their ordinary numeric display. Use `IsReadOnly="True"` for a progress indicator. |

An explicit preview requires a compatible field and is exclusive with `Choices`
and `Property.DisplayTemplate`. A plain choice row has a disclosure arrow.
The preview reads the accepted source value after Apply, Cancel and refresh.

Give a property a `Description` to enable a panel below the rows. It shows the
selected property name and its description as keyboard focus or pointer selection
moves. A read-only property with a description can be selected for inspection.
Run `cargo run -p xaml-form -- --property-editors` for an example with one
publication, or use `--properties` to inspect two documents together.

## Bounds and verification

The profile supports 1–64 objects, 1–128 properties, at most eight nested group
levels, 256 expandable groups, 32 KiB descriptive metadata and 256 undrained
successful change events. Static paths are presentation groups over schema
fields; they do not promise arbitrary recursive object graphs.

Regression tests cover mixed/NULL/default separation, exact integer limits,
all-target optimistic expectations, one rejected setter without partial writes,
retained nested editors, readonly actions, malformed metadata and hostile
validation messages. The example opens with `xaml-form --properties`.
