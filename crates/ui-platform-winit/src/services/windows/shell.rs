use super::*;
use ::windows::{
    Win32::{
        Foundation::{HWND as ComHwnd, PROPERTYKEY},
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize, StructuredStorage::PROPVARIANT,
        },
        UI::Shell::{
            self as com,
            Common::{IObjectArray, IObjectCollection},
            PropertiesSystem::IPropertyStore,
        },
    },
    core::{Interface, PCWSTR},
};
use windows_sys::Win32::Graphics::Gdi::*;
fn com_error(_: impl fmt::Display) -> ServiceError {
    native("Windows shell operation failed")
}
pub(super) struct Icon(HICON);
impl Icon {
    fn new(image: &ClipboardImage) -> ServiceResult<Self> {
        image.validate()?;
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: image.width as i32,
                biHeight: -(image.height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        // SAFETY: bounded dimensions, initialized DIB header and output pointer.
        let color = unsafe {
            CreateDIBSection(
                std::ptr::null_mut(),
                &info,
                DIB_RGB_COLORS,
                &mut bits,
                std::ptr::null_mut(),
                0,
            )
        };
        if color.is_null() {
            return Err(native("Cannot create shell icon bitmap"));
        }
        if bits.is_null() {
            unsafe {
                DeleteObject(color);
            }
            return Err(native("Cannot map shell icon bitmap"));
        }
        let target = unsafe { std::slice::from_raw_parts_mut(bits as *mut u8, image.rgba.len()) };
        for (s, d) in image.rgba.chunks_exact(4).zip(target.chunks_exact_mut(4)) {
            let alpha = u16::from(s[3]);
            d.copy_from_slice(&[
                ((u16::from(s[2]) * alpha + 127) / 255) as u8,
                ((u16::from(s[1]) * alpha + 127) / 255) as u8,
                ((u16::from(s[0]) * alpha + 127) / 255) as u8,
                s[3],
            ]);
        }
        let mask_bytes = vec![0u8; (image.width as usize).div_ceil(16) * 2 * image.height as usize];
        let mask = unsafe {
            CreateBitmap(
                image.width as i32,
                image.height as i32,
                1,
                1,
                mask_bytes.as_ptr() as *const _,
            )
        };
        if mask.is_null() {
            unsafe {
                DeleteObject(color);
            }
            return Err(native("Cannot create shell icon mask"));
        }
        let info = ICONINFO {
            fIcon: 1,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = unsafe { CreateIconIndirect(&info) };
        // CreateIconIndirect copies the bitmap contents; both originals are ours.
        unsafe {
            DeleteObject(mask);
            DeleteObject(color);
        }
        if icon.is_null() {
            Err(native("Cannot create shell icon"))
        } else {
            Ok(Self(icon))
        }
    }
}
impl Drop for Icon {
    fn drop(&mut self) {
        unsafe {
            DestroyIcon(self.0);
        }
    }
}
pub(super) struct Tray {
    hwnd: HWND,
    id: u16,
    icon: Icon,
    spec: TrayIcon,
    owner_alive: Rc<Cell<bool>>,
}
impl Tray {
    pub(super) fn new(
        hwnd: HWND,
        id: u16,
        spec: &TrayIcon,
        owner_alive: Rc<Cell<bool>>,
    ) -> ServiceResult<Self> {
        let tray = Self {
            hwnd,
            id,
            icon: Icon::new(&spec.image)?,
            spec: spec.clone(),
            owner_alive,
        };
        tray.add()?;
        Ok(tray)
    }
    fn data(&self) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: u32::from(self.id),
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: TRAY_MESSAGE,
            hIcon: self.icon.0,
            ..Default::default()
        };
        for (target, unit) in data.szTip.iter_mut().zip(self.spec.tooltip.encode_utf16()) {
            *target = unit;
        }
        data
    }
    pub(super) fn add(&self) -> ServiceResult<()> {
        if !self.owner_alive.get() {
            return Err(ServiceError::new(
                ServiceErrorKind::OwnerClosed,
                "Tray owner closed",
            ));
        }
        let mut data = self.data();
        if unsafe { Shell_NotifyIconW(NIM_ADD, &data) } == 0 {
            return Err(native("Tray icon is unavailable"));
        }
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        if unsafe { Shell_NotifyIconW(NIM_SETVERSION, &data) } == 0 {
            unsafe {
                Shell_NotifyIconW(NIM_DELETE, &data);
            }
            return Err(native("Tray icon callback version is unavailable"));
        }
        Ok(())
    }
    pub(super) fn menu(&self) -> ServiceResult<Option<u32>> {
        let menu = unsafe { CreatePopupMenu() };
        if menu.is_null() {
            return Err(native("Cannot create tray menu"));
        }
        struct Menu(HMENU);
        impl Drop for Menu {
            fn drop(&mut self) {
                unsafe {
                    DestroyMenu(self.0);
                }
            }
        }
        let menu = Menu(menu);
        for action in &self.spec.actions {
            let flags = MF_STRING
                | if action.enabled {
                    MF_ENABLED
                } else {
                    MF_GRAYED
                };
            if unsafe {
                AppendMenuW(
                    menu.0,
                    flags,
                    action.id as usize,
                    wide(&action.label).as_ptr(),
                )
            } == 0
            {
                return Err(native("Cannot add tray action"));
            }
        }
        let mut point = windows_sys::Win32::Foundation::POINT::default();
        if unsafe { GetCursorPos(&mut point) } == 0 {
            return Err(native("Cannot place tray menu"));
        }
        unsafe {
            SetForegroundWindow(self.hwnd);
        }
        let command = unsafe {
            TrackPopupMenu(
                menu.0,
                TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                0,
                self.hwnd,
                std::ptr::null(),
            )
        } as u32;
        if self.owner_alive.get() {
            unsafe {
                PostMessageW(self.hwnd, WM_NULL, 0, 0);
                Shell_NotifyIconW(NIM_SETFOCUS, &self.data());
            }
        }
        Ok(self
            .spec
            .actions
            .iter()
            .find(|a| a.id == command && a.enabled)
            .map(|a| a.id))
    }
}
impl Drop for Tray {
    fn drop(&mut self) {
        if self.owner_alive.get() {
            unsafe {
                Shell_NotifyIconW(NIM_DELETE, &self.data());
            }
        }
    }
}
struct Apartment;
impl Apartment {
    fn enter() -> ServiceResult<Self> {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(com_error)?;
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
pub(super) fn progress(hwnd: HWND, value: TaskbarProgress) -> ServiceResult<()> {
    let _apartment = Apartment::enter()?;
    let taskbar: com::ITaskbarList3 =
        unsafe { CoCreateInstance(&com::TaskbarList, None, CLSCTX_INPROC_SERVER) }
            .map_err(com_error)?;
    unsafe { taskbar.HrInit() }.map_err(com_error)?;
    let hwnd = ComHwnd(hwnd);
    let (state, range) = match value {
        TaskbarProgress::None => (com::TBPF_NOPROGRESS, None),
        TaskbarProgress::Indeterminate => (com::TBPF_INDETERMINATE, None),
        TaskbarProgress::Normal { completed, total } => {
            (com::TBPF_NORMAL, Some((completed, total)))
        }
        TaskbarProgress::Error { completed, total } => (com::TBPF_ERROR, Some((completed, total))),
        TaskbarProgress::Paused { completed, total } => {
            (com::TBPF_PAUSED, Some((completed, total)))
        }
    };
    if let Some((completed, total)) = range {
        unsafe { taskbar.SetProgressValue(hwnd, completed, total) }.map_err(com_error)?;
    }
    unsafe { taskbar.SetProgressState(hwnd, state) }.map_err(com_error)
}
pub(super) fn overlay(
    hwnd: HWND,
    image: Option<&ClipboardImage>,
    description: &str,
) -> ServiceResult<()> {
    let icon = image.map(Icon::new).transpose()?;
    let _apartment = Apartment::enter()?;
    let taskbar: com::ITaskbarList3 =
        unsafe { CoCreateInstance(&com::TaskbarList, None, CLSCTX_INPROC_SERVER) }
            .map_err(com_error)?;
    unsafe { taskbar.HrInit() }.map_err(com_error)?;
    unsafe {
        taskbar.SetOverlayIcon(
            ComHwnd(hwnd),
            ::windows::Win32::UI::WindowsAndMessaging::HICON(
                icon.as_ref().map_or(std::ptr::null_mut(), |v| v.0),
            ),
            PCWSTR(wide(description).as_ptr()),
        )
    }
    .map_err(com_error)
}
pub(super) fn jump_list(value: &JumpList) -> ServiceResult<()> {
    use std::os::windows::ffi::OsStrExt;
    let exe =
        std::env::current_exe().map_err(|_| native("Cannot determine application executable"))?;
    let exe: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let _apartment = Apartment::enter()?;
    let destination: com::ICustomDestinationList =
        unsafe { CoCreateInstance(&com::DestinationList, None, CLSCTX_INPROC_SERVER) }
            .map_err(com_error)?;
    unsafe { destination.SetAppID(PCWSTR(wide(&value.app_id).as_ptr())) }.map_err(com_error)?;
    let mut slots = 0;
    let _removed: IObjectArray = unsafe { destination.BeginList(&mut slots) }.map_err(com_error)?;
    struct Transaction<'a>(&'a com::ICustomDestinationList, bool);
    impl Drop for Transaction<'_> {
        fn drop(&mut self) {
            if !self.1 {
                let _ = unsafe { self.0.AbortList() };
            }
        }
    }
    let mut transaction = Transaction(&destination, false);
    let collection: IObjectCollection =
        unsafe { CoCreateInstance(&com::EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER) }
            .map_err(com_error)?;
    for task in &value.tasks {
        let link: com::IShellLinkW =
            unsafe { CoCreateInstance(&com::ShellLink, None, CLSCTX_INPROC_SERVER) }
                .map_err(com_error)?;
        unsafe { link.SetPath(PCWSTR(exe.as_ptr())) }.map_err(com_error)?;
        unsafe { link.SetArguments(PCWSTR(wide(&task.arguments).as_ptr())) }.map_err(com_error)?;
        unsafe { link.SetIconLocation(PCWSTR(exe.as_ptr()), 0) }.map_err(com_error)?;
        let properties: IPropertyStore = link.cast().map_err(com_error)?;
        let title = PROPERTYKEY {
            fmtid: ::windows::core::GUID::from_u128(0xf29f85e0_4ff9_1068_ab91_08002b27b3d9),
            pid: 2,
        };
        unsafe { properties.SetValue(&title, &PROPVARIANT::from(task.title.as_str())) }
            .map_err(com_error)?;
        unsafe { properties.Commit() }.map_err(com_error)?;
        unsafe { collection.AddObject(&link) }.map_err(com_error)?;
    }
    let array: IObjectArray = collection.cast().map_err(com_error)?;
    unsafe { destination.AddUserTasks(&array) }.map_err(com_error)?;
    unsafe { destination.CommitList() }.map_err(com_error)?;
    transaction.1 = true;
    Ok(())
}
pub(super) fn delete_jump_list(id: &str) -> ServiceResult<()> {
    let _apartment = Apartment::enter()?;
    let destination: com::ICustomDestinationList =
        unsafe { CoCreateInstance(&com::DestinationList, None, CLSCTX_INPROC_SERVER) }
            .map_err(com_error)?;
    unsafe { destination.DeleteList(PCWSTR(wide(id).as_ptr())) }.map_err(com_error)
}

