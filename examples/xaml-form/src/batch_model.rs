//! Application data and localization for the component gallery.
use crate::view::MountedView;
use rust_desktop_ui_controls::*;
use rust_desktop_ui_core::WidgetId;
use std::{cell::RefCell, rc::Rc, time::Duration};

pub(crate) fn model() -> Result<Rc<RefCell<BindingContext>>, String> {
    let mut model = BindingContext::new(DataValue::object([
        ("Code", "DEMO-001".into()),
        ("Quantity", 12.0.into()),
        ("Price", 19.5.into()),
        ("Lower", 20.0.into()),
        ("Upper", 80.0.into()),
        ("Progress", 65.0.into()),
        ("Busy", false.into()),
        ("Check", DataValue::Null),
        ("SelectedKey", "one".into()),
        (
            "Choices",
            DataValue::List(vec![
                DataValue::object([
                    ("key", "one".into()),
                    ("label", "Первый".into()),
                    ("enabled", true.into()),
                ]),
                DataValue::object([
                    ("key", "two".into()),
                    ("label", "Второй".into()),
                    ("enabled", true.into()),
                ]),
                DataValue::object([
                    ("key", "three".into()),
                    ("label", "Третий".into()),
                    ("enabled", false.into()),
                ]),
            ]),
        ),
    ]))
    .map_err(err)?;
    model
        .set_validator(Some(
            BindingValidator::from_rules(vec![CrossFieldRule {
                paths: ["Quantity", "Price", "Lower", "Upper"]
                    .into_iter()
                    .map(BindingPath::parse)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(err)?,
                condition: BindingExpression::parse("$0 >= 0 && $1 >= 0 && $2 <= $3")
                    .map_err(err)?,
                message: "Количество и цена неотрицательны; начало диапазона не превышает конец."
                    .into(),
            }])
            .map_err(err)?,
        ))
        .map_err(err)?;
    Ok(Rc::new(RefCell::new(model)))
}

pub(crate) struct BatchState {
    context: Rc<RefCell<BindingContext>>,
    editor: WidgetId,
    status: Option<WidgetId>,
    details: Option<WidgetId>,
    localizer: Localizer,
    labels: LocalizationBindings,
    context_revision: Option<(u64, UiCulture)>,
    status_text: String,
    details_text: String,
}
pub(crate) fn install(ui: &mut Controls, view: &MountedView) -> Result<Option<BatchState>, String> {
    let Some(editor) = view.find_name("BatchCode") else {
        return Ok(None);
    };
    let context = view
        .bindings
        .as_ref()
        .ok_or("Component gallery requires bindings")?
        .context();
    let mut localizer = Localizer::default();
    for (culture, greeting, placeholder) in [
        (UiCulture::Russian, "Компоненты формы", "Введите код"),
        (UiCulture::English, "Form components", "Enter a code"),
        (UiCulture::Arabic, "مكونات النموذج", "أدخل الرمز"),
    ] {
        localizer
            .register(culture, "gallery", "greeting", greeting.into())
            .map_err(err)?;
        localizer
            .register(culture, "gallery", "code", placeholder.into())
            .map_err(err)?;
    }
    let mut labels = LocalizationBindings::default();
    if let Some(label) = view.find_name("BatchGreeting") {
        labels
            .add(label, LocalizedProperty::Label, "gallery", "greeting", None)
            .map_err(err)?;
    }
    labels
        .add(
            editor,
            LocalizedProperty::Placeholder,
            "gallery",
            "code",
            None,
        )
        .map_err(err)?;
    ui.set_async_editor_validator(
        editor,
        Some(AsyncEditorValidator::new(|request| {
            for _ in 0..10 {
                if request.cancellation.is_cancelled() {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            if request.draft.trim().is_empty() || request.draft.starts_with('!') {
                Err(match request.culture {
                    UiCulture::Russian => "Код не должен быть пустым или начинаться с !",
                    UiCulture::English => "The code must not be empty or start with !",
                    UiCulture::Arabic => "يجب ألا يكون الرمز فارغًا أو يبدأ بعلامة !",
                }
                .into())
            } else {
                Ok(())
            }
        })),
    )
    .map_err(err)?;
    let mut state = BatchState {
        context,
        editor,
        status: view.find_name("BatchValidation"),
        details: view.find_name("BatchCultureDetails"),
        localizer,
        labels,
        context_revision: None,
        status_text: String::new(),
        details_text: String::new(),
    };
    state.refresh(ui)?;
    Ok(Some(state))
}
impl BatchState {
    pub(crate) fn command(&mut self, ui: &mut Controls, id: &str) -> Result<bool, String> {
        let culture = match id {
            "BatchRussian" => UiCulture::Russian,
            "BatchEnglish" => UiCulture::English,
            "BatchArabic" => UiCulture::Arabic,
            _ => return Ok(false),
        };
        self.localizer.set_culture(culture);
        self.refresh(ui)?;
        Ok(true)
    }
    /// Called before the UI's update/poll; context/culture changes invalidate a
    /// pending request before its completion can be considered for acceptance.
    pub(crate) fn refresh(&mut self, ui: &mut Controls) -> Result<bool, String> {
        let culture = self.localizer.culture();
        let revision = self.context.borrow().revision();
        if self.context_revision != Some((revision, culture)) {
            ui.set_editor_validation_context(
                self.editor,
                self.context.borrow().root().clone(),
                revision,
                culture,
            )
            .map_err(err)?;
            self.context_revision = Some((revision, culture));
        }
        let mut changed = self.labels.sync(ui, &self.localizer).map_err(err)?;
        let details = format!(
            "{} · {} · {} · {:?}",
            culture.tag(),
            culture.format_number(12345.67, 2, true).map_err(err)?,
            culture
                .format_date(DateValue::new(2026, 10, 16).map_err(err)?)
                .map_err(err)?,
            culture.direction()
        );
        if details != self.details_text {
            if let Some(id) = self.details {
                ui.set_label(id, details.clone()).map_err(err)?;
            }
            self.details_text = details;
            changed = true;
        }
        let status = if ui.editor_validation_status(self.editor) == EditorValidationStatus::Pending
        {
            match culture {
                UiCulture::Russian => "Проверка…",
                UiCulture::English => "Validating…",
                UiCulture::Arabic => "جار التحقق…",
            }
            .into()
        } else if let Some(error) = ui.validation_error(self.editor) {
            error.to_owned()
        } else if ui.is_edit_dirty(self.editor) == Some(true) {
            match culture {
                UiCulture::Russian => "Черновик: Enter подтверждает, Escape отменяет",
                UiCulture::English => "Draft: Enter accepts, Escape cancels",
                UiCulture::Arabic => "مسودة: Enter يقبل و Escape يلغي",
            }
            .into()
        } else {
            format!(
                "{}: {}",
                match culture {
                    UiCulture::Russian => "Принято",
                    UiCulture::English => "Accepted",
                    UiCulture::Arabic => "تم القبول",
                },
                ui.accepted_value(self.editor).unwrap_or("")
            )
        };
        if status != self.status_text {
            if let Some(id) = self.status {
                ui.set_label(id, status.clone()).map_err(err)?;
            }
            self.status_text = status;
            changed = true;
        }
        Ok(changed)
    }
}
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
