use super::*;
use rust_desktop_ui_controls::PropertyGrid;
use rust_desktop_ui_core::Bitmap;
use rust_desktop_ui_images::encode_bitmap_png;
use rust_desktop_ui_platform_winit::CursorIcon;
use rust_desktop_ui_render_wgpu::OffscreenRenderer;
use rust_desktop_ui_xaml::{Theme, WindowDefinition};

const SIZE: Size = Size::new(660., 525.);
const SCALE: f32 = 1.5;

fn mount(mut definition: WindowDefinition, theme: Theme, size: Size) -> Form {
    definition.theme = theme;
    let mut form = Form::new(definition, false).unwrap();
    form.event(PlatformEvent::Input(InputEvent::WindowFocused(true)))
        .unwrap();
    form.update(size, SCALE).unwrap();
    form.update(size, SCALE).unwrap();
    form
}

fn grid(form: &Form) -> &PropertyGrid {
    form.view.property_grids.values().next().unwrap()
}

fn row(form: &Form, field: &str) -> Rect {
    form.ui
        .runtime
        .bounds(grid(form).property_row(field).unwrap())
        .unwrap()
}

fn separator(form: &Form, field: &str) -> Point {
    // Use the laid-out value column even when its display button is hidden by editing.
    let display = grid(form).property_actions(field).unwrap().0;
    let host = form.ui.runtime.tree().parent(display).unwrap();
    let bounds = form.ui.runtime.bounds(host).unwrap();
    Point::new(bounds.x - 1., bounds.y + bounds.height * 0.5)
}

fn move_to(form: &mut Form, position: Point, size: Size) {
    form.event(PlatformEvent::Input(InputEvent::PointerMoved { position }))
        .unwrap();
    form.update(size, SCALE).unwrap();
}

