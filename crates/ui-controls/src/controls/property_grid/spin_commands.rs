use super::*;

pub(super) fn button(
    ui: &mut Controls,
    host: WidgetId,
    label: &str,
) -> Result<WidgetId, ControlError> {
    let id = ui.button(
        host,
        "",
        None,
        LayoutStyle {
            flex_shrink: 0.,
            height: Length::Px(28.),
            ..button_style(22.)
        },
    )?;
    ui.set_button_chrome(id, ButtonChrome::Flat)?;
    let name = format!("Действия свойства «{label}»");
    ui.set_automation_name(id, &name)?;
    ui.set_tooltip(id, name)?;
    ui.controls.get_mut(&id).unwrap().inspector_part = Some(InspectorPart::Commands);
    Ok(id)
}

// The inspector shares this flyout between rows and builds it on first use.
pub(super) struct Commands {
    flyout: Flyout,
    primary: WidgetId,
    cancel: WidgetId,
    reset: WidgetId,
    row: usize,
    editing: bool,
}

impl Commands {
    fn new(ui: &mut Controls, anchor: WidgetId) -> Result<Self, ControlError> {
        let flyout = ui.flyout(anchor, Size::new(202., 114.))?;
        ui.edit_props(flyout.content, |p| {
            p.style.kind = LayoutKind::Stack(Axis::Vertical);
            p.style.gap = 0.;
        })?;
        let mut item = |label| {
            let id = ui.button(
                flyout.content,
                label,
                None,
                LayoutStyle {
                    width: Length::Percent(1.),
                    height: Length::Px(32.),
                    ..Default::default()
                },
            )?;
            ui.set_button_chrome(id, ButtonChrome::Flat)?;
            Ok::<_, ControlError>(id)
        };
        Ok(Self {
            flyout,
            primary: item("Изменить")?,
            cancel: item("Отменить правку")?,
            reset: item("Значение по умолчанию")?,
            row: 0,
            editing: false,
        })
    }
}

impl PropertyGrid {
    fn open_spin_commands(
        &mut self,
        ui: &mut Controls,
        index: usize,
        anchor: WidgetId,
    ) -> Result<(), ControlError> {
        self.select(ui, Some(index))?;
        if self.spin_commands.is_none() {
            self.spin_commands = Some(Commands::new(ui, anchor)?);
        }
        let menu = self.spin_commands.as_mut().unwrap();
        let p = &self.properties[index];
        menu.row = index;
        menu.editing = p.active;
        ui.set_label(
            menu.primary,
            if p.active {
                "Применить"
            } else {
                "Изменить"
            },
        )?;
        show(ui, menu.cancel, p.active)?;
        let can_reset = p.descriptor.default.is_some();
        show(ui, menu.reset, can_reset)?;
        ui.set_flyout_anchor(menu.flyout, anchor)?;
        let rows = 1 + u8::from(p.active) + u8::from(can_reset);
        ui.set_flyout_placement(
            menu.flyout,
            Size::new(202., 16. + f32::from(rows) * 32.),
            FlyoutPlacement::Below,
        )?;
        ui.open_flyout(menu.flyout)
    }

    pub(super) fn update_spin_commands(&mut self, ui: &mut Controls) -> Result<bool, ControlError> {
        let mut changed = false;
        if let Some(menu) = &self.spin_commands {
            let mut action = None;
            ui.activated.retain(|id| {
                if [menu.primary, menu.cancel, menu.reset].contains(id) {
                    action = Some(*id);
                    return false;
                }
                true
            });
            if let Some(action) = action {
                let name = self.properties[menu.row].schema.name.clone();
                ui.close_flyout(menu.flyout)?;
                if action == menu.cancel {
                    self.cancel(ui, &name)?;
                } else if action == menu.reset {
                    self.reset(ui, &name)?;
                } else if menu.editing {
                    self.commit(ui, &name)?;
                } else {
                    self.begin_edit(ui, &name)?;
                }
                changed = true;
            }
        }
        let mut open = None;
        ui.activated.retain(|id| {
            if let Some(index) = self
                .properties
                .iter()
                .position(|p| p.spin_menu_button == Some(*id))
            {
                open = Some((index, *id));
                return false;
            }
            true
        });
        if let Some((index, anchor)) = open {
            self.open_spin_commands(ui, index, anchor)?;
            changed = true;
        }
        Ok(changed)
    }
}
