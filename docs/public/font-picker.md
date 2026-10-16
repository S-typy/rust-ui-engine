# FontPicker

`Controls::font_picker` combines a family selector, a face selector, a bounded
size editor and a shaped preview. The selectors use the same system font
collection as `TextEngine`; the list does not contain invented font families or
synthetic faces. Family discovery is cached per `Controls` instance. Face
metadata is read when a value changes, not during ordinary frame updates.

```rust,ignore
let picker = ui.font_picker(
    parent,
    "Document font",
    FontDescriptor { family: "Arial".into(), size: 18.0, ..Default::default() },
    FontPickerSettings::default(),
    LayoutStyle::default(),
)?;
```

`FontDescriptor` contains `family`, logical pixel `size`, integer `weight`, width
percentage `width`, and `FontSlant::{Normal, Italic, Oblique(angle)}`. FontPicker
accepts sizes 1–512, weights 1–1000, widths 25–400%, and oblique angles −45–45°.
The family is trimmed and a recognized family uses its catalog spelling.
A missing family retains its requested name, shown as a hint in the empty family
selector and in the status. It does not become a fabricated catalog entry.
`font_resolution` reports whether
the family has installed faces and the actual font names used for the preview.
Fallback coverage is determined by the text engine and installed fonts.

`font_value` is the accepted descriptor. `set_font_value` validates the entire
descriptor and shapes the preview before changing it; `FontValidator` can reject
a proposed selection. `take_font_changes` returns one event per accepted change.
The size editor keeps an invalid draft until correction or cancellation; it does
not publish an invalid descriptor. Read-only mode disables family, face and size
editing, including the size step buttons. A descriptor update during active text
composition is rejected before changing the fields or preview. Programmatic value assignment remains
available.

The `FontPicker` handle exposes stable IDs for its fields, preview and status.
These are owned implementation parts: remove the picker or its ancestor, and
attach validation to the picker. Do not replace their sources, validators,
editor settings or geometry independently. Family/style/size changes preserve
the retained fields. Preview height is bounded and large text can be clipped by
its viewport; descriptor size is never silently reduced to fit.

The binding `Value` is an object with exactly six fields:

```json
{"family":"Arial","size":18,"weight":400,"width":100,"style":"Italic","angle":0}
```

`style` is `Normal`, `Italic` or `Oblique`; `angle` is zero for the first two.
Use `font_descriptor_to_value` and `font_descriptor_from_value` in Rust. Two-way
binding validates the complete object before publishing a UI change. The picker
also supports `IsReadOnly` and ordinary size, visibility and enabled bindings.

```xml
<FontPicker x:Name="DocumentFont"
            Value="{Binding SelectedFont}"
            FontFamily="Segoe UI" FontSize="18"
            PreviewText="Aa Бб 123 · Sample" Width="640" />
```

Literal properties are `FontFamily`, `FontSize`, `FontWeight`, `FontWidth`,
`FontStyle`, `FontAngle`, `PreviewText` and `IsReadOnly`. `Value` requires a
binding, defaults to TwoWay/PropertyChanged, and does not accept a string
fallback. Individual descriptor fields are not separate binding endpoints.

`TextSpan::font` and `InlineSpan::font` apply a descriptor during shaping,
measurement and glyph rasterization. Explicit span size/weight overrides remain
available. With `font: None`, existing typography behavior is preserved.
Family, width, slant, size, weight and synthetic slant all participate in the
corresponding shaping or raster cache keys. Scene composition scales descriptor
sizes together with other text metrics.

The picker supports up to 64 instances per `Controls`, 16,384 catalog families,
1,024 physical faces per family, 256-byte family names and 4,096-byte preview
text. It exposes installed face attributes rather than an arbitrary variable
font axis editor. Catalog names and face attributes outside the supported
descriptor bounds are omitted. Installing fonts during a running process requires a new text
engine/Controls instance. Loading external font files, embedding fonts and
changing document content belong to the application.

`examples/xaml-form/FontWindow.xaml` and `font_model.rs` demonstrate descriptor
bindings, read-only mode, an unavailable requested family and a document preview
that uses the selected descriptor. The displayed actual font names describe the
picker's sample text; fallback may differ for other characters in a document.