fn near(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 1.,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn dragging_another_rows_divider_preserves_text_focus_selection_and_source() {
    let mut form = mount(
        crate::generated_property_editors::window_definition(),
        Theme::Light,
        SIZE,
    );
    let source = form.ui.data_source("InspectionObjects").unwrap();
    let original = source.borrow().value(1, "author").unwrap().clone();
    let editor = grid(&form).property_editor("author").unwrap();
    let author = separator(&form, "author");
    click(&mut form, Point::new(author.x + 40., author.y));
    press_modified_key(
        &mut form,
        Key::Character("a".into()),
        Modifiers {
            control: true,
            ..Default::default()
        },
    );
    form.event(PlatformEvent::Text("Новый автор".into()))
        .unwrap();
    press_modified_key(
        &mut form,
        Key::ArrowLeft,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    let selection = form.ui.document(editor).unwrap().selection();
    assert_eq!(form.ui.document(editor).unwrap().selected_text(), "р");
    let baseline = row(&form, "author");
    let from = separator(&form, "department");
    let to = Point::new(from.x + 82., from.y);
    assert_eq!(form.pointer_cursor(from), CursorIcon::ColResize);
    assert_eq!(form.pointer_cursor(to), CursorIcon::Default);
    pointer_down(&mut form, from);
    assert_eq!(
        form.pointer_cursor(Point::new(1., 1.)),
        CursorIcon::ColResize
    );
    move_to(&mut form, to, SIZE);
    // Observe the geometry before release; a final-only resize is insufficient.
    for field in ["author", "department", "status", "accent"] {
        near(separator(&form, field).x, to.x);
    }
    pointer_up(&mut form, to);
    form.update(SIZE, SCALE).unwrap();
    assert_eq!(form.ui.runtime.focused(), Some(editor));
    assert_eq!(form.ui.document(editor).unwrap().selection(), selection);
    assert_eq!(form.ui.document(editor).unwrap().text(), "Новый автор");
    assert_eq!(source.borrow().value(1, "author"), Some(&original));
    assert_eq!(row(&form, "author"), baseline);
    press_key(&mut form, Key::Enter);
    assert_eq!(
        source.borrow().value(1, "author"),
        Some(&CellValue::Text("Новый автор".into()))
    );
    // The platform may deliver a new release position without an intervening move.
    pointer_down(&mut form, to);
    form.event(PlatformEvent::Input(InputEvent::PointerLeft))
        .unwrap();
    pointer_up(&mut form, from);
    form.update(SIZE, SCALE).unwrap();
    near(separator(&form, "department").x, from.x);
    assert_eq!(form.pointer_cursor(to), CursorIcon::Default);
}

#[test]
fn scrolled_divider_clamps_and_cancels_without_resizing_full_width_cards() {
    let size = Size::new(425., 455.);
    let mut form = mount(
        crate::generated_properties::window_definition(),
        Theme::Dark,
        size,
    );
    let viewport = grid(&form).row_viewport().unwrap();
    let field = "edition";
    let visible = form.ui.runtime.bounds(viewport).unwrap();
    let offset = (row(&form, field).y - visible.y - 60.).max(0.);
    form.ui.scroll_to(viewport, offset).unwrap();
    form.update(size, SCALE).unwrap();
    assert!(form.ui.scroll_offset(viewport).unwrap().y > 0.);
    let from = separator(&form, field);
    assert!(visible.contains(from.x, from.y));
    let bounds = row(&form, field);
    let card = separator(&form, "cover").x;
    pointer_down(&mut form, from);
    move_to(&mut form, Point::new(-40., from.y), size);
    assert!(separator(&form, field).x < from.x - 20.);
    assert!(separator(&form, field).x >= bounds.x + 24.);
    near(separator(&form, "cover").x, card);
    move_to(&mut form, Point::new(size.width + 70., from.y), size);
    assert!(separator(&form, field).x > from.x + 20.);
    assert!(separator(&form, field).x <= bounds.x + bounds.width - 24.);
    press_key(&mut form, Key::Escape);
    form.update(size, SCALE).unwrap();
    assert!(form.ui.runtime.captured().is_none());
    near(separator(&form, field).x, from.x);
    near(separator(&form, "cover").x, card);
    pointer_up(&mut form, from);

    let old_scroll = form.ui.scroll_offset(viewport).unwrap().y;
    let card_offset = row(&form, "cover").y - visible.y - 10.;
    form.ui
        .scroll_to(viewport, old_scroll + card_offset)
        .unwrap();
    form.update(size, SCALE).unwrap();
    let card_row = row(&form, "cover");
    let card_hit = Point::new(from.x, card_row.y + 8.);
    assert!(visible.contains(card_hit.x, card_hit.y));
    assert_eq!(form.pointer_cursor(card_hit), CursorIcon::Default);
    pointer_down(&mut form, card_hit);
    let to = Point::new(card_hit.x + 55., card_hit.y);
    move_to(&mut form, to, size);
    pointer_up(&mut form, to);
    near(separator(&form, field).x, from.x);
}

#[test]
fn key_resize_retains_the_xaml_unit_across_window_resize_and_filtering() {
    for fractional in [false, true] {
        let mut definition = crate::generated_property_editors::window_definition();
        let rust_desktop_ui_xaml::ElementKind::PropertyGrid(settings) =
            &mut definition.content.kind
        else {
            panic!("expected property grid root");
        };
        if !fractional {
            settings.label_fraction = None;
            settings.label_width = 180.;
        }
        let mut form = mount(definition, Theme::Light, SIZE);
        let author = separator(&form, "author");
        click(&mut form, Point::new(author.x + 44., author.y));
        let focus = form.ui.runtime.focused();
        press_modified_key(
            &mut form,
            Key::ArrowRight,
            Modifiers {
                alt: true,
                shift: true,
                ..Default::default()
            },
        );
        form.update(SIZE, SCALE).unwrap();
        let widened = separator(&form, "department").x - row(&form, "department").x;
        let initial = author.x - row(&form, "author").x;
        assert!(widened > initial + 1.);
        assert_eq!(form.ui.runtime.focused(), focus);
        let old_span = row(&form, "author").width;
        let narrow = Size::new(450., SIZE.height);
        form.update(narrow, SCALE).unwrap();
        let expected = if fractional {
            widened * row(&form, "author").width / old_span
        } else {
            widened
        };
        near(
            separator(&form, "author").x - row(&form, "author").x,
            expected,
        );

        let (search, _, sorted) = grid(&form).toolbar_parts();
        form.ui
            .perform_action(
                sorted,
                rust_desktop_ui_controls::SemanticAction::Activate,
                None,
            )
            .unwrap();
        form.ui.set_text(search, "цвет метки").unwrap();
        form.update(narrow, SCALE).unwrap();
        form.update(narrow, SCALE).unwrap();
        assert!(
            form.ui
                .runtime
                .bounds(grid(&form).property_row("author").unwrap())
                .is_none()
        );
        let color = row(&form, "accent");
        let expected = if fractional {
            widened * color.width / old_span
        } else {
            widened
        };
        near(separator(&form, "accent").x - color.x, expected);
    }
}

#[test]
fn narrow_window_keeps_the_fixed_divider_reachable_then_restores_its_width() {
    let mut definition = crate::generated_property_editors::window_definition();
    let rust_desktop_ui_xaml::ElementKind::PropertyGrid(settings) = &mut definition.content.kind
    else {
        panic!("expected property grid root");
    };
    settings.label_fraction = None;
    settings.label_width = 460.;
    let mut form = mount(definition, Theme::Light, SIZE);
    let before = separator(&form, "author");
    let chosen = Point::new(before.x + 42., before.y);
    pointer_down(&mut form, before);
    pointer_up(&mut form, chosen);
    form.update(SIZE, SCALE).unwrap();
    near(separator(&form, "author").x, chosen.x);

    let narrow = Size::new(350., SIZE.height);
    form.update(narrow, SCALE).unwrap();
    let col = separator(&form, "author");
    let bounds = row(&form, "author");
    let row_id = grid(&form).property_row("author").unwrap();
    let layout = form.ui.runtime.layout_bounds(row_id);
    let tracks = &form
        .ui
        .runtime
        .tree()
        .node(row_id)
        .unwrap()
        .props
        .style
        .grid_columns;
    assert!(
        bounds.x + bounds.width - col.x >= 79.,
        "row={bounds:?}, local={layout:?}, tracks={tracks:?}, divider={col:?}"
    );
    assert_eq!(form.pointer_cursor(col), CursorIcon::ColResize);
    form.update(SIZE, SCALE).unwrap();
    near(separator(&form, "author").x, chosen.x);
}

fn divider_contrast(bitmap: &Bitmap, x: f32, y: f32) -> u16 {
    let x = (x * SCALE).round() as u32;
    let y = (y * SCALE).round() as u32;
    let pixel = |x| {
        let i = ((y * bitmap.width() + x) * 4) as usize;
        &bitmap.pixels()[i..i + 3]
    };
    let surface = pixel(x - 5);
    (x - 2..=x + 2)
        .map(|at| {
            pixel(at)
                .iter()
                .zip(surface)
                .map(|(a, b)| u16::from(a.abs_diff(*b)))
                .sum()
        })
        .max()
        .unwrap()
}

#[test]
#[ignore = "requires a hardware GPU adapter"]
fn render_property_column_resize() {
    let mut gpu = pollster::block_on(OffscreenRenderer::new()).unwrap();
    eprintln!("GPU {:?}", gpu.adapter_info());
    for (theme, name) in [(Theme::Light, "light"), (Theme::Dark, "dark")] {
        let mut form = mount(
            crate::generated_property_editors::window_definition(),
            theme,
            SIZE,
        );
        let from = separator(&form, "department");
        let to = Point::new(from.x + 88., from.y);
        let y = row(&form, "author").y + 3.;
        let mut ordinary_contrast = 0;
        for (stage, x, blank) in [
            ("before", from.x, to.x),
            ("dragging", to.x, from.x),
            ("after", to.x, from.x),
        ] {
            let bitmap = gpu
                .render(
                    form.ui.scene(),
                    [(SIZE.width * SCALE) as u32, (SIZE.height * SCALE) as u32],
                    SCALE,
                )
                .unwrap();
            let line = divider_contrast(&bitmap, x, y);
            let empty = divider_contrast(&bitmap, blank, y);
            assert!(
                line >= empty + 10,
                "{name} {stage}: line={line}, empty={empty}"
            );
            if stage == "before" {
                ordinary_contrast = line;
            } else if stage == "dragging" {
                assert!(
                    line > ordinary_contrast + 15,
                    "drag feedback is missing: {name} {line} / {ordinary_contrast}"
                );
            }
            if let Some(dir) = std::env::var_os("PROPERTY_GRID_FRAME_DIR") {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(
                    std::path::Path::new(&dir).join(format!("column-{name}-{stage}.png")),
                    encode_bitmap_png(&bitmap).unwrap(),
                )
                .unwrap();
            }
            if stage == "before" {
                pointer_down(&mut form, from);
                move_to(&mut form, to, SIZE);
            } else if stage == "dragging" {
                pointer_up(&mut form, to);
                move_to(&mut form, Point::new(1., 1.), SIZE);
            }
        }
    }
}
