//! Generate barcode vector/raster fixtures from the same checked geometry.
use rust_desktop_ui_controls::barcode::*;
use rust_desktop_ui_images::{DecodeOptions, ImageFormat, ImageSource};
use serde_json::json;
use std::{error::Error, fs, io::Write, path::Path};

fn fixture(
    directory: &Path,
    name: &str,
    symbol: &BarcodeSymbol,
    geometry: &BarcodeGeometry,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let pgm = directory.join(format!("{name}.pgm"));
    geometry.write_pgm(&mut fs::File::create(&pgm)?)?;
    let svg = geometry.to_svg();
    fs::write(directory.join(format!("{name}.svg")), &svg)?;
    let svg_scale = if geometry.width() > 4096 {
        1.0 / f32::from(geometry.module_size())
    } else {
        1.0
    };
    let asset =
        ImageSource::from_bytes(svg.into_bytes(), ImageFormat::Svg)?.decode(DecodeOptions {
            svg_scale,
            ..Default::default()
        })?;
    let bitmap = asset.bitmap();
    let mut raster = fs::File::create(directory.join(format!("{name}-svg.pgm")))?;
    write!(raster, "P5\n{} {}\n255\n", bitmap.width(), bitmap.height())?;
    for pixel in bitmap.pixels().chunks_exact(4) {
        // A fractional final SVG scanline can extend beyond its viewport.
        // Composite straight RGBA on the same opaque white output surface.
        let alpha = u32::from(pixel[3]);
        let gray = ((u32::from(pixel[0]) * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
        raster.write_all(&[gray])?;
    }
    Ok(
        json!({"name":name,"data":symbol.data(),"format":symbol.symbology().name(),
        "width":geometry.width(),"height":geometry.height(),"module_size":geometry.module_size(),"svg_raster_scale":svg_scale}),
    )
}
fn main() -> Result<(), Box<dyn Error>> {
    let output = std::env::args_os()
        .nth(1)
        .ok_or("Pass an output directory")?;
    let output = Path::new(&output);
    fs::create_dir_all(output)?;
    let mut fixtures = vec![];
    let cases = [
        (BarcodeSymbology::Ean13, "400638133393".to_owned()),
        (BarcodeSymbology::Ean13, "000000000000".to_owned()),
        (BarcodeSymbology::Ean8, "9638507".to_owned()),
        (
            BarcodeSymbology::Code128B,
            "Rust UI / 2026-10-17".to_owned(),
        ),
        (
            BarcodeSymbology::Code128B,
            (32..=126).map(char::from).collect(),
        ),
        (BarcodeSymbology::Code128B, "Z".repeat(128)),
    ];
    for (case, (kind, data)) in cases.iter().enumerate() {
        let symbol = BarcodeSymbol::encode(*kind, data)?;
        for pitch in [1, 2, 3, 5] {
            let geometry = BarcodeGeometry::new(
                &symbol,
                BarcodeGeometryOptions {
                    module_size: pitch,
                    bar_height: 96,
                    ..Default::default()
                },
            )?;
            fixtures.push(fixture(
                output,
                &format!("case{case}-pitch{pitch}"),
                &symbol,
                &geometry,
            )?);
        }
        for dpi in [96, 203, 300, 600] {
            let layout = BarcodePrintLayout::new(
                &symbol,
                BarcodePrintOptions {
                    dpi,
                    ..Default::default()
                },
            )?;
            let mut item = fixture(
                output,
                &format!("case{case}-dpi{dpi}"),
                &symbol,
                &layout.geometry,
            )?;
            item["dpi"] = json!(dpi);
            item["actual_module_mm"] = json!(layout.actual_module_mm);
            fixtures.push(item);
        }
    }
    fs::write(
        output.join("fixtures.json"),
        serde_json::to_vec_pretty(&fixtures)?,
    )?;
    println!(
        "Generated {} barcode fixtures and SVG rasterizations",
        fixtures.len()
    );
    Ok(())
}
