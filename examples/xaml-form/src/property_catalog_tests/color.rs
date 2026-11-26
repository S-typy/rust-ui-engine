use super::*;
use rust_desktop_ui_controls::{ColorEditor, ColorPopup};
use rust_desktop_ui_core::{Bitmap, Srgba8, WidgetId};
use rust_desktop_ui_images::{PixelCrop, crop_bitmap, encode_bitmap_png};
use rust_desktop_ui_render_wgpu::OffscreenRenderer;
use rust_desktop_ui_xaml::{Theme, WindowDefinition};

const SIZE: Size = Size::new(700., 760.);
const SCALE: f32 = 1.5;
const ACCENT: Srgba8 = Srgba8::rgb(40, 118, 155);

struct ColorRow {
    form: Form,
    row: Rect,
    parts: ColorEditor,
}

impl ColorRow {
    fn new(mut definition: WindowDefinition, theme: Theme) -> Self {
        definition.theme = theme;
        let mut form = Form::new(definition, false).unwrap();
        form.event(PlatformEvent::Input(InputEvent::WindowFocused(true)))
            .unwrap();
        form.update(SIZE, SCALE).unwrap();
        form.update(SIZE, SCALE).unwrap();
        let grid = form.view.property_grids.values().next().unwrap();
        let row_id = grid.property_row("accent").unwrap();
        let parts = form
            .ui
            .color_editor_parts(grid.property("accent").unwrap())
            .expect("accent must mount a color editor in the compiled form");
        let viewport = grid.row_viewport().unwrap();
        let offset =
            form.ui.runtime.bounds(row_id).unwrap().y - form.ui.runtime.bounds(viewport).unwrap().y;
        form.ui.scroll_to(viewport, (offset - 60.).max(0.)).unwrap();
        form.update(SIZE, SCALE).unwrap();
        let row = form.ui.runtime.bounds(row_id).unwrap();
        Self { form, row, parts }
    }

    fn open_by_text(&mut self) {
        let run = self
            .form
            .ui
            .scene()
            .texts
            .iter()
            .find(|run| {
                run.text.parse::<Srgba8>() == Ok(ACCENT)
                    && run
                        .bounds
                        .intersection(run.clip)
                        .and_then(|ink| ink.intersection(self.row))
                        .is_some()
            })
            .expect("resting row must show its color code beside the swatch");
        let position = Point::new(run.bounds.x + 8., self.row.y + self.row.height / 2.);
        click(&mut self.form, position);
        self.form.update(SIZE, SCALE).unwrap();
        assert!(
            self.form.ui.color_popup_is_open(self.parts.container),
            "first press on the color code should open its palette"
        );
    }

    fn close_with_escape(&mut self) {
        press_key(&mut self.form, Key::Escape);
        self.form.update(SIZE, SCALE).unwrap();
        assert!(!self.form.ui.color_popup_is_open(self.parts.container));
    }

    fn check_active(&self) {
        let grid = self.form.view.property_grids.values().next().unwrap();
        assert_eq!(self.form.ui.runtime.focused(), Some(self.parts.button));
        assert_eq!(
            self.form
                .ui
                .runtime
                .bounds(grid.property_row("accent").unwrap()),
            Some(self.row)
        );
        assert!(
            self.form
                .ui
                .runtime
                .bounds(grid.property_actions("accent").unwrap().1)
                .is_none()
        );
    }

    fn click_part(&mut self, id: WidgetId) {
        let b = self
            .form
            .ui
            .runtime
            .bounds(id)
            .expect("control must be visible");
        let center = Point::new(b.x + b.width / 2., b.y + b.height / 2.);
        assert_eq!(self.form.ui.runtime.hit_test(center), Some(id));
        click(&mut self.form, center);
        self.form.update(SIZE, SCALE).unwrap();
    }

    fn open(&mut self) {
        self.click_part(self.parts.button);
        assert!(self.form.ui.color_popup_is_open(self.parts.container));
    }

    fn popup(&self) -> &ColorPopup {
        self.form.ui.color_popup(self.parts.container).unwrap()
    }

