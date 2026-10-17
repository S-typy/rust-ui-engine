# Linear barcodes

`rust_desktop_ui_controls::barcode` encodes EAN-13, EAN-8 and Code 128 Set B.
The retained `Controls::barcode` displays the same checked symbol as vector
rectangles. There are no barcode fonts, external images or platform dependencies
in the encoder.

| Format | Input | Check character |
| --- | --- | --- |
| EAN-13 | 12 payload digits or 13 complete ASCII digits | Calculated or verified, modulo 10 |
| EAN-8 | 7 payload digits or 8 complete ASCII digits | Calculated or verified, modulo 10 |
| Code 128 B | 1–128 printable ASCII characters, codes 32–126 | Weighted modulo 103, with Start B and Stop |

EAN canonical data includes its check digit. Code 128 does not switch to sets A/C
and does not accept FNC codes or GS1 application-identifier syntax. QR, Data Matrix,
scanning and camera input are outside this profile. Syntax/checksum validation
does not establish assignment of a product identifier or certify print quality.

## Retained display and bindings

```xml
<Barcode Symbology="Ean13" Value="400638133393"
         ModuleSize="3" BarHeight="72" ShowText="True"/>
<Barcode Symbology="Code128B" Value="{Binding ProductCode}"/>
```

`ModuleSize` is an integer from 1 to 16 in logical Scene units. `BarHeight` is
16–1024; EAN guard bars extend another five modules. Optional human-readable text
is outside the bars and quiet zones. Auto dimensions follow the symbol, while
explicit dimensions remain application-owned. The control never shrinks or
clips a symbol to fit: insufficient space produces a message and
`BarcodeDisplayStatus::InsufficientSpace` or `Clipped`. The accepted value remains
available. White background and black bars are retained in both themes.

`set_barcode` changes data and settings together. Invalid values, checksums or
settings leave the previous symbol and layout intact. `set_barcode_value` retains
the current settings. `barcode_symbol`, `barcode_geometry` and `barcode_value`
expose the accepted result. Accessibility exposes an image with its canonical
payload, without editing actions.

`Value` bindings accept strings and support OneWay/OneTime. Failed updates leave
the last valid image intact and report a binding diagnostic. An invalid initial
binding prevents mounting unless a valid fallback was supplied. Displaying a
calculated EAN check digit does not write it back to the application model.

## Export and printing

`BarcodeGeometry::new` resolves a single integer pitch for every bar, space and
quiet-zone boundary. Minimum quiet zones, in narrow modules, are 11 left/7 right
for EAN-13, 7/7 for EAN-8 and 10/10 for Code 128. Overrides can enlarge them.

`visit_rectangles` sends an opaque white background followed by black rectangles
to a vector or print adapter. `to_svg`, `write_pgm` and `append_scene` reuse those
rectangles. Output sides are limited to 32768 units; raster output is additionally
limited to 16777216 pixels. Writer errors may leave partial output, so applications
requiring atomic files should write a temporary file and publish it on success.

`BarcodePrintLayout::new` accepts DPI, module millimetres and bar-height
millimetres. It rounds module pitch to the nearest whole printer dot, half up,
rounds height upward, and reports `actual_module_mm`. It rejects pitches smaller
than one dot and dimensions outside the bounded geometry. Applications must use
the returned actual pitch when checking their physical requirements.

Scene coordinates are logical vector coordinates. They preserve the symbol's
proportions but do not promise physical-pixel snapping after an arbitrary display
DPI transform. `append_scene` rejects partial clips and placements where float
precision cannot preserve the module pitch, before adding commands. Printer
geometry uses integer dots independently of the screen renderer.

The `barcode_export` example writes symbols, SVG and raster fixtures:

```text
cargo run -p rust-desktop-ui-controls --example barcode_export -- output/barcodes
```

The encoding tables follow the symbol definitions in the
[GS1 General Specifications](https://ref.gs1.org/standards/genspecs/), version
26.0, sections 5.2 and 5.4. Independent decoding covers exported raster and
rasterized SVG at multiple pitches and printer resolutions. Physical paper,
scanner certification and device-specific printer drivers are not tested by
these export checks.
