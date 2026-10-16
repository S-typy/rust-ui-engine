//! Application-owned font descriptor and document preview.
use crate::view::MountedView;
use rust_desktop_ui_controls::{
    BindingContext, Controls, DataValue, FontDescriptor, FontSlant, InlineSpan,
    font_descriptor_from_value, font_descriptor_to_value,
};
use rust_desktop_ui_core::WidgetId;
use std::{cell::RefCell, rc::Rc};

const DOCUMENT: &str = "Заметки о типографике\n\nЧёткая иерархия помогает читать документ. Выберите семейство, начертание и размер, чтобы увидеть результат.\n\nThe quick brown fox jumps over the lazy dog. 0123456789";
const MISSING_FAMILY: &str = "Example Missing Family 7254";

fn initial_font() -> FontDescriptor {
    FontDescriptor {
        size: 24.0,
        ..Default::default()
    }
}

pub(crate) fn model() -> Result<Rc<RefCell<BindingContext>>, String> {
    BindingContext::new(DataValue::object([
        ("SelectedFont", font_descriptor_to_value(&initial_font())),
        ("Locked", false.into()),
    ]))
    .map(|model| Rc::new(RefCell::new(model)))
    .map_err(err)
}

pub(crate) fn register(ui: &mut Controls) {
    for (id, label) in [
        ("FontDemoInstalled", "Системный курсив"),
        ("FontDemoMissing", "Отсутствующий шрифт"),
        ("FontDemoReset", "Сбросить"),
    ] {
        ui.commands_mut().register(id, label);
    }
}

pub(crate) struct State {
    owner: WidgetId,
    requested: WidgetId,
    resolved: WidgetId,
    document: WidgetId,
    binding_value: WidgetId,
    activity: WidgetId,
    context: Rc<RefCell<BindingContext>>,
    shown: Option<FontDescriptor>,
    model_shown: Option<DataValue>,
    accepted_changes: u64,
}

impl State {
    pub(crate) fn install(
        ui: &mut Controls,
        view: &mut MountedView,
    ) -> Result<Option<Self>, String> {
        let Some(owner) = view.find_name("DocumentFont") else {
            return Ok(None);
        };
        let parts = ui
            .font_parts(owner)
            .ok_or("FontPicker parts missing")?
            .clone();
        for (name, id) in [
            ("FontFamily", parts.family.container),
            ("FontFamilyEditor", parts.family.editor),
            ("FontFamilyButton", parts.family.buttons[0]),
            ("FontFace", parts.face.container),
            ("FontFaceEditor", parts.face.editor),
            ("FontFaceButton", parts.face.buttons[0]),
            ("FontSize", parts.size.container),
            ("FontSizeEditor", parts.size.editor),
            ("FontPickerPreview", parts.preview),
            ("FontPickerStatus", parts.status),
        ] {
            view.names.insert(name.into(), id);
        }
        for (id, label) in [
            (parts.family.editor, "Семейство шрифта"),
            (parts.face.editor, "Начертание шрифта"),
            (parts.size.editor, "Размер шрифта"),
        ] {
            ui.set_automation_name(id, label).map_err(err)?;
        }
        let required = |name| {
            view.find_name(name)
                .ok_or_else(|| format!("Missing {name}"))
        };
        let mut state = Self {
            owner,
            requested: required("FontRequested")?,
            resolved: required("FontResolved")?,
            document: required("FontDocumentPreview")?,
            binding_value: required("FontBindingValue")?,
            activity: required("FontActivity")?,
            context: view
                .bindings
                .as_ref()
                .ok_or("Font binding missing")?
                .context(),
            shown: None,
            model_shown: None,
            accepted_changes: 0,
        };
        state.refresh(ui)?;
        Ok(Some(state))
    }

    pub(crate) fn command(
        &mut self,
        ui: &mut Controls,
        view: &mut MountedView,
        command: &str,
    ) -> Result<bool, String> {
        let (font, message) = match command {
            "FontDemoReset" => (initial_font(), "Начальные параметры загружены из модели"),
            "FontDemoMissing" => (
                FontDescriptor {
                    family: MISSING_FAMILY.into(),
                    size: 26.0,
                    ..Default::default()
                },
                "Запрошенное семейство сохранено; текст использует подстановку",
            ),
            "FontDemoInstalled" => {
                let families = ui.font_families().map_err(err)?.to_vec();
                let family = families
                    .iter()
                    .find(|name| name.eq_ignore_ascii_case("Arial"))
                    .or_else(|| families.first())
                    .ok_or("System font catalog is empty")?;
                let faces = ui.font_faces(family).map_err(err)?;
                let face = faces
                    .iter()
                    .find(|face| face.slant == FontSlant::Italic && face.weight == 400)
                    .or_else(|| faces.first())
                    .ok_or("No supported face in the selected family")?;
                (
                    face.descriptor(family, 28.0),
                    "Системное начертание загружено через binding",
                )
            }
            _ => return Ok(false),
        };
        self.context
            .borrow_mut()
            .set("SelectedFont", font_descriptor_to_value(&font))
            .map_err(err)?;
        view.bindings
            .as_mut()
            .ok_or("Font binding missing")?
            .sync(ui);
        self.refresh(ui)?;
        ui.set_label(self.activity, message).map_err(err)?;
        Ok(true)
    }

