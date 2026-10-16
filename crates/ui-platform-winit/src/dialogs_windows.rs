use super::*;
use std::{
    ffi::OsString,
    os::windows::ffi::{OsStrExt, OsStringExt},
};
use windows::{
    Win32::{
        Foundation::HWND,
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoTaskMemFree, CoUninitialize,
        },
        UI::{
            Shell::{
                self, Common::COMDLG_FILTERSPEC, IFileDialog, IFileOpenDialog, IFileSaveDialog,
                IShellItem,
            },
            WindowsAndMessaging::*,
        },
    },
    core::{HRESULT, Interface, PCWSTR},
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

fn native_error(error: impl fmt::Display) -> DialogError {
    DialogError {
        kind: DialogErrorKind::Native,
        message: error.to_string(),
    }
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

pub(super) fn show(
    window: &winit::window::Window,
    options: &DialogOptions,
) -> Result<DialogOutcome, DialogError> {
    let RawWindowHandle::Win32(handle) = window.window_handle().map_err(native_error)?.as_raw()
    else {
        return Err(native_error("Expected a Windows owner"));
    };
    let owner = HWND(handle.hwnd.get() as *mut _);
    // The host retains the Window throughout this call. Native modal APIs own
    // their message loop and restore the owner's enabled state on completion.
    if !unsafe { IsWindow(Some(owner)) }.as_bool() {
        return Err(DialogError {
            kind: DialogErrorKind::OwnerClosed,
            message: "Dialog owner is closed".into(),
        });
    }
    let result = match options {
        DialogOptions::Message(message) => message_dialog(owner, message),
        DialogOptions::File(file) => file_dialog(owner, file),
    };
    if !unsafe { IsWindow(Some(owner)) }.as_bool() {
        return Err(DialogError {
            kind: DialogErrorKind::OwnerClosed,
            message: "Dialog owner closed before completion".into(),
        });
    }
    result
}

fn message_dialog(owner: HWND, message: &MessageDialog) -> Result<DialogOutcome, DialogError> {
    let title = wide(&message.title);
    let text = wide(&message.message);
    let buttons = match message.buttons {
        DialogButtons::Ok => MB_OK,
        DialogButtons::OkCancel => MB_OKCANCEL,
        DialogButtons::YesNo => MB_YESNO,
        DialogButtons::YesNoCancel => MB_YESNOCANCEL,
        DialogButtons::RetryCancel => MB_RETRYCANCEL,
    };
    let default = match message
        .buttons
        .buttons()
        .iter()
        .position(|button| *button == message.default_button)
    {
        Some(0) => MB_DEFBUTTON1,
        Some(1) => MB_DEFBUTTON2,
        Some(2) => MB_DEFBUTTON3,
        _ => return Err(DialogError::invalid("Invalid default button")),
    };
    // Text buffers remain live; the API uses system-localized button labels.
    let result = unsafe {
        MessageBoxW(
            Some(owner),
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            buttons | default,
        )
    };
    Ok(match result {
        IDCANCEL => DialogOutcome::Cancelled,
        IDOK => DialogOutcome::Button(DialogButton::Ok),
        IDYES => DialogOutcome::Button(DialogButton::Yes),
        IDNO => DialogOutcome::Button(DialogButton::No),
        IDRETRY => DialogOutcome::Button(DialogButton::Retry),
        _ => return Err(native_error(windows::core::Error::from_thread())),
    })
}

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

fn file_dialog(owner: HWND, file: &FileDialog) -> Result<DialogOutcome, DialogError> {
    // S_OK and S_FALSE both acquire a matching initialization count. An
    // incompatible apartment fails explicitly; it is never uninitialized here.
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
        .ok()
        .map_err(native_error)?;
    let _apartment = Apartment;
    let (dialog, open): (IFileDialog, Option<IFileOpenDialog>) = unsafe {
        if file.kind == FileDialogKind::Save {
            let save: IFileSaveDialog =
                CoCreateInstance(&Shell::FileSaveDialog, None, CLSCTX_INPROC_SERVER)
                    .map_err(native_error)?;
            (save.cast().map_err(native_error)?, None)
        } else {
            let open: IFileOpenDialog =
                CoCreateInstance(&Shell::FileOpenDialog, None, CLSCTX_INPROC_SERVER)
                    .map_err(native_error)?;
            (open.cast().map_err(native_error)?, Some(open))
        }
    };
    let title = wide(&file.title);
    let labels: Vec<_> = file
        .filters
        .iter()
        .map(|filter| wide(&filter.label))
        .collect();
    let patterns: Vec<_> = file
        .filters
        .iter()
        .map(|filter| {
            wide(
                &filter
                    .extensions
                    .iter()
                    .map(|extension| {
                        if extension == "*" {
                            "*.*".into()
                        } else {
                            format!("*.{extension}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(";"),
            )
        })
        .collect();
    let filters: Vec<_> = labels
        .iter()
        .zip(&patterns)
        .map(|(label, pattern)| COMDLG_FILTERSPEC {
            pszName: PCWSTR(label.as_ptr()),
            pszSpec: PCWSTR(pattern.as_ptr()),
        })
        .collect();
    // COM retains/copies configuration strings. These buffers also remain
    // alive through Show. Results are extracted before releasing the apartment.
    unsafe {
        let mut options = dialog.GetOptions().map_err(native_error)?
            | Shell::FOS_FORCEFILESYSTEM
            | Shell::FOS_NOCHANGEDIR
            | Shell::FOS_PATHMUSTEXIST
            | Shell::FOS_DONTADDTORECENT;
        if file.kind == FileDialogKind::Folder {
            options |= Shell::FOS_PICKFOLDERS;
        }
        if file.kind == FileDialogKind::Open {
            options |= Shell::FOS_FILEMUSTEXIST;
        }
        if file.kind == FileDialogKind::Save {
            options |= Shell::FOS_OVERWRITEPROMPT | Shell::FOS_NOTESTFILECREATE;
        }
        if file.multiple {
            options |= Shell::FOS_ALLOWMULTISELECT;
        }
        dialog.SetOptions(options).map_err(native_error)?;
        if !file.title.is_empty() {
            dialog
                .SetTitle(PCWSTR(title.as_ptr()))
                .map_err(native_error)?;
        }
        if !filters.is_empty() {
            dialog.SetFileTypes(&filters).map_err(native_error)?;
            dialog
                .SetFileTypeIndex(file.filter_index as u32 + 1)
                .map_err(native_error)?;
        }
        if let Some(name) = &file.file_name {
            let text = wide(name);
            dialog
                .SetFileName(PCWSTR(text.as_ptr()))
                .map_err(native_error)?;
        }
        if let Some(extension) = &file.default_extension {
            let text = wide(extension);
            dialog
                .SetDefaultExtension(PCWSTR(text.as_ptr()))
                .map_err(native_error)?;
        }
        if let Some(directory) = &file.initial_directory {
            let mut path: Vec<u16> = directory.as_os_str().encode_wide().collect();
            if path.contains(&0) {
                return Err(DialogError::invalid("Initial directory contains NUL"));
            }
            path.push(0);
            let item: IShellItem = Shell::SHCreateItemFromParsingName(PCWSTR(path.as_ptr()), None)
                .map_err(native_error)?;
            dialog.SetFolder(&item).map_err(native_error)?;
        }
        if let Err(error) = dialog.Show(Some(owner)) {
            if error.code() == HRESULT::from_win32(1223) {
                return Ok(DialogOutcome::Cancelled);
            }
            return Err(native_error(error));
        }
        let paths = if let Some(open) = open {
            let items = open.GetResults().map_err(native_error)?;
            let count = items.GetCount().map_err(native_error)?;
            if count == 0 || count > 4096 {
                return Err(native_error("Native file selection exceeds 1–4096 paths"));
            }
            (0..count)
                .map(|index| shell_path(&items.GetItemAt(index).map_err(native_error)?))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            vec![shell_path(&dialog.GetResult().map_err(native_error)?)?]
        };
        Ok(DialogOutcome::Paths(paths))
    }
}

fn shell_path(item: &IShellItem) -> Result<PathBuf, DialogError> {
    // SIGDN_FILESYSPATH returns a COM-allocated NUL-terminated UTF-16 path.
    // OsString preserves non-Unicode Windows paths; no lossy UTF-8 conversion.
    unsafe {
        let text = item
            .GetDisplayName(Shell::SIGDN_FILESYSPATH)
            .map_err(native_error)?;
        let path = PathBuf::from(OsString::from_wide(text.as_wide()));
        CoTaskMemFree(Some(text.0.cast()));
        if !path.is_absolute() {
            return Err(native_error("Native dialog returned a non-absolute path"));
        }
        Ok(path)
    }
}