    fn spectrum(&self) -> (Rect, Rect) {
        let popup = self.popup();
        let bounds = |part: Option<WidgetId>| self.form.ui.runtime.bounds(part.unwrap()).unwrap();
        (bounds(popup.saturation_value), bounds(popup.hue))
    }

    fn popup_bounds(&self) -> Rect {
        self.form
            .ui
            .runtime
            .bounds(self.popup().flyout.container)
            .unwrap()
    }

    fn click_named(&mut self, name: &str) {
        let command = self
            .form
            .ui
            .semantics()
            .iter()
            .find(|node| node.label == name)
            .unwrap_or_else(|| panic!("missing visible command {name}"))
            .id;
        self.click_part(command);
    }

    fn show_custom_color(&mut self) {
        let more = self.popup().more_colors.unwrap();
        self.click_part(more);
        assert_eq!(self.form.ui.runtime.focused(), Some(self.popup().hex));
    }

    fn render(&self, gpu: &mut OffscreenRenderer) -> Bitmap {
        gpu.render(
            self.form.ui.scene(),
            [(SIZE.width * SCALE) as u32, (SIZE.height * SCALE) as u32],
            SCALE,
        )
        .unwrap()
    }
}

#[test]
fn color_palette_click_preserves_the_property_grids_commit_policy() {
    for (definition, source_name, keys) in [
        (
            crate::generated_properties::window_definition(),
            "InspectionObjects",
            &[1, 2][..],
        ),
        (
            crate::generated_property_editors::window_definition(),
            "InspectionObjects",
            &[1][..],
        ),
        (
            generated_property_catalog::window_definition(),
            "EditorCatalogData",
            &[1][..],
        ),
    ] {
        let mut example = ColorRow::new(definition, Theme::Light);
        let owner = example.parts.container;
        let source = example.form.ui.data_source(source_name).unwrap();
        let original = CellValue::Text("#28769B".into());
        example.open_by_text();
        example.close_with_escape();
        example.check_active();
        for &key in keys {
            assert_eq!(source.borrow().value(key, "accent"), Some(&original));
        }
        example.open();
        let swatch = example.popup().palette[4];
        let picked = example
            .form
            .ui
            .color_editor_settings(owner)
            .unwrap()
            .palette[4];
        let expected = CellValue::Text(picked.to_string());
        assert_ne!(picked, ACCENT);
        example.click_part(swatch);
        assert!(!example.form.ui.color_popup_is_open(owner));
        if keys.len() == 2 {
            for &key in keys {
                assert_eq!(source.borrow().value(key, "accent"), Some(&original));
            }
            example.click_named("Действия свойства «Цвет метки»");
            example.click_named("Применить");
        }
        for &key in keys {
            assert_eq!(source.borrow().value(key, "accent"), Some(&expected));
        }
    }
}

#[test]
fn custom_argb_in_the_catalog_preserves_transparent_rgb_and_respects_cancel() {
    let mut example = ColorRow::new(
        generated_property_catalog::window_definition(),
        Theme::Light,
    );
    let source = example.form.ui.data_source("EditorCatalogData").unwrap();
    for apply in [false, true] {
        example.open();
        example.show_custom_color();
        press_modified_key(
            &mut example.form,
            Key::Character("a".into()),
            Modifiers {
                control: true,
                ..Default::default()
            },
        );
        example
            .form
            .event(PlatformEvent::Text("#00123456".into()))
            .unwrap();
        example.form.update(SIZE, SCALE).unwrap();
        assert_eq!(
            source.borrow().value(1, "accent"),
            Some(&CellValue::Text("#28769B".into()))
        );
        if apply {
            let button = example.popup().apply;
            example.click_part(button);
            assert_eq!(
                source.borrow().value(1, "accent"),
                Some(&CellValue::Text("#00123456".into()))
            );
        } else {
            example.close_with_escape();
            assert_eq!(
                source.borrow().value(1, "accent"),
                Some(&CellValue::Text("#28769B".into()))
            );
        }
    }
    example.open();
    example.show_custom_color();
    let (plane, hue) = example.spectrum();
    let green = point_at(hue, 0.5, 1. / 3.);
    click(&mut example.form, green);
    pointer_down(&mut example.form, point_at(plane, 0.25, 0.7));
    let end = point_at(plane, 0.9, 0.2);
    example
        .form
        .event(PlatformEvent::Input(InputEvent::PointerMoved {
            position: end,
        }))
        .unwrap();
    pointer_up(&mut example.form, end);
    example.form.update(SIZE, SCALE).unwrap();
    assert_eq!(
        source.borrow().value(1, "accent"),
        Some(&CellValue::Text("#00123456".into()))
    );
    example.click_part(example.popup().apply);
    let source = source.borrow();
    let Some(CellValue::Text(value)) = source.value(1, "accent") else {
        panic!("expected a color value")
    };
    let actual = value.parse::<Srgba8>().unwrap();
    assert_eq!(actual.a, 0);
    assert!(
        actual.g > 185 && actual.r < 65 && actual.b < 65,
        "expected saturated green: {actual}"
    );
}

