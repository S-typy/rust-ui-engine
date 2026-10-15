use super::*;
use crate::{MenuEntry, MenuItem};
use rust_desktop_ui_core::DrawCommand;

fn fixture() -> (Controls, WidgetId) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.commands_mut().register("action", "Action");
    let button = ui
        .button(
            ui.root(),
            "Base anchor",
            None,
            LayoutStyle {
                width: Length::Px(160.0),
                height: Length::Px(36.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.update(Size::new(500.0, 400.0)).unwrap();
    (ui, button)
}

fn assert_complete(ui: &Controls) {
    let mut expected = ui.base_scene().clone();
    expected.append(ui.overlay_scene());
    assert_eq!(ui.scene(), &expected);
}

#[test]
fn popup_descendants_and_submenus_render_above_inserted_opaque_content() {
    let (mut ui, button) = fixture();
    ui.popup_menu_entries(
        button,
        &[
            MenuEntry::Submenu {
                label: "Overlay submenu".into(),
                items: vec![MenuEntry::Item(MenuItem::new("Overlay child", "action"))],
                enabled: true,
            },
            MenuEntry::Item(MenuItem::new("Overlay item", "action")),
        ],
    )
    .unwrap();
    ui.update(Size::new(500.0, 400.0)).unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::ArrowRight,
        modifiers: Default::default(),
        repeat: false,
    })
    .unwrap();
    ui.update(Size::new(500.0, 400.0)).unwrap();
    assert!(
        ui.base_scene()
            .texts
            .iter()
            .any(|text| text.text == "Base anchor")
    );
    assert!(
        ui.base_scene()
            .texts
            .iter()
            .all(|text| !text.text.starts_with("Overlay"))
    );
    for caption in ["Overlay submenu", "Overlay child", "Overlay item"] {
        assert!(
            ui.overlay_scene()
                .texts
                .iter()
                .any(|text| text.text == caption),
            "{caption}"
        );
    }
    assert!(!ui.overlay_scene().rounded_rectangles.is_empty());
    assert_complete(&ui);
    let mut composed = ui.base_scene().clone();
    composed.fill(Rect::new(0.0, 40.0, 500.0, 360.0), Color::rgb(241, 3, 156));
    let inserted = composed.commands().len() - 1;
    composed.append(ui.overlay_scene());
    for (position, command) in composed.commands().iter().enumerate() {
        if let DrawCommand::Text(index) = *command
            && composed.texts[index].text.starts_with("Overlay")
        {
            assert!(
                position > inserted,
                "Menu content must remain above opaque inserted panes"
            );
        }
    }
    ui.set_theme(Theme::FluentDark).unwrap();
    ui.update(Size::new(500.0, 400.0)).unwrap();
    assert_complete(&ui);
    ui.close_popup().unwrap();
    ui.update(Size::new(500.0, 400.0)).unwrap();
    assert!(ui.overlay_scene().commands().is_empty());
    assert_complete(&ui);
}

#[test]
fn key_tips_and_delayed_tooltips_are_overlays_without_moving_control_content() {
    let (mut ui, button) = fixture();
    assert_eq!(ui.base_scene(), ui.scene());
    assert!(ui.overlay_scene().commands().is_empty());
    ui.set_key_tip(button, Some("A1".into())).unwrap();
    ui.update(Size::new(500.0, 400.0)).unwrap();
    assert!(
        ui.overlay_scene()
            .texts
            .iter()
            .any(|text| text.text == "A1")
    );
    assert!(ui.base_scene().texts.iter().all(|text| text.text != "A1"));
    assert_complete(&ui);
    ui.set_key_tip(button, None).unwrap();
    ui.set_tooltip(button, "Overlay helpful tooltip").unwrap();
    let bounds = ui.runtime.bounds(button).unwrap();
    ui.dispatch(InputEvent::PointerMoved {
        position: Point::new(bounds.x + 10.0, bounds.y + 10.0),
    })
    .unwrap();
    ui.tooltip_tick(ui.tooltip_deadline().unwrap()).unwrap();
    assert!(
        ui.overlay_scene()
            .texts
            .iter()
            .any(|text| text.text == "Overlay helpful tooltip")
    );
    assert!(
        ui.base_scene()
            .texts
            .iter()
            .all(|text| !text.text.starts_with("Overlay"))
    );
    assert_complete(&ui);
    ui.dismiss_tooltip();
    ui.update(Size::new(500.0, 400.0)).unwrap();
    assert!(ui.overlay_scene().commands().is_empty());
    assert_complete(&ui);
}
