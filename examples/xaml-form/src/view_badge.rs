//! Attached metadata uses the owner identity and never creates a child control.
use super::*;
use rust_desktop_ui_controls::{
    BadgeAnchor, BadgeClip, BadgeContent, BadgeSettings, BadgeSeverity,
};
use rust_desktop_ui_core::Point;
use rust_desktop_ui_xaml as x;

pub(super) fn apply(
    ui: &mut Controls,
    owner: WidgetId,
    common: &x::CommonProperties,
) -> Result<(), String> {
    let Some(badge) = &common.badge else {
        return Ok(());
    };
    let content = if let Some(count) = badge.count {
        BadgeContent::Count(count)
    } else if let Some(text) = &badge.text {
        BadgeContent::Text(text.clone())
    } else if let Some(icon) = badge.icon {
        BadgeContent::Icon(icon_symbol(icon))
    } else if common.bindings.contains_key("Badge.Text") {
        BadgeContent::Text("…".into())
    } else {
        BadgeContent::Count(0)
    };
    ui.set_badge(
        owner,
        Some(BadgeSettings {
            content,
            severity: match badge.severity {
                x::BadgeSeverityDefinition::Neutral => BadgeSeverity::Neutral,
                x::BadgeSeverityDefinition::Info => BadgeSeverity::Info,
                x::BadgeSeverityDefinition::Success => BadgeSeverity::Success,
                x::BadgeSeverityDefinition::Warning => BadgeSeverity::Warning,
                x::BadgeSeverityDefinition::Error => BadgeSeverity::Error,
            },
            anchor: match badge.anchor {
                x::BadgeAnchorDefinition::TopLeft => BadgeAnchor::TopLeft,
                x::BadgeAnchorDefinition::TopRight => BadgeAnchor::TopRight,
                x::BadgeAnchorDefinition::BottomLeft => BadgeAnchor::BottomLeft,
                x::BadgeAnchorDefinition::BottomRight => BadgeAnchor::BottomRight,
                x::BadgeAnchorDefinition::Center => BadgeAnchor::Center,
            },
            offset: Point::new(badge.offset[0], badge.offset[1]),
            visible: badge.visible,
            clip: match badge.clip {
                x::BadgeClipDefinition::Owner => BadgeClip::Owner,
                x::BadgeClipDefinition::Parent => BadgeClip::Parent,
            },
            max_count: badge.max_count,
            label: badge.label.clone(),
        }),
    )
    .map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_controls::{BindingContext, DataValue, Theme};
    use rust_desktop_ui_core::{Mutation, Size};
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn compiled_badge_binding_is_exact_and_preserves_owner_draft_and_identity() {
        let definition=x::parse("<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'><StackPanel Padding='30'><TextBox x:Name='Editor' Text='initial' Width='240' Badge.Count='{Binding Count}' Badge.Visibility='{Binding Visible}' Badge.Label='Сообщения' ToolTip='Подсказка'/><Button x:Name='Status' Content='Статус' Badge.Text='{Binding Caption}'/></StackPanel></Window>").unwrap().content;
        let context = Rc::new(RefCell::new(
            BindingContext::new(DataValue::object([
                ("Count", DataValue::String("18446744073709551615".into())),
                ("Visible", DataValue::Bool(true)),
                ("Caption", DataValue::from("Готово")),
            ]))
            .unwrap(),
        ));
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let root = ui.root();
        let mut view =
            MountedView::mount_with_context(&mut ui, root, &definition, context.clone()).unwrap();
        ui.update(Size::new(500.0, 400.0)).unwrap();
        let editor = view.find_name("Editor").unwrap();
        let status = view.find_name("Status").unwrap();
        assert_eq!(
            ui.badge(editor).unwrap().content,
            BadgeContent::Count(u64::MAX)
        );
        assert_eq!(
            ui.badge(status).unwrap().content,
            BadgeContent::Text("Готово".into())
        );
        ui.runtime.apply(Mutation::Focus(Some(editor))).unwrap();
        ui.text_input("draft").unwrap();
        let draft = ui.document(editor).unwrap().text().to_owned();
        let nodes = ui.runtime.tree().len();
        context
            .borrow_mut()
            .set("Count", DataValue::from("9007199254740993"))
            .unwrap();
        view.sync_bindings(&mut ui);
        assert_eq!(
            ui.badge(editor).unwrap().content,
            BadgeContent::Count(9_007_199_254_740_993)
        );
        assert_eq!(ui.document(editor).unwrap().text(), draft);
        assert_eq!(ui.runtime.focused(), Some(editor));
        assert_eq!(ui.runtime.tree().len(), nodes);
        context
            .borrow_mut()
            .set("Count", DataValue::Number(3.0))
            .unwrap();
        view.sync_bindings(&mut ui);
        assert!(!view.bindings.as_ref().unwrap().diagnostics().is_empty());
        assert_eq!(
            ui.badge(editor).unwrap().content,
            BadgeContent::Count(9_007_199_254_740_993)
        );
        context
            .borrow_mut()
            .set("Count", DataValue::from("10"))
            .unwrap();
        context
            .borrow_mut()
            .set("Visible", DataValue::Bool(false))
            .unwrap();
        view.sync_bindings(&mut ui);
        assert!(ui.badge_bounds(editor).is_none());
        assert_eq!(ui.document(editor).unwrap().text(), draft);
    }
    #[test]
    fn manual_invalid_metadata_does_not_mount_partial_content() {
        let mut definition =
            x::parse("<Window xmlns='urn:rust-ui-engine:ui'><Button Badge.Count='1'/></Window>")
                .unwrap()
                .content;
        definition.common.badge.as_mut().unwrap().text = Some("conflict".into());
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let root = ui.root();
        let before = ui.runtime.tree().len();
        assert!(MountedView::mount(&mut ui, root, &definition).is_err());
        assert_eq!(ui.runtime.tree().len(), before);
    }
    #[test]
    fn badge_demo_parent_clip_contains_whole_indicator_after_count_and_reflow() {
        let definition = x::parse(include_str!("../BadgeWindow.xaml")).unwrap();
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        crate::badge_model::register(&mut ui);
        ui.commands_mut().register("Light", "Светлая");
        ui.commands_mut().register("Dark", "Тёмная");
        let root = ui.root();
        ui.runtime
            .apply(Mutation::SetStyle {
                node: root,
                style: LayoutStyle {
                    kind: LayoutKind::Stack(Axis::Vertical),
                    width: Length::Percent(1.0),
                    height: Length::Percent(1.0),
                    padding: edges(definition.padding),
                    ..Default::default()
                },
            })
            .unwrap();
        let mut view = MountedView::mount_with_context(
            &mut ui,
            root,
            &definition.content,
            crate::badge_model::model().unwrap(),
        )
        .unwrap();
        let mut state = crate::badge_model::State::install(&mut ui, &view)
            .unwrap()
            .unwrap();
        let check = |ui: &mut Controls, view: &mut MountedView, width: f32| {
            ui.update(Size::new(width, 820.0)).unwrap();
            view.update_form_layout(ui, Size::new(width, 820.0))
                .unwrap();
            for name in [
                "BadgeInbox",
                "BadgeDone",
                "BadgeWarning",
                "BadgeError",
                "BadgeDraft",
                "BadgeCard",
            ] {
                let owner = view.find_name(name).unwrap();
                let bounds = ui.badge_bounds(owner).unwrap_or_else(|| {
                    panic!(
                        "{name} invisible at {width}, owner {:?}, clip {:?}, wrap {:?}",
                        ui.runtime.bounds(owner),
                        ui.runtime.clip(owner),
                        ui.runtime.bounds(view.find_name("BadgeWrap").unwrap())
                    )
                });
                let clip = ui.runtime.clip(owner).unwrap();
                assert!(
                    bounds.x >= clip.x - 0.01
                        && bounds.y >= clip.y - 0.01
                        && bounds.x + bounds.width <= clip.x + clip.width + 0.01
                        && bounds.y + bounds.height <= clip.y + clip.height + 0.01,
                    "{name} badge {bounds:?} exceeds ancestor clip {clip:?} at width {width}"
                );
            }
        };
        for width in [1120.0, 1000.0] {
            state.command(&mut ui, &mut view, "BadgeReset").unwrap();
            check(&mut ui, &mut view, width);
            state.command(&mut ui, &mut view, "BadgeHuge").unwrap();
            check(&mut ui, &mut view, width);
            let warning = view.find_name("BadgeWarning").unwrap();
            let before_reflow = ui.runtime.bounds(warning).unwrap();
            state.command(&mut ui, &mut view, "BadgeReflow").unwrap();
            check(&mut ui, &mut view, width);
            if width == 1120.0 {
                assert_ne!(ui.runtime.bounds(warning).unwrap(), before_reflow);
            }
            for resized in [1000.0, 1120.0, 900.0] {
                check(&mut ui, &mut view, resized);
            }
            state.command(&mut ui, &mut view, "BadgeReflow").unwrap();
            check(&mut ui, &mut view, width);
        }
    }
}