fn point_at(bounds: Rect, x: f32, y: f32) -> Point {
    Point::new(bounds.x + bounds.width * x, bounds.y + bounds.height * y)
}

fn verify_spectrum_pixels(bitmap: &Bitmap, plane: Rect, hue: Rect) {
    let sample = |position: Point| -> [u8; 3] {
        let x = (position.x * SCALE) as u32;
        let y = (position.y * SCALE) as u32;
        let i = ((y * bitmap.width() + x) * 4) as usize;
        bitmap.pixels()[i..i + 3].try_into().unwrap()
    };
    let white = sample(point_at(plane, 0.07, 0.07));
    let dark = sample(point_at(plane, 0.50, 0.94));
    let saturated = sample(point_at(plane, 0.92, 0.08));
    assert!(
        white.into_iter().all(|v| v > 175) && dark.into_iter().all(|v| v < 115),
        "missing light/dark range in the color field: {white:?}, {dark:?}"
    );
    assert!(
        saturated.iter().max().unwrap() - saturated.iter().min().unwrap() > 100,
        "missing saturation gradient: {saturated:?}"
    );
    for (y, channel) in [(0.025, 0), (1. / 3., 1), (2. / 3., 2)] {
        let rgb = sample(point_at(hue, 0.5, y));
        assert!(
            rgb[channel] > 180 && rgb[(channel + 1) % 3] < 150 && rgb[(channel + 2) % 3] < 150,
            "hue strip at {y}: {rgb:?}"
        );
    }
}

fn frame(bitmap: &Bitmap, name: &str, area: Rect) {
    let Some(dir) = std::env::var_os("PROPERTY_GRID_FRAME_DIR") else {
        return;
    };
    let left = (area.x * SCALE).floor().max(0.) as u32;
    let top = (area.y * SCALE).floor().max(0.) as u32;
    let right = (((area.x + area.width) * SCALE).ceil() as u32).min(bitmap.width());
    let bottom = (((area.y + area.height) * SCALE).ceil() as u32).min(bitmap.height());
    let image = crop_bitmap(
        bitmap,
        PixelCrop {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        },
    )
    .unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        std::path::Path::new(&dir).join(format!("{name}.png")),
        encode_bitmap_png(&image).unwrap(),
    )
    .unwrap();
}

/// Measure the colored interior separately from grid dividers and label ink.
fn swatch_bounds(bitmap: &Bitmap, row: Rect) -> (u32, u32, u32, u32) {
    let mut left = bitmap.width();
    let mut right = 0;
    let mut top = bitmap.height();
    let mut bottom = 0;
    for y in (row.y * SCALE).ceil() as u32..((row.y + row.height) * SCALE).floor() as u32 {
        for x in (row.x * SCALE).ceil() as u32..((row.x + row.width) * SCALE).floor() as u32 {
            let i = ((y * bitmap.width() + x) * 4) as usize;
            if [ACCENT.r, ACCENT.g, ACCENT.b]
                .iter()
                .zip(&bitmap.pixels()[i..i + 3])
                .all(|(a, b)| a.abs_diff(*b) <= 2)
            {
                left = left.min(x);
                right = right.max(x);
                top = top.min(y);
                bottom = bottom.max(y);
            }
        }
    }
    assert!(left < right && top < bottom, "swatch absent from color row");
    assert!(
        (right - left).abs_diff(bottom - top) <= 2,
        "swatch must be square"
    );
    assert!(
        (11..=36).contains(&(bottom - top)),
        "swatch must fit the text row"
    );
    (left, top, right, bottom)
}

