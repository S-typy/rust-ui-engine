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

## Accepted values and transactions

`accepted` returns `PropertyValue::Uniform(CellValue)` or `Mixed`. Mixed is not
NULL, an empty string or false. A mixed row displays “Разные значения” and an
explicit Edit action. Its hidden editor has a deterministic default or neutral
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

## Bounds and verification

The profile supports 1–64 objects, 1–128 properties, at most eight nested group
levels, 256 expandable groups, 32 KiB descriptive metadata and 256 undrained
successful change events. Static paths are presentation groups over schema
fields; they do not promise arbitrary recursive object graphs.

Regression tests cover mixed/NULL/default separation, exact integer limits,
all-target optimistic expectations, one rejected setter without partial writes,
retained nested editors, readonly actions, malformed metadata and hostile
validation messages. The example opens with `xaml-form --properties`.