// Window properties are restored before WM_NCDESTROY, not retained until COM
// reference teardown. Only this owned HWND is touched; no process AppID change.
pub(super) struct WindowIdentity {
    hwnd: HWND,
    original: PROPVARIANT,
    pub(super) id: String,
}
fn app_key() -> PROPERTYKEY {
    PROPERTYKEY {
        fmtid: ::windows::core::GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
        pid: 5,
    }
}
impl WindowIdentity {
    pub(super) fn bind(hwnd: HWND, id: &str) -> ServiceResult<Self> {
        let _apartment = Apartment::enter()?;
        let properties: IPropertyStore =
            unsafe { com::PropertiesSystem::SHGetPropertyStoreForWindow(ComHwnd(hwnd)) }
                .map_err(com_error)?;
        let original = unsafe { properties.GetValue(&app_key()) }.map_err(com_error)?;
        let guard = Self {
            hwnd,
            original,
            id: id.into(),
        };
        unsafe { properties.SetValue(&app_key(), &PROPVARIANT::from(id)) }.map_err(com_error)?;
        unsafe { properties.Commit() }.map_err(com_error)?;
        Ok(guard)
    }
}
impl Drop for WindowIdentity {
    fn drop(&mut self) {
        let Ok(_apartment) = Apartment::enter() else {
            return;
        };
        if unsafe { IsWindow(self.hwnd) } == 0 {
            return;
        }
        if let Ok(properties) = unsafe {
            com::PropertiesSystem::SHGetPropertyStoreForWindow::<IPropertyStore>(ComHwnd(self.hwnd))
        } {
            let current = unsafe { properties.GetValue(&app_key()) };
            let ours = current
                .as_ref()
                .ok()
                .and_then(|v| ::windows::core::BSTR::try_from(v).ok())
                .is_some_and(|v| v == self.id);
            if !ours {
                return;
            }
            let _ = unsafe { properties.SetValue(&app_key(), &self.original) };
            let _ = unsafe { properties.Commit() };
        }
    }
}
