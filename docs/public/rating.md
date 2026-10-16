# Rating

`Rating` displays a nullable score as a row of stars and a numeric caption.
The empty value, zero and a fractional score are distinct accepted values.
Stars use bounded vector geometry supplied by the control; no symbol font or
external artwork is required.

```xml
<Rating x:Name="Quality"
        Value="{Binding Quality}"
        Maximum="5" Step="0.5"
        AllowEmpty="True" IsReadOnly="False"
        GlyphSize="24" Spacing="4"/>
```

`Value` is a nullable number. Its binding defaults to `TwoWay` and
`PropertyChanged`, but publishes only the accepted value. A literal `Value="null"`
or an omitted value selects the empty state; `Value="0"` selects zero.
`IsReadOnly` and the common layout, visibility and enabled properties also accept
bindings. `FallbackValue` for `Value` accepts a finite number or `null`.
The control has no child content or item template.

## Value and validation

`Controls::rating(parent, label, settings, layout)` returns the retained owner
`WidgetId`. `RatingSettings` contains:

| Field | Default | Accepted range |
| --- | --- | --- |
| `value` | `None` | `None` when allowed, otherwise finite `0..=maximum` |
| `maximum` | `5` | Integer `1..=20` |
| `step` | `1.0` | Finite `0.01..=1.0` |
| `allow_empty` | `true` | Boolean |
| `read_only` | `false` | Boolean |
| `glyph_size` | `24` | Finite `8..=64` logical pixels |
| `spacing` | `4` | Finite `0..=32` logical pixels |

Programmatic values preserve their fractional precision; they are not rounded to
`step`. The step controls pointer and keyboard input. Invalid settings, non-finite
values and values outside the range are rejected before changing the accepted
state or layout. `allow_empty=false` requires a non-empty value. A bound XAML
control initializes from its model value after mounting.

`rating_value` returns `Some(None)` for an empty rating and `None` for an unknown
owner. `rating_preview` distinguishes no preview from a previewed value.
`set_rating_value`, `set_rating_settings` and `set_rating_read_only` update the
existing control. Changing the theme or viewport preserves its identity and score.

Use `RatingValidator` with `set_rating_validator` or `chain_rating_validator` to
check application rules. Two-way bindings install their source validator on the
same acceptance path. Rejected input keeps both the score and source value,
clears the transient preview, and exposes `rating_error`; the semantic node also
reports the validation message. An error outline marks the control visually.

`drain_rating_changed` returns accepted user changes. Repeated confirmation of the
same score does not emit another event. Programmatic setters do not emit these
events. The queue holds 256 changes; applications must drain it. When full, a new
user change is rejected without changing the score.

## Input

| Input | Effect |
| --- | --- |
| Pointer hover or drag | Preview only |
| Primary pointer release inside the owner | Accept the pointed score |
| Release outside, capture loss, window blur | Cancel preview |
| Right/Up or Left/Down | Increase or decrease keyboard preview by `step` |
| Home / End | Preview zero / maximum |
| Enter / Space | Accept the preview |
| Delete | Clear when `allow_empty=true` |
| Escape | Cancel an active preview |

Moving keyboard focus, hiding, disabling or removing the owner cancels transient
state. Read-only controls remain inspectable and focusable but reject user and
accessibility changes. Explicit programmatic updates remain available.

## Accessibility and rendering

The semantic role is `Rating`, with an accepted score and maximum in its value
description. The Windows bridge exposes a custom rating description and numeric
minimum, maximum and step. Its native numeric-range role supports writable
RangeValue while the role description identifies it as a rating. A non-empty
score supplies the platform numeric value;
the empty state keeps that value absent instead of substituting zero. Numeric
range support therefore follows the platform's available pattern for that state.

`Increment`, `Decrement` and `SetValue` are immediate acceptance operations. They
use the same validation, enabled and read-only gates as pointer and keyboard
confirmation. String `SetValue` with `null` or an empty string requests an empty
score; invalid numeric input is ignored or reported as a validation error without
terminating the event loop. Keyboard previews appear in the accessibility
description and do not replace the accepted numeric value.

The stars shrink within a smaller viewport while retaining a separate score
caption. Their order is left to right. The control currently uses the built-in
star shape; custom symbols and per-star templates are outside this contract.

Compiler, retained runtime, bindings and accessibility projection have separate
regression tests. Projection tests exercise the adapter's action routing; they
do not by themselves establish a native screen-reader or GPU result.