    pub(crate) fn refresh(&mut self, ui: &mut Controls) -> Result<(), String> {
        let font = ui
            .font_value(self.owner)
            .ok_or("FontPicker removed")?
            .clone();
        let changes = ui
            .take_font_changes()
            .into_iter()
            .filter(|change| change.id == self.owner)
            .count();
        if changes != 0 {
            self.accepted_changes += changes as u64;
            ui.set_label(
                self.activity,
                format!("Принято изменений: {}", self.accepted_changes),
            )
            .map_err(err)?;
        }
        if self.shown.as_ref() != Some(&font) {
            let resolution = ui
                .font_resolution(self.owner)
                .ok_or("Font resolution missing")?
                .clone();
            ui.set_label(self.requested, describe(&font)).map_err(err)?;
            ui.set_label(
                self.resolved,
                format!(
                    "{}\n{}",
                    if resolution.family_available {
                        "Семейство установлено"
                    } else {
                        "Семейство отсутствует — используется подстановка"
                    },
                    resolution.actual_fonts.join(", ")
                ),
            )
            .map_err(err)?;
            ui.set_text_inlines(
                self.document,
                vec![InlineSpan {
                    text: DOCUMENT.into(),
                    font: Some(font.clone()),
                    ..Default::default()
                }],
            )
            .map_err(err)?;
            self.shown = Some(font);
        }
        let value = self
            .context
            .borrow()
            .get("SelectedFont")
            .map_err(err)?
            .clone();
        if self.model_shown.as_ref() != Some(&value) {
            let model_font = font_descriptor_from_value(&value).map_err(err)?;
            ui.set_label(
                self.binding_value,
                format!("Значение модели:\n{}", describe(&model_font)),
            )
            .map_err(err)?;
            self.model_shown = Some(value);
        }
        Ok(())
    }
}

fn describe(font: &FontDescriptor) -> String {
    let slant = match font.slant {
        FontSlant::Normal => "Обычное".into(),
        FontSlant::Italic => "Курсив".into(),
        FontSlant::Oblique(angle) => format!("Наклон {angle}°"),
    };
    format!(
        "{} · {} px\n{slant} · вес {} · ширина {}%",
        font.family, font.size, font.weight, font.width
    )
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_controls::Theme;
    use rust_desktop_ui_core::Size;

    #[test]
    fn font_demo_model_commands_and_readonly_keep_the_shaped_document() {
        let definition = rust_desktop_ui_xaml::parse(include_str!("../FontWindow.xaml")).unwrap();
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        register(&mut ui);
        for command in ["Light", "Dark"] {
            ui.commands_mut().register(command, command);
        }
        let context = model().unwrap();
        let root = ui.root();
        let mut view =
            MountedView::mount_with_context(&mut ui, root, &definition.content, context.clone())
                .unwrap();
        let mut state = State::install(&mut ui, &mut view).unwrap().unwrap();
        ui.update(Size::new(1200.0, 820.0)).unwrap();
        let parts = ui.font_parts(state.owner).unwrap().clone();
        assert!(
            state
                .command(&mut ui, &mut view, "FontDemoMissing")
                .unwrap()
        );
        assert_eq!(ui.font_value(state.owner).unwrap().family, MISSING_FAMILY);
        assert!(!ui.font_resolution(state.owner).unwrap().family_available);
        assert_eq!(ui.label_text(state.document), Some(DOCUMENT));
        assert!(
            ui.label_text(state.binding_value)
                .unwrap()
                .contains(MISSING_FAMILY)
        );
        context.borrow_mut().set("Locked", true.into()).unwrap();
        view.bindings.as_mut().unwrap().sync(&mut ui);
        assert!(ui.font_picker_settings(state.owner).unwrap().read_only);
        assert!(
            state
                .command(&mut ui, &mut view, "FontDemoInstalled")
                .unwrap()
        );
        assert!(ui.font_resolution(state.owner).unwrap().family_available);
        assert_eq!(ui.font_parts(state.owner), Some(&parts));
        assert!(state.command(&mut ui, &mut view, "FontDemoReset").unwrap());
        assert_eq!(ui.font_value(state.owner), Some(&initial_font()));
        assert!(!state.command(&mut ui, &mut view, "Other").unwrap());
    }
}
