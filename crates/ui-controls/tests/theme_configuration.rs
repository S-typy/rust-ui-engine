//! Theme changes preserve live application state and explicit appearance.
use rust_desktop_ui_controls::{
    ButtonAppearance, ButtonStyle, Controls, EditBoxAppearance, EditBoxStyle, TextStyle, Theme,
    ThemeConfig, ThemeDensity, ThemeMode, ThemePalette,
};
use rust_desktop_ui_core::{
    Color, InputEvent, Key, LayoutStyle, Length, Modifiers, Mutation, Size,
};

#[test]
fn an_initial_auto_height_becomes_explicit_after_runtime_style_override() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let automatic = ui
        .button(root, "Automatic", None, LayoutStyle::default())
        .unwrap();
    let explicit = ui
        .button(root, "Explicit later", None, LayoutStyle::default())
        .unwrap();
    let removed = ui
        .button(root, "Removed", None, LayoutStyle::default())
        .unwrap();
    let mut style = ui
        .runtime
        .tree()
        .node(explicit)
        .unwrap()
        .props
        .style
        .clone();
    style.height = Length::Px(44.0);
    ui.runtime
        .apply(Mutation::SetStyle {
            node: explicit,
            style,
        })
        .unwrap();
    ui.runtime.apply(Mutation::Remove(removed)).unwrap();
    for density in [
        ThemeDensity::Touch,
        ThemeDensity::Compact,
        ThemeDensity::Comfortable,
    ] {
        let theme = Theme::Configured(ThemeConfig::default().with_density(density));
        ui.set_theme(theme).unwrap();
        ui.update(Size::new(640.0, 600.0)).unwrap();
        assert_eq!(ui.runtime.bounds(explicit).unwrap().height, 44.0);
        assert_eq!(
            ui.runtime.bounds(automatic).unwrap().height,
            theme.tokens().control_height
        );
        assert!(ui.button_style(removed).is_none());
    }
}

#[test]
fn theme_failure_on_removed_form_root_does_not_change_the_selected_palette() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.runtime.apply(Mutation::Remove(ui.root())).unwrap();
    assert!(ui.set_theme(Theme::HighContrastDark).is_err());
    assert_eq!(ui.theme(), Theme::FluentLight);
}

#[test]
fn live_palette_and_density_changes_preserve_document_selection_undo_and_overrides() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let auto = LayoutStyle {
        width: Length::Px(320.0),
        flex_shrink: 0.0,
        ..Default::default()
    };
    let editor = ui.edit_box(root, "Name", "initial", auto.clone()).unwrap();
    let check = ui.checkbox(root, "Enabled", true, auto.clone()).unwrap();
    let button = ui
        .button(
            root,
            "Fixed",
            None,
            LayoutStyle {
                height: Length::Px(63.0),
                ..auto.clone()
            },
        )
        .unwrap();
    let label = ui.label(root, "Explicit label", auto).unwrap();
    let explicit = Color::rgb(91, 21, 151);
    let button_style = ButtonStyle {
        font_size: Some(19.0),
        normal: ButtonAppearance {
            foreground: Some(explicit),
            ..Default::default()
        },
        ..Default::default()
    };
    let editor_style = EditBoxStyle {
        font_size: Some(18.0),
        border_width: Some(3.0),
        normal: EditBoxAppearance {
            foreground: Some(explicit),
            ..Default::default()
        },
        ..Default::default()
    };
    let label_style = TextStyle {
        foreground: Some(explicit),
        ..Default::default()
    };
    ui.set_button_style(button, button_style).unwrap();
    ui.set_edit_box_style(editor, editor_style).unwrap();
    ui.set_text_style(label, label_style).unwrap();
    ui.update(Size::new(640.0, 600.0)).unwrap();
    ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
    ui.text_input(" draft").unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::ArrowLeft,
        modifiers: Modifiers {
            shift: true,
            ..Default::default()
        },
        repeat: false,
    })
    .unwrap();
    let selection = ui.document(editor).unwrap().selection();
    let nodes = ui.runtime.tree().preorder();
    let custom = ThemeConfig::new(ThemeMode::Dark)
        .with_palette(ThemePalette {
            canvas: [24, 24, 32],
            surface: [32, 32, 44],
            text: [245, 245, 255],
            muted: [180, 180, 200],
            border: [140, 140, 180],
            accent: [180, 160, 255],
            error: [255, 160, 160],
        })
        .unwrap();
    for theme in [
        Theme::HighContrastLight,
        Theme::HighContrastDark,
        Theme::Configured(
            ThemeConfig::default()
                .with_accent([112, 52, 180])
                .unwrap()
                .with_density(ThemeDensity::Compact),
        ),
        Theme::Configured(custom.with_density(ThemeDensity::Touch)),
        Theme::FluentLight,
    ] {
        ui.set_theme(theme).unwrap();
        ui.update(Size::new(640.0, 600.0)).unwrap();
        assert_eq!(ui.runtime.tree().preorder(), nodes);
        assert_eq!(ui.runtime.focused(), Some(editor));
        assert_eq!(ui.document(editor).unwrap().text(), "initial draft");
        assert_eq!(ui.document(editor).unwrap().selection(), selection);
        assert!(ui.document(editor).unwrap().can_undo());
        assert_eq!(ui.accepted_value(editor), Some("initial"));
        assert_eq!(ui.checked(check), Some(true));
        assert_eq!(ui.edit_box_style(editor), Some(&editor_style));
        assert_eq!(ui.button_style(button), Some(&button_style));
        assert_eq!(ui.text_style(label), Some(label_style));
        assert_eq!(ui.runtime.bounds(button).unwrap().height, 63.0);
        assert_eq!(
            ui.runtime.bounds(check).unwrap().height,
            theme.tokens().control_height
        );
        for caption in ["Fixed", "Explicit label", "initial draft"] {
            assert_eq!(
                ui.scene()
                    .texts
                    .iter()
                    .find(|run| run.text == caption)
                    .unwrap()
                    .color,
                explicit
            );
        }
    }
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Character("z".into()),
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
        repeat: false,
    })
    .unwrap();
    assert_eq!(ui.document(editor).unwrap().text(), "initial");
}
