# Declarative grid presentation

`GridControl` accepts bounded presentation metadata alongside `GridColumn` or
`GridControl.Columns`. Existing grids default to table view without grouping,
summaries, bands, detail records or cell presentation overrides.

```xml
<GridControl DataSource="Orders" GroupFields="Category" View="Table">
  <GridColumn FieldName="Name" Width="240"/>
  <GridColumn FieldName="Amount"/>
  <GridControl.Summaries>
    <GridSummary Function="CountRows" Title="Rows"/>
    <GridSummary Function="Sum" FieldName="Amount" Title="Total"/>
  </GridControl.Summaries>
  <GridControl.Bands>
    <GridBand Header="Order" Fields="Name,Amount"/>
  </GridControl.Bands>
  <GridControl.CellTemplates>
    <GridCellTemplate FieldName="Amount" Suffix=" ₽" Emphasis="True"/>
  </GridControl.CellTemplates>
  <GridControl.Editors>
    <GridEditor FieldName="Amount" Factory="Numeric"/>
  </GridControl.Editors>
</GridControl>
```

`View` is `Table` or `Cards`. `GroupFields` is a comma-separated list of at most
eight unique field names. Empty `GroupFields` disables grouping.

`GridSummary.Function` supports `CountRows`, `Count`, `Sum`, `Average`, `Min` and
`Max`. `CountRows` has no `FieldName`; every other function requires it. `Title`
defaults to the function name. The source validates field types and aggregate
operations. At most 16 summaries are supported.

Each of at most 16 `GridBand` entries requires a `Header` and nonempty `Fields`.
Band field sets must be disjoint. Summary titles and band headers are limited to
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

`GridEditor` selects one existing typed recipe per field. `Factory` defaults to
`Auto`; built-in names are `Auto`, `Text`, `Numeric`, `Date`, `Time` and `CheckBox`.
A custom factory uses an explicitly registered identifier of at most 128 bytes.
This metadata does not instantiate arbitrary types. At most 64 editor entries
are supported. The adapter validates the recipe against the schema.

Each metadata property can occur once. The complete content uses the ordinary
XAML element/depth budgets. These presentation properties are literals;
`FilterText`, `SelectedValue`, `IsReadOnly` and common geometry/state retain their
existing binding contracts. The compiler rejects unsupported properties,
duplicate fields and invalid dimensions; source-dependent validation remains
the responsibility of the host.
