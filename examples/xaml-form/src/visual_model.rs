//! Application-owned themes and example image resources.
use rust_desktop_ui_controls::{
    Controls, ImageSource, Theme, ThemeConfig, ThemeDensity, ThemeMode,
};
use rust_desktop_ui_xaml as xaml;

pub(crate) fn theme(
    mode: xaml::Theme,
    density: xaml::ThemeDensity,
    accent: Option<[u8; 3]>,
) -> Result<Theme, String> {
    let mode = match mode {
        xaml::Theme::Light => ThemeMode::Light,
        xaml::Theme::Dark => ThemeMode::Dark,
        xaml::Theme::HighContrastLight => ThemeMode::HighContrastLight,
        xaml::Theme::HighContrastDark => ThemeMode::HighContrastDark,
    };
    let density = match density {
        xaml::ThemeDensity::Comfortable => ThemeDensity::Comfortable,
        xaml::ThemeDensity::Compact => ThemeDensity::Compact,
        xaml::ThemeDensity::Touch => ThemeDensity::Touch,
    };
    if density == ThemeDensity::Comfortable && accent.is_none() {
        return Ok(match mode {
            ThemeMode::Light => Theme::FluentLight,
            ThemeMode::Dark => Theme::FluentDark,
            ThemeMode::HighContrastLight => Theme::HighContrastLight,
            ThemeMode::HighContrastDark => Theme::HighContrastDark,
        });
    }
    let mut config = ThemeConfig::new(mode).with_density(density);
    if let Some(accent) = accent {
        config = config.with_accent(accent).map_err(|e| e.to_string())?;
    }
    Ok(Theme::Configured(config))
}

pub(crate) fn register_images(ui: &mut Controls) -> Result<(), String> {
    // Original geometry: an abstract landscape and a currentColor compass.
    // Resource decoding cannot access files, network or system fonts.
    const LANDSCAPE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="240" height="120" viewBox="0 0 240 120"><defs><linearGradient id="sky" x2="0" y2="1"><stop stop-color="#41b8e6"/><stop offset="1" stop-color="#d8f3f0"/></linearGradient></defs><rect width="240" height="120" rx="12" fill="url(#sky)"/><circle cx="187" cy="30" r="15" fill="#ffe09a"/><path d="M0 112L69 39L140 120H0Z" fill="#376d82"/><path d="M60 120L150 48L240 112V120Z" fill="#427b63"/><path d="M69 39L50 59L70 55L84 59Z" fill="#f7f9fd"/></svg>"##;
    const COMPASS: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64"><circle cx="32" cy="32" r="27" fill="none" stroke="currentColor" stroke-width="3"/><path d="M42 19L36 36L20 45L27 27Z" fill="currentColor"/></svg>"##;
    for (name, data) in [
        ("Landscape", LANDSCAPE),
        ("Compass", COMPASS),
        ("Broken", "<svg>"),
    ] {
        let source = ImageSource::from_bytes(
            data.as_bytes().to_vec(),
            rust_desktop_ui_controls::ImageFormat::Svg,
        )
        .map_err(|e| e.to_string())?;
        ui.register_image_source(name, source)
            .map_err(|e| e.to_string())?;
    }
    // Original 2x2 RGBA checker, encoded as PNG.
    const CHECKER: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 2, 8, 6,
        0, 0, 0, 114, 182, 13, 36, 0, 0, 0, 22, 73, 68, 65, 84, 120, 156, 99, 96, 136, 223, 241,
        255, 255, 49, 159, 255, 12, 96, 2, 200, 1, 0, 89, 226, 10, 77, 76, 54, 151, 197, 0, 0, 0,
        0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let checker =
        ImageSource::from_bytes(CHECKER.to_vec(), rust_desktop_ui_controls::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
    ui.register_image_source("Checker", checker)
        .map_err(|e| e.to_string())?;
    Ok(())
}
