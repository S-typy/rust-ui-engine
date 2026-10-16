# Slider and RangeSlider

`Controls::slider(parent, SliderSettings, style)` creates a numeric slider.
`Controls::range_slider(parent, RangeSliderSettings, style)` creates an interval
with two retained, independently focusable thumbs. Both controls use theme
colors and density, support horizontal and vertical axes, and preserve their
values when the theme or viewport changes. `ZoomControl` remains the specialized
percentage control described in [status bars](status-bars.md).

Settings require finite, increasing `minimum` and `maximum`, a finite span,
contained values and a positive finite `step`. Range values must satisfy
`minimum <= lower <= upper <= maximum`. Invalid settings leave the previous
configuration intact. The optional positive finite `tick_frequency` is relative
to the minimum; its span quotient must remain finite. Tick painting is bounded
to 101 marks even for very dense scales.

Pointer and keyboard interaction use the same grid, anchored at `minimum`.
By default its interval is `step`; `snap_to_ticks` selects `tick_frequency`, or
falls back to `step` when no tick frequency is supplied. Explicit setters and bindings preserve a valid exact value
without snapping it. `reversed` reverses the physical direction; a normal
vertical slider has its minimum at the bottom.

Arrow keys along the axis change the value by one grid interval. Ctrl multiplies that
step by ten; PageUp/PageDown apply ten steps. Home/End request the minimum and
maximum. Pointer input captures the gesture; clicking a range track chooses the
nearest thumb. Escape, focus loss, native capture cancellation or disabling the
control restores both values from the start of the active pointer gesture.
Release accepts the current values. Programmatic configuration ends a gesture
and applies the requested configuration.

`ThumbCrossing::Clamp` stops a moving thumb at the other value.
`ThumbCrossing::Swap` exchanges the moving endpoint when it crosses the other
thumb; focus follows that endpoint. The retained lower and upper widget IDs
remain stable. `set_range_slider_values` validates and changes the ordered pair
atomically. `set_slider_value` accepts a scalar slider or one range thumb ID.
Remove the range container or an ancestor rather than a required thumb.

Inspect values through `slider_value`, `slider_settings`,
`range_slider_values`, `range_slider_settings` and `range_slider_thumbs`.
Configuration methods validate the full next settings before mutating state.
Each scalar slider and range thumb exposes a Slider role, numeric limits, step,
orientation and Focus/SetValue actions. The range parent is an accessible group,
not a third value. Native adapters discard nonfinite or out-of-range numeric
requests without ending the application loop. Native screen reader behavior
requires separate platform testing.

## Compiled XAML

```xml
<Slider Minimum="0" Maximum="100" Value="{Binding Level}"
        Step="5" TickFrequency="10" SnapToTicks="True" />
<RangeSlider Minimum="0" Maximum="100"
             LowerValue="{Binding Lower}" UpperValue="{Binding Upper}"
             Step="5" CrossingPolicy="Swap" />
```

Both elements accept `Orientation="Horizontal|Vertical"`, `IsReversed`, common
size/margin/alignment/visibility properties and an automation name. XAML's
default tick frequency is 10. `Slider.Value` and the range's `LowerValue` and
`UpperValue` participate in typed numeric bindings. A containing layout owns
their available size; axis-specific defaults apply only when size was omitted.
