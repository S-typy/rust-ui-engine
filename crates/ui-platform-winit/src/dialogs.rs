//! Owned native dialogs. A successful selection returns paths, never file contents.
use std::{fmt, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogButton {
    Ok,
    Cancel,
    Yes,
    No,
    Retry,
}

/// Standard button sets have platform-localized labels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DialogButtons {
    #[default]
    Ok,
    OkCancel,
    YesNo,
    YesNoCancel,
    RetryCancel,
}
impl DialogButtons {
    pub fn buttons(self) -> &'static [DialogButton] {
        use DialogButton::*;
        match self {
            Self::Ok => &[Ok],
            Self::OkCancel => &[Ok, Cancel],
            Self::YesNo => &[Yes, No],
            Self::YesNoCancel => &[Yes, No, Cancel],
            Self::RetryCancel => &[Retry, Cancel],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageDialog {
    pub title: String,
    pub message: String,
    pub buttons: DialogButtons,
    pub default_button: DialogButton,
}
impl MessageDialog {
    pub fn validate(&self) -> Result<(), DialogError> {
        validate_text(&self.title, 512, "Dialog title")?;
        validate_text(&self.message, 32768, "Dialog message")?;
        if !self.buttons.buttons().contains(&self.default_button) {
            return Err(DialogError::invalid("The default button is not present"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileDialogKind {
    Open,
    Save,
    Folder,
}

/// Extensions exclude the dot. A single `*` means every file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDialogFilter {
    pub label: String,
    pub extensions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDialog {
    pub kind: FileDialogKind,
    pub title: String,
    pub filters: Vec<FileDialogFilter>,
    /// Zero-based index of the initial filter, when filters are present.
    pub filter_index: usize,
    pub initial_directory: Option<PathBuf>,
    pub file_name: Option<String>,
    pub default_extension: Option<String>,
    pub multiple: bool,
}
impl FileDialog {
    pub fn new(kind: FileDialogKind) -> Self {
        Self {
            kind,
            title: String::new(),
            filters: Vec::new(),
            filter_index: 0,
            initial_directory: None,
            file_name: None,
            default_extension: None,
            multiple: false,
        }
    }
    pub fn validate(&self) -> Result<(), DialogError> {
        validate_text(&self.title, 512, "Dialog title")?;
        if self.multiple && self.kind != FileDialogKind::Open {
            return Err(DialogError::invalid("Multiple selection requires Open"));
        }
        if self.filters.len() > 32
            || (self.filters.is_empty() && self.filter_index != 0)
            || (!self.filters.is_empty() && self.filter_index >= self.filters.len())
        {
            return Err(DialogError::invalid("Invalid file filter count or index"));
        }
        if self.kind == FileDialogKind::Folder
            && (!self.filters.is_empty()
                || self.file_name.is_some()
                || self.default_extension.is_some())
        {
            return Err(DialogError::invalid(
                "Folder selection does not use file filters or names",
            ));
        }
        for filter in &self.filters {
            validate_text(&filter.label, 256, "Filter label")?;
            if filter.label.is_empty()
                || filter.extensions.is_empty()
                || filter.extensions.len() > 32
            {
                return Err(DialogError::invalid(
                    "A filter needs a label and 1–32 extensions",
                ));
            }
            for extension in &filter.extensions {
                if extension != "*" {
                    validate_extension(extension)?;
                }
            }
        }
        if let Some(extension) = &self.default_extension {
            validate_extension(extension)?;
        }
        if let Some(name) = &self.file_name {
            validate_text(name, 1024, "File name")?;
            if name.contains(['/', '\\']) {
                return Err(DialogError::invalid("File name cannot contain a directory"));
            }
        }
        if let Some(directory) = &self.initial_directory
            && (!directory.is_absolute()
                || directory.as_os_str().is_empty()
                || directory.as_os_str().as_encoded_bytes().contains(&0))
        {
            return Err(DialogError::invalid(
                "Initial directory must be absolute and contain no NUL",
            ));
        }
        Ok(())
    }
}
fn validate_extension(value: &str) -> Result<(), DialogError> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err(DialogError::invalid(
            "Extensions contain only letters, digits, '_' or '-'",
        ));
    }
    Ok(())
}
fn validate_text(value: &str, max: usize, label: &str) -> Result<(), DialogError> {
    if value.len() > max || value.contains('\0') {
        Err(DialogError::invalid(format!(
            "{label} contains NUL or exceeds its size limit"
        )))
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogOptions {
    Message(MessageDialog),
    File(FileDialog),
}
impl DialogOptions {
    pub fn validate(&self) -> Result<(), DialogError> {
        match self {
            Self::Message(value) => value.validate(),
            Self::File(value) => value.validate(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogRequest {
    /// Application correlation identity, returned unchanged on completion.
    pub id: u64,
    pub options: DialogOptions,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogOutcome {
    Cancelled,
    Button(DialogButton),
    Paths(Vec<PathBuf>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogErrorKind {
    Invalid,
    Unavailable,
    Native,
    OwnerClosed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogError {
    pub kind: DialogErrorKind,
    pub message: String,
}
impl DialogError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            kind: DialogErrorKind::Invalid,
            message: message.into(),
        }
    }
}
impl fmt::Display for DialogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for DialogError {}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogCompletion {
    pub id: u64,
    pub result: Result<DialogOutcome, DialogError>,
}

/// Default/cancel behavior for an application-rendered modal window. Results
/// are taken exactly once; owner shutdown always cancels an unfinished session.
#[derive(Clone, Debug)]
pub struct DialogSession<T> {
    default: Option<T>,
    cancel: T,
    result: Option<T>,
    finished: bool,
}
impl<T: Clone> DialogSession<T> {
    pub fn new(default: Option<T>, cancel: T) -> Self {
        Self {
            default,
            cancel,
            result: None,
            finished: false,
        }
    }
    pub fn complete(&mut self, value: T) -> bool {
        if self.finished {
            return false;
        }
        self.finished = true;
        self.result = Some(value);
        true
    }
    pub fn accept_default(&mut self) -> bool {
        self.default
            .clone()
            .is_some_and(|value| self.complete(value))
    }
    pub fn cancel(&mut self) -> bool {
        self.complete(self.cancel.clone())
    }
    pub fn owner_closed(&mut self) -> bool {
        self.cancel()
    }
    pub fn is_finished(&self) -> bool {
        self.finished
    }
    pub fn take_result(&mut self) -> Option<T> {
        self.result.take()
    }
}

pub(crate) fn show(owner: &winit::window::Window, request: DialogRequest) -> DialogCompletion {
    let result = request
        .options
        .validate()
        .and_then(|()| native::show(owner, &request.options));
    DialogCompletion {
        id: request.id,
        result,
    }
}

/// Reject malformed options before the host changes focus or commits a draft.
/// The error completion keeps the original correlation identity and requires
/// no native owner, so this check is shared by both host implementations.
pub(crate) fn validate_request(request: DialogRequest) -> Result<DialogRequest, DialogCompletion> {
    match request.options.validate() {
        Ok(()) => Ok(request),
        Err(error) => Err(DialogCompletion {
            id: request.id,
            result: Err(error),
        }),
    }
}

#[cfg(target_os = "windows")]
#[path = "dialogs_windows.rs"]
mod native;
#[cfg(not(target_os = "windows"))]
mod native {
    use super::*;
    pub(super) fn show(
        _: &winit::window::Window,
        _: &DialogOptions,
    ) -> Result<DialogOutcome, DialogError> {
        Err(DialogError {
            kind: DialogErrorKind::Unavailable,
            message: "Native dialog service is currently implemented on Windows".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_cancel_and_owner_close_resolve_only_once() {
        let mut session = DialogSession::new(Some("save"), "cancel");
        assert!(session.accept_default());
        assert!(!session.owner_closed());
        assert_eq!(session.take_result(), Some("save"));
        assert_eq!(session.take_result(), None);
        assert!(!session.cancel());
        let mut session = DialogSession::new(None::<String>, "cancel".into());
        assert!(!session.accept_default());
        assert!(session.owner_closed());
        assert_eq!(session.take_result().as_deref(), Some("cancel"));
    }
    #[test]
    fn invalid_filters_and_button_defaults_are_rejected_before_opening() {
        let mut file = FileDialog::new(FileDialogKind::Save);
        file.multiple = true;
        assert!(file.validate().is_err());
        file.multiple = false;
        file.filters.push(FileDialogFilter {
            label: "Text".into(),
            extensions: vec!["txt;*.exe".into()],
        });
        assert!(file.validate().is_err());
        file.filters[0].extensions = vec!["txt".into()];
        assert!(file.validate().is_ok());
        file.filter_index = 1;
        assert!(file.validate().is_err());
        let message = MessageDialog {
            title: "Prompt".into(),
            message: "Save?".into(),
            buttons: DialogButtons::YesNo,
            default_button: DialogButton::Cancel,
        };
        assert!(message.validate().is_err());
        let mut folder = FileDialog::new(FileDialogKind::Folder);
        folder.default_extension = Some("txt".into());
        assert!(folder.validate().is_err());
    }

    #[test]
    fn invalid_request_returns_its_identity_without_entering_the_blur_or_native_path() {
        let mut file = FileDialog::new(FileDialogKind::Open);
        file.initial_directory = Some(
            if cfg!(windows) {
                "C:\\invalid\0tail"
            } else {
                "/invalid\0tail"
            }
            .into(),
        );
        let request = DialogRequest {
            id: 19,
            options: DialogOptions::File(file),
        };
        let mut blurred = false;
        let completion = validate_request(request)
            .map(|_| {
                blurred = true;
            })
            .unwrap_err();
        assert!(!blurred);
        assert_eq!(completion.id, 19);
        assert_eq!(
            completion.result.unwrap_err().kind,
            DialogErrorKind::Invalid
        );
        let request = DialogRequest {
            id: 20,
            options: DialogOptions::Message(MessageDialog {
                title: "Confirm".into(),
                message: "Save?".into(),
                buttons: DialogButtons::YesNo,
                default_button: DialogButton::Cancel,
            }),
        };
        assert_eq!(validate_request(request).unwrap_err().id, 20);
    }
}
