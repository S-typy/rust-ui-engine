# Attached badges

A badge adds a count, short text or a built-in vector icon to an existing
control. It does not create a layout child, change the owner's desired size,
capture input or create a focus or accessibility target.

```rust,ignore
ui.set_badge(button, Some(BadgeSettings {
    content: BadgeContent::Count(150),
    label: "Unread messages".into(),
    severity: BadgeSeverity::Info,
    ..Default::default()
}))?;
```

`set_badge(owner, None)` removes the attachment. `badge(owner)` returns its
settings. `badge_bounds(owner)` returns its unclipped logical rectangle after
layout, or `None` when hidden, detached or wholly clipped. It is not a hit-test
rectangle. Replacing a badge keeps the owner's `WidgetId`, command, document,
selection, focus and binding endpoints.

`BadgeContent` accepts an exact `u64` count, nonempty `Text(String)`, or an
`IconSymbol`. Counts above `max_count` display the cap followed by `+`; the
default cap is 99. The accessible description always includes the full count.
Long visual text is fitted at grapheme boundaries with an ellipsis. Its full
content remains available in the description.

`BadgeSeverity` offers Neutral, Info, Success, Warning and Error. Severity is
also described in text. For a layout-only owner, the native accessibility bridge exposes the existing
owner as a group while its supplementary description is visible, so native
container filtering does not discard the information. Its identity, children
and lack of focus actions remain unchanged.

High-contrast themes use the theme's foreground and
surface colors. A disabled owner keeps a muted badge and its description.

## Position and lifetime

`BadgeAnchor` places the badge's center at TopLeft, TopRight, BottomLeft,
BottomRight or Center of the owner's border box. TopRight is the default.
`offset` adds logical x/y coordinates. Corners intentionally allow the badge
to extend beyond the owner; provide surrounding space when needed.

`BadgeClip::Parent` uses the owner's resolved ancestor clip and window viewport.
`BadgeClip::Owner` additionally intersects that clip with the owner's border
box. The badge uses the owner's base or popup scene layer and is painted after
the owner's descendants. It does not cover later siblings through an unrelated
global overlay. Ordinary scene scaling applies to its shape, glyphs and clip.

Hiding or collapsing the owner or any ancestor suppresses both the visual
indicator and its supplementary description. Showing the same owner restores
the attachment. Detaching retains the settings but suppresses presentation;
removing the owner discards them. Scrolling and layout changes reposition the
badge using the current arranged geometry.

## XAML

```xml
<Button Content="Inbox"
        Badge.Count="{Binding Unread}"
        Badge.Label="Unread messages"
        Badge.Severity="Info"
        Badge.Visibility="{Binding ShowUnread}"
        Badge.Anchor="TopRight"
        Badge.Offset="-8,0"
        Badge.Clip="Parent"
        Badge.MaxCount="99"/>
```

Specify exactly one of `Badge.Count`, `Badge.Text` or `Badge.Icon`. Other
attributes are `Badge.Label`, `Badge.Severity`, `Badge.Anchor`, `Badge.Offset`,
`Badge.Clip`, `Badge.MaxCount` and `Badge.Visibility`.

`Badge.Count`, `Badge.Text` and `Badge.Visibility` support OneWay and OneTime
bindings. Count bindings use a canonical decimal `DataValue::String`, including
values above the exact integer range of `f64`; numeric values, signs, leading
zeros and overflow are rejected. Text bindings require a nonempty string.
Visibility bindings accept a boolean or `Visible`, `Hidden` or `Collapsed`.
Literal visibility accepts these three names; Hidden and Collapsed have the
same effect because a badge occupies no layout space.

The Rust endpoints are `BindingProperty::BadgeCount`, `BadgeText` and
`BadgeVisibility`. Invalid updates retain the previous attachment and produce
the binding host's ordinary diagnostic. Badge bindings never write back to the
source. The separate owner bindings keep their existing semantics.

Badges apply to ordinary mounted content elements. The bounded item-projection
profile of `ListBox.ItemTemplate` does not accept attached badges. Badge icons,
severity and positioning are literal XAML metadata; dynamic changes use
`set_badge`.

## Bounds and validation

- At most 1024 attachments per `Controls` instance.
- Text and the optional accessible label: at most 64 graphemes and 256 UTF-8
  bytes each, with no control characters. Text cannot be empty.
- Offset coordinates must be finite and within −256 through 256 logical pixels.
- `max_count` must be positive; counts and caps retain all `u64` bits.
- Visual size is bounded to 22 pixels high and 22–160 pixels wide, with
  12-pixel semibold text. These dimensions are logical pixels.
- The complete displayed and accessible strings are checked before publication,
  including grapheme joins at caption and suffix boundaries. The text engine's
  shaping limits also apply.

Invalid settings leave the previous settings, clipping mode and description
unchanged. No arbitrary visual child, animation, tooltip lifecycle or interactive
action is introduced by an attachment.

`BadgeWindow.xaml` demonstrates bound counts, text and icons on buttons, an
editor and a grid, wrapping layout, visibility and clipping. Regression tests
cover exact count bindings, owner state retention, scene layers, clipping,
removal, capacity, malformed text and accessibility description composition.
