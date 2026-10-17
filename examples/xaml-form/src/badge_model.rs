//! Application-owned badge counts and layout demonstration commands.
use crate::view::MountedView;
use rust_desktop_ui_controls::{BadgeClip, BindingContext, Controls, DataValue};
use rust_desktop_ui_core::{Length, Mutation, WidgetId};
use std::{cell::RefCell, rc::Rc};

pub(crate) fn model() -> Result<Rc<RefCell<BindingContext>>, String> {
    BindingContext::new(DataValue::object([
        ("Count", DataValue::from("5")),
        ("Visible", true.into()),
        ("OwnerVisible", true.into()),
        ("OwnerEnabled", true.into()),
    ]))
    .map(|model| Rc::new(RefCell::new(model)))
    .map_err(err)
}
pub(crate) fn register(ui: &mut Controls) {
    for (id, label) in [
        ("BadgeIncrement", "Добавить сообщение"),
        ("BadgeHuge", "Большой счётчик"),
        ("BadgeReset", "Сбросить"),
        ("BadgeOpen", "Входящие"),
        ("BadgeReflow", "Изменить ширину"),
        ("BadgeClip", "Обрезка по кнопке"),
    ] {
        ui.commands_mut().register(id, label);
    }
}
pub(crate) struct State {
    inbox: WidgetId,
    wrap: WidgetId,
    activity: WidgetId,
    clip_status: WidgetId,
    context: Rc<RefCell<BindingContext>>,
    opened: u64,
    wide: bool,
}
impl State {
    pub(crate) fn install(_ui: &mut Controls, view: &MountedView) -> Result<Option<Self>, String> {
        let Some(inbox) = view.find_name("BadgeInbox") else {
            return Ok(None);
        };
        let named = |name| {
            view.find_name(name)
                .ok_or_else(|| format!("Missing {name}"))
        };
        Ok(Some(Self {
            inbox,
            wrap: named("BadgeWrap")?,
            activity: named("BadgeActivity")?,
            clip_status: named("BadgeClipStatus")?,
            context: view
                .bindings
                .as_ref()
                .ok_or("Badge bindings missing")?
                .context(),
            opened: 0,
            wide: false,
        }))
    }
    pub(crate) fn command(
        &mut self,
        ui: &mut Controls,
        view: &mut MountedView,
        command: &str,
    ) -> Result<bool, String> {
        let message = match command {
            "BadgeIncrement" | "BadgeHuge" | "BadgeReset" => {
                let count = match command {
                    "BadgeHuge" => u64::MAX,
                    "BadgeReset" => 5,
                    _ => {
                        let context = self.context.borrow();
                        let DataValue::String(count) = context.get("Count").map_err(err)? else {
                            return Err("Count must be a string".into());
                        };
                        count.parse::<u64>().map_err(err)?.saturating_add(1)
                    }
                };
                self.context
                    .borrow_mut()
                    .set("Count", DataValue::from(count.to_string()))
                    .map_err(err)?;
                view.sync_bindings(ui);
                format!("Непрочитанных сообщений: {count}")
            }
            "BadgeOpen" => {
                self.opened += 1;
                format!("Входящие открыты: {}", self.opened)
            }
            "BadgeReflow" => {
                let mut style = ui
                    .runtime
                    .tree()
                    .node(self.wrap)
                    .ok_or("Badge wrap removed")?
                    .props
                    .style
                    .clone();
                let wide = !self.wide;
                style.width = Length::Percent(1.0);
                style.max_size.width = if wide { 600.0 } else { 420.0 };
                ui.runtime
                    .apply(Mutation::SetStyle {
                        node: self.wrap,
                        style,
                    })
                    .map_err(err)?;
                self.wide = wide;
                "Ширина области изменена".into()
            }
            "BadgeClip" => {
                let mut settings = ui.badge(self.inbox).cloned().ok_or("Badge removed")?;
                settings.clip = if settings.clip == BadgeClip::Owner {
                    BadgeClip::Parent
                } else {
                    BadgeClip::Owner
                };
                let message = if settings.clip == BadgeClip::Owner {
                    "Индикатор обрезается по краю кнопки."
                } else {
                    "Индикатор может выступать за край кнопки."
                };
                ui.set_badge(self.inbox, Some(settings)).map_err(err)?;
                ui.set_label(self.clip_status, message).map_err(err)?;
                message.into()
            }
            _ => return Ok(false),
        };
        ui.set_label(self.activity, message).map_err(err)?;
        Ok(true)
    }
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_controls::{BadgeContent, Theme};
    use rust_desktop_ui_core::Size;
    #[test]
    fn badge_demo_mounts_and_commands_keep_all_attached_owners() {
        let definition = rust_desktop_ui_xaml::parse(include_str!("../BadgeWindow.xaml")).unwrap();
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        register(&mut ui);
        ui.commands_mut().register("Light", "Светлая");
        ui.commands_mut().register("Dark", "Тёмная");
        let root = ui.root();
        let mut view =
            MountedView::mount_with_context(&mut ui, root, &definition.content, model().unwrap())
                .unwrap();
        let mut state = State::install(&mut ui, &view).unwrap().unwrap();
        ui.update(Size::new(1120.0, 820.0)).unwrap();
        for name in [
            "BadgeInbox",
            "BadgeDone",
            "BadgeWarning",
            "BadgeError",
            "BadgeDraft",
            "BadgeCard",
        ] {
            let owner = view.find_name(name).unwrap();
            assert!(
                ui.badge_bounds(owner).is_some(),
                "{name} is not visible in the initial demo viewport"
            );
        }
        let id = state.inbox;
        let nodes = ui.runtime.tree().len();
        state.command(&mut ui, &mut view, "BadgeHuge").unwrap();
        assert_eq!(ui.badge(id).unwrap().content, BadgeContent::Count(u64::MAX));
        for command in ["BadgeReflow", "BadgeClip", "BadgeOpen", "BadgeReset"] {
            state.command(&mut ui, &mut view, command).unwrap();
        }
        ui.update(Size::new(1120.0, 820.0)).unwrap();
        assert_eq!(ui.runtime.tree().len(), nodes);
        assert_eq!(view.find_name("BadgeInbox"), Some(id));
        assert_eq!(ui.badge(id).unwrap().clip, BadgeClip::Owner);
        assert!(!state.command(&mut ui, &mut view, "unrelated").unwrap());
    }
}
