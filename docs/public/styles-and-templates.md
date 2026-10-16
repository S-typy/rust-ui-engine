# Styles, resources and templates

The form compiler resolves resources, styles and reusable presentations into
ordinary typed definitions. It does not load executable markup at runtime.
`parse`, `compile` and `ComponentRegistry::parse` share this profile.

## Resources and precedence

An element can declare one `<Element.Resources>` property. Its resources are
visible to that element and its descendants. A nearer scope shadows a key from
an ancestor; duplicate keys in one scope are errors. Forward references within
a scope are allowed. A resource's references resolve in its declaration scope,
including references inherited through a style.

Scalar resources are `String`, `Color`, `Double` and `Thickness`. Each requires
`x:Key` and `Value`. Colors use `#RRGGBB` or `#AARRGGBB`; numbers must be finite;
thickness accepts one, two or four nonnegative comma-separated values. Use a
scalar with `{StaticResource Key}`. Image bytes still belong to the application's
image registry; a string resource can supply a registered image name.

```xml
<Window xmlns="urn:rust-ui-engine:ui"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
  <Window.Resources>
    <Color x:Key="ActionInk" Value="#005FB8" />
    <Style x:Key="ActionBase" TargetType="Button">
      <Setter Property="Foreground" Value="{StaticResource ActionInk}" />
      <Setter Property="CornerRadius" Value="6" />
    </Style>
    <Style x:Key="Action" TargetType="Button" BasedOn="{StaticResource ActionBase}">
      <Setter Property="FontSize" Value="16" />
    </Style>
    <Style TargetType="Button" Selector="Button.primary">
      <Setter Property="MinWidth" Value="120" />
    </Style>
  </Window.Resources>
  <Button Content="Apply" Classes="primary" Style="{StaticResource Action}"
          FontSize="18" />
</Window>
```

An unkeyed style applies implicitly to its `TargetType`. A keyed style is
explicit unless it also declares `Selector`. Supported selectors are a type,
`Type.class`, and `Type#name`; `Classes` is a whitespace-separated identifier
list. State and descendant selectors are not part of this compiled profile.

Values are applied in this order, with later values winning:

1. Matching ancestor-scope styles, then matching nearer-scope styles.
2. Within each scope: type, class and name selectors; declaration order breaks ties.
3. The explicit `Style`, including its `BasedOn` chain.
4. Attributes on the consuming element, including bindings.

`BasedOn` requires the same target type. Unknown setters, duplicate setters,
invalid scalar values, missing references and inheritance cycles are errors.
Omitted appearance properties retain the control's live theme defaults.
Class/name selection and static resources are compiled once; changing classes
or resource dictionaries at runtime is not supported.

## Control templates

`ControlTemplate` changes the decorative content of a `Button` or
`ToggleButton`. The existing control continues to own its command, checked
state, focus, tooltip, accessible name, hit testing and state painting.

```xml
<Window.Resources>
  <ControlTemplate x:Key="ActionVisual" TargetType="Button">
    <StackPanel Orientation="Horizontal" Spacing="8">
      <Icon Symbol="Check" />
      <ContentPresenter x:Name="PART_Content"
                        Foreground="{TemplateBinding Foreground}" />
    </StackPanel>
  </ControlTemplate>
</Window.Resources>
<!-- Inside the same Window: -->
<Button Content="Apply" Template="{StaticResource ActionVisual}" />
```

Exactly one leaf `ContentPresenter` named `PART_Content` is required. It becomes
a typed `TextBlock` presenting the owner's `Content`. Layout, text, icons and
images may surround it; interactive descendants are rejected. Other behavioral
owners, including text editors, do not accept `ControlTemplate`.

`TemplateBinding` projects an owner attribute into a visual property. Supported
owner attributes are `Content`, `Header`, `Foreground`, `FontSize`, `FontWeight`,
`Background`, `BorderBrush`, `Padding` and `CornerRadius`. Missing optional
attributes leave the visual's default intact. If the owner attribute contains a
binding, its source/context is preserved as a read-only projection. Literal
projections are construction-time values; changing the owner's literal later
does not automatically rebuild the presentation.

`CommonProperties::control_template` contains a boxed
`ControlTemplateDefinition` with its target, isolated content-part name and
visual tree. Hosts call `validate_control_template` before installing a
manually constructed definition. A target mismatch, absent/duplicate part,
wrong part type or interactive visual is rejected before replacement.

## Header and data templates

`HeaderTemplate` resources apply to `GroupBox`, `Expander` and `GroupTab` through
`HeaderTemplate="{StaticResource Key}"`. An optional `TargetType` restricts the
owner. Inline `GroupBox.HeaderTemplate`, `Expander.HeaderTemplate` and
`GroupTab.HeaderTemplate` remain available. Header visuals are decorative;
the original header keeps its disclosure/tab action and accessible caption.
The compiled control/header visual root defaults to vertical centering. An
explicit alignment, including one supplied by a style, takes precedence.

`DataTemplate` supplies a reusable element tree, including interactive editors:

```xml
<Window.Resources>
  <DataTemplate x:Key="CustomerCard">
    <StackPanel Spacing="8">
      <TextBlock Text="Customer" />
      <TextBox x:Name="NameEditor" Text="{Binding Name}" />
    </StackPanel>
  </DataTemplate>
</Window.Resources>
<!-- Inside the same Window: -->
<ContentPresenter x:Name="Customer" DataContext="SelectedCustomer"
                  ContentTemplate="{StaticResource CustomerCard}" />
```

The presenter compiles to a transparent host with the consumer's common
properties and data context. The template's root retains a separate identity.
Every use receives private names. Local `ElementName` and MultiBinding element
references are renamed together with their targets; repeated instances cannot
capture one another. Templates use the enclosing resource declaration scope;
declare their resources outside the template's visual tree.

## Retained replacement and bounds

The form adapter's `replace_control_template` and
`replace_group_header_template` validate and initialize a candidate decorative
subtree before attaching it. They retain the owner and its original binding
engine, and remove the old decorative subtree only after the replacement is
ready. Group content, editor drafts and focus survive header replacement.
Runtime replacement creates a fresh private namescope even when the same
definition is reused. Failed part, target or binding configuration leaves the
old presentation installed.

Decorative templates permit `OneWay` and `OneTime` bindings. They cannot write
to the shared model during candidate initialization. Data templates retain the
ordinary binding modes of their controls. The runtime replacement API concerns
decorative control/header presentations; it does not migrate arbitrary data
template editor trees between different structures.

There are at most 256 resource declarations and 256 template instances per
source, with bounded reference depth of 32. Expanded content still observes the
256-element/32-depth form limits and the 1 MiB source/payload limit. The writer
checks its budget before appending output or source-location mappings. Errors
refer to the originating source declaration or use. No external dictionaries,
network resources, dynamic markup extensions or vendor assets are loaded.
