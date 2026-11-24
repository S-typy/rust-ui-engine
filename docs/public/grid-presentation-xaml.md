# Declarative grid presentation

`GridControl` accepts bounded presentation metadata alongside `GridColumn` or
`GridControl.Columns`. Existing grids default to table view without grouping,
summaries, bands, detail records or cell presentation overrides. When constructing
the compiled element tree directly in Rust, wrap the definition as
`ElementKind::GridControl(Box::new(definition))`. This keeps deeply nested forms
within the supported stack budget. XAML authors use the ordinary element below.

```xml
<GridControl DataSource="Orders" GroupFields="Region,City" GroupMode="Hierarchy" ShowGroupPanel="True" View="Table">
  <GridColumn FieldName="Region"/><GridColumn FieldName="City"/>
  <GridColumn FieldName="ClientId" Header="Client" Width="240"/>
  <GridColumn FieldName="Quantity"/><GridColumn FieldName="Price"/>
  <GridColumn FieldName="Amount" Expression='mul(to_decimal(field("Quantity")),field("Price"))'/>
  <GridControl.Summaries>
    <GridSummary Function="CountRows" Title="Rows"/>
    <GridSummary Function="Sum" FieldName="Amount" Title="Total" DisplayColumn="Amount"/>
  </GridControl.Summaries>
  <GridControl.Bands>
    <GridBand Header="Order"><GridBand Header="Location" Fields="Region,City"/>
      <GridBand Header="Sale" Fields="ClientId,Quantity,Price,Amount"/></GridBand>
  </GridControl.Bands>
  <GridControl.CellTemplates>
    <GridCellTemplate FieldName="Amount" Suffix=" ₽" Emphasis="True"/>
  </GridControl.CellTemplates>
  <GridControl.Editors>
    <GridEditor FieldName="Quantity" Factory="Numeric"/>
    <GridEditor FieldName="ClientId" Factory="Lookup" ItemsSource="Clients"
                ValueField="Id" DisplayFields="Name,City"/>
  </GridControl.Editors>
</GridControl>
```

`View` is `Table` or `Cards`. `GroupFields` is a comma-separated list of at most
eight unique field names. Empty `GroupFields` disables grouping. The default mode is
`Composite`. In `Hierarchy` each prefix gets its own expandable row. On a column,
`GroupExpression` accepts a typed expression such as `month_start(field("Date"))`
or `@display`. `GroupSortSummary` selects a zero-based summary index,
`GroupDescending` reverses the order, and `GroupFooter` places subtotals after children.

`GridSummary.Function` supports `CountRows`, `Count`, `Sum`, `Average`, `Min` and
`Max`. `CountRows` has no `FieldName`; every other function requires it. `Title`
defaults to the function name. The source validates field types and aggregate
operations. At most 16 summaries are supported.

A band has a `Header` and either leaf `Fields` or child `GridBand` elements.
There can be up to 16 root bands and 64 bands across eight levels. Siblings have
disjoint field sets. Set `StackedFields="Price=Quantity"` to place Price below
Quantity in the same band and `RightPinnedFields="Amount"` to fix Amount on the right. Summary titles and band headers are limited to
256 UTF-8 bytes without control characters.

`GridControl.Detail` contains exactly one `GridDetail`:

```xml
<GridControl.Detail>
  <GridDetail DataSource="Lines" MasterField="Id" ForeignField="OrderId">
    <GridColumn FieldName="Description"/>
    <GridColumn FieldName="Quantity"/>
  </GridDetail>
</GridControl.Detail>
```

The detail source is explicitly registered by the application. Detail columns
are required, limited to 64, and cannot outnumber the master columns. Details
are read-only and have one level; nested grids and arbitrary child content are
rejected. Source registration, field existence and relation types are checked
before mounting by the adapter.

`GridCellTemplate` is a limited cell presentation: a prefix, suffix, optional
built-in `Icon` and Boolean `Emphasis`. Prefix and suffix each allow 256 UTF-8
bytes. It does not contain a visual tree, event handlers or interactive controls.
At most one template can be declared per field, up to 64 fields.

`GridEditor` chooses the editing recipe for a field. `Factory` defaults to
`Auto`. Typed recipes include `Text`, `Numeric`, `Date`, `Time` and `CheckBox`.
`ComboBox` and `Lookup` require `ItemsSource`, `ValueField` and `DisplayFields`.
Text columns also support `Masked` with `Mask`, `Memo` for multiline text, and
`Button` with an optional registered `Command`. A conditional editor uses
`WhenField` and `WhenValue`. Each column may have one base and one conditional
editor. The condition is parsed with the schema of the referenced field.
An expression column is read-only. A writable application-supplied column uses
`ValueSource` and `ValueMember` and joins by the master row key.
A custom factory uses an explicitly registered identifier of at most 128 bytes.
This metadata does not instantiate arbitrary types. At most 64 editor entries
are supported. The adapter validates the recipe against the schema.

Each metadata property can occur once. The complete content uses the ordinary
XAML element/depth budgets. These presentation properties are literals;
`FilterText`, `SelectedValue`, `IsReadOnly` and common geometry/state retain their
existing binding contracts. The compiler rejects unsupported properties,
duplicate fields and invalid dimensions; source-dependent validation remains
the responsibility of the host.