fn check_compact_focus(bitmap: &Bitmap, cell: Rect) {
    let left = (cell.x * SCALE).ceil() as u32;
    let right = ((cell.x + cell.width) * SCALE).floor() as u32;
    let top = (cell.y * SCALE).ceil() as u32;
    let bottom = ((cell.y + cell.height) * SCALE).floor() as u32;
    for y in top..bottom {
        let blue = (left..right)
            .filter(|&x| {
                let i = ((y * bitmap.width() + x) * 4) as usize;
                let rgb = &bitmap.pixels()[i..i + 3];
                rgb[2] > 110 && rgb[2].saturating_sub(rgb[0]) > 70
            })
            .count();
        assert!(
            blue < (right - left) as usize / 3,
            "full-width accent outline on focused color cell at pixel y={y}"
        );
    }
}

#[test]
#[ignore = "requires a hardware GPU adapter"]
fn render_property_color_rows_and_palette() {
    let mut gpu = pollster::block_on(OffscreenRenderer::new()).unwrap();
    eprintln!("GPU {:?}", gpu.adapter_info());
    for (definition, theme, label) in [
        (
            crate::generated_properties::window_definition(),
            Theme::Light,
            "documents",
        ),
        (
            generated_property_catalog::window_definition(),
            Theme::Dark,
            "catalog-dark",
        ),
    ] {
        let mut example = ColorRow::new(definition, theme);
        let row = example.row;
        let band = Rect::new(row.x, row.y - 10., row.width, row.height + 20.);
        let idle = example.render(&mut gpu);
        let resting_swatch = swatch_bounds(&idle, row);
        frame(&idle, &format!("{label}-rest"), band);
        example.open_by_text();
        example.close_with_escape();
        example.check_active();
        let active = example.render(&mut gpu);
        let cell = example
            .form
            .ui
            .runtime
            .bounds(example.parts.button)
            .unwrap();
        check_compact_focus(&active, cell);
        let active_swatch = swatch_bounds(&active, row);
        assert!(
            resting_swatch.0.abs_diff(active_swatch.0) <= 2
                && resting_swatch.1.abs_diff(active_swatch.1) <= 2,
            "activating the row moved the swatch: {resting_swatch:?} -> {active_swatch:?}"
        );
        frame(&active, &format!("{label}-active"), band);
        example.open();
        let b = example.popup_bounds();
        assert!(
            b.width < SIZE.width && b.height < SIZE.height / 2.,
            "quick palette must stay compact"
        );
        for &swatch in &example.popup().palette {
            let swatch = example.form.ui.runtime.bounds(swatch).unwrap();
            let visible = swatch
                .intersection(b)
                .expect("palette color must be visible");
            assert!(
                swatch.width - visible.width < 0.01 && swatch.height - visible.height < 0.01,
                "palette clips a color: {visible:?} of {swatch:?}"
            );
        }
        let top = band.y.min(b.y - 8.);
        let bottom = (band.y + band.height).max(b.y + b.height + 8.);
        frame(
            &example.render(&mut gpu),
            &format!("{label}-palette"),
            Rect::new(row.x, top, row.width + 8., bottom - top),
        );
        if theme == Theme::Light {
            example.show_custom_color();
            let b = example.popup_bounds();
            let detailed = example.render(&mut gpu);
            let (plane, hue) = example.spectrum();
            verify_spectrum_pixels(&detailed, plane, hue);
            frame(
                &detailed,
                "documents-custom-color",
                Rect::new(b.x - 6., b.y - 6., b.width + 12., b.height + 12.),
            );
        }
    }
}
