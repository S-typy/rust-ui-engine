# Measurement gauges

`Controls::gauge` presents a read-only measurement as a circular scale, linear
scale, digital value or state indicator. `GaugeSettings` is shared by all four
views. `set_gauge` replaces settings and value atomically; `set_gauge_value`
changes only the measurement. Failed validation leaves the previous value,
settings, retained widget and layout intact.

The minimum, maximum and value must be finite. Minimum is strictly less than
maximum, the span must be finite, and value must lie within the inclusive scale.
An out-of-range value is rejected; it is never silently clamped. These controls
do not edit values or represent process completion.

## Scale and state

`major_ticks` accepts 2–21 ticks including both endpoints, `minor_ticks` accepts
0–4 subdivisions between major ticks, and precision is 0–6 decimal places.
Ticks use the same numeric mapping as range bands, the needle and level marker.
Adjacent floating-point ticks that represent the same number are combined.
`GaugeSettings::ticks` returns an empty collection for invalid settings.

At most 16 `GaugeRange` intervals are accepted, in ascending order without
overlap. Each interval lies inside the scale and has a nonempty label plus a
semantic tone: `Neutral`, `Accent`, `Good`, `Warning` or `Danger`. Intervals are
half-open: a shared boundary belongs to the following interval. The scale
maximum belongs to a range ending at that maximum. Uncovered values have a
neutral state. State labels are always available as text; color is supplemental.

Unit text is limited to 32 graphemes and 128 UTF-8 bytes. Range labels are limited
to 64 graphemes and 256 bytes. Control characters are rejected. The final joined
value/unit/state text passes the same shaping limits as other controls before
publication. Scientific notation avoids excessively long numbers or formatting
a nonzero measurement as zero. This display rounding never changes the stored
`f64` value exposed by `gauge_value` and accessibility.

## Size and accessibility

The scale uses renderer-independent Scene strokes and fills. Tick labels become
less frequent at smaller sizes while the numeric mapping remains unchanged.
`gauge_display_status` returns `Full` or `Compact`. A viewport too small for the
scale, or one clipped by an ancestor, uses a compact value/state line instead of
a partly clipped needle or scale. Long text uses grapheme-safe ellipsis; the
complete value remains accessible. A zero-size or sub-line-height viewport cannot
display readable text and retains only its semantic value.

The retained role is `Meter`. Both example accessibility bridges publish numeric
value, minimum, maximum, a read-only flag and text describing the current state.
No SetValue, increment, activation or keyboard editing action is supplied. On
Windows the current AccessKit backend maps Meter to a native progress-bar control
type; the role description identifies it as a measurement. The component itself
does not reuse ProgressBar process semantics.

## Compiled XAML

```xml
<CircularGauge Value="{Binding Temperature}" Minimum="0" Maximum="100"
               Unit="°C" MajorTicks="6" MinorTicks="3" Precision="1">
  <GaugeRange Start="0" End="60" Label="Normal" Tone="Good"/>
  <GaugeRange Start="60" End="80" Label="Warning" Tone="Warning"/>
  <GaugeRange Start="80" End="100" Label="Hot" Tone="Danger"/>
</CircularGauge>
```

`LinearGauge`, `DigitalGauge` and `StateIndicator` use the same properties and
`GaugeRange` children. Value bindings accept finite numeric values in `OneWay`
or `OneTime` mode. A numeric `FallbackValue` is checked against the scale. An
invalid initial binding rejects the mount; a later invalid model value preserves
the last accepted display and records a binding diagnostic. Layout, visibility
and enabled bindings retain their ordinary behavior.

Run `xaml-form --gauges` for all four presentations, explicit live updates,
out-of-range rejection, compact sizing and Light/Dark commands. Model, Scene,
binding, compiler, real-form mount and accessibility projection tests cover the
supported profile. Native input, GPU screenshots and actual screen-reader output
are separate acceptance evidence. Animation, editable needles, three-dimensional
skins and dataset charting are not part of this component.
