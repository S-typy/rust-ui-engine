//! STA-scoped Unicode text drag and drop for the single-window Windows host.
use super::NativeTextDrag;
use rust_desktop_ui_core::Point;
use std::{cell::Cell, rc::Rc, sync::Arc};
use windows::{
    Win32::{
        Foundation::{
            DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, E_INVALIDARG, HWND,
            POINT, POINTL, S_OK,
        },
        Graphics::Gdi::ScreenToClient,
        System::{
            Com::{
                DVASPECT_CONTENT, FORMATETC, IDataObject, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL,
            },
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
            Ole::{
                CF_UNICODETEXT, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_MOVE, DROPEFFECT_NONE,
                DoDragDrop, IDropSource, IDropSource_Impl, IDropTarget, IDropTarget_Impl,
                OleInitialize, OleUninitialize, RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop,
            },
            SystemServices::{MK_CONTROL, MK_LBUTTON, MK_SHIFT, MODIFIERKEYS_FLAGS},
        },
        UI::Shell::SHCreateDataObject,
    },
    core::{BOOL, HRESULT, Ref, Result, implement},
};
use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};
const MAX_TEXT: usize = 4 * 1024 * 1024;
struct Apartment(std::marker::PhantomData<Rc<()>>);
impl Apartment {
    fn enter() -> Result<Self> {
        unsafe { OleInitialize(None)? };
        Ok(Self(Default::default()))
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { OleUninitialize() };
    }
}
struct Medium(STGMEDIUM);
impl Drop for Medium {
    fn drop(&mut self) {
        unsafe { ReleaseStgMedium(&mut self.0) };
    }
}
fn format() -> FORMATETC {
    FORMATETC {
        cfFormat: CF_UNICODETEXT.0,
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
        ..Default::default()
    }
}
fn data_object(text: &str) -> Result<IDataObject> {
    if text.len() > MAX_TEXT || text.contains('\0') {
        return Err(E_INVALIDARG.into());
    }
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let object: IDataObject = unsafe { SHCreateDataObject(None, None, None) }?;
    let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) }?;
    let medium = Medium(STGMEDIUM {
        tymed: TYMED_HGLOBAL.0 as u32,
        u: STGMEDIUM_0 { hGlobal: handle },
        ..Default::default()
    });
    let dst = unsafe { GlobalLock(handle) }.cast::<u16>();
    if dst.is_null() {
        return Err(windows::core::Error::from_thread());
    }
    unsafe {
        std::ptr::copy_nonoverlapping(wide.as_ptr(), dst, wide.len());
        let _ = GlobalUnlock(handle);
    }
    // On success the object owns the HGLOBAL. On failure Medium releases it.
    unsafe { object.SetData(&format(), &medium.0, true) }?;
    std::mem::forget(medium);
    Ok(object)
}
fn read_text(object: &IDataObject) -> Option<String> {
    let medium = Medium(unsafe { object.GetData(&format()) }.ok()?);
    if medium.0.tymed != TYMED_HGLOBAL.0 as u32 {
        return None;
    }
    let handle = unsafe { medium.0.u.hGlobal };
    let bytes = unsafe { GlobalSize(handle) };
    if !(2..=MAX_TEXT * 2 + 16).contains(&bytes) {
        return None;
    }
    let ptr = unsafe { GlobalLock(handle) }.cast::<u16>();
    if ptr.is_null() {
        return None;
    }
    let wide = unsafe { std::slice::from_raw_parts(ptr, bytes / 2) };
    let value = wide
        .iter()
        .position(|v| *v == 0)
        .and_then(|n| String::from_utf16(&wide[..n]).ok())
        .filter(|s| s.len() <= MAX_TEXT);
    unsafe {
        let _ = GlobalUnlock(handle);
    }
    value
}
#[implement(IDropSource)]
struct Source;
impl IDropSource_Impl for Source_Impl {
    fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
        if escape.as_bool() {
            DRAGDROP_S_CANCEL
        } else if !keys.contains(MK_LBUTTON) {
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }
    fn GiveFeedback(&self, _: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}
pub(super) fn start(request: &NativeTextDrag) -> Result<bool> {
    let _apt = Apartment::enter()?;
    let object = data_object(&request.text)?;
    let source: IDropSource = Source.into();
    let allowed = if request.allow_move {
        DROPEFFECT_COPY | DROPEFFECT_MOVE
    } else {
        DROPEFFECT_COPY
    };
    let mut effect = DROPEFFECT_NONE;
    let result = unsafe { DoDragDrop(&object, &source, allowed, &mut effect) };
    result.ok()?;
    Ok(request.allow_move && result == DRAGDROP_S_DROP && effect.contains(DROPEFFECT_MOVE))
}
type Receive = Box<dyn Fn(POINTL, Option<&str>) -> bool>;
#[implement(IDropTarget)]
struct Target {
    supported: Cell<bool>,
    receive: Receive,
}
fn choose(allowed: DROPEFFECT, keys: MODIFIERKEYS_FLAGS) -> DROPEFFECT {
    let requested = if keys.contains(MK_CONTROL) {
        DROPEFFECT_COPY
    } else if keys.contains(MK_SHIFT) {
        DROPEFFECT_MOVE
    } else if allowed.contains(DROPEFFECT_COPY) {
        DROPEFFECT_COPY
    } else {
        DROPEFFECT_MOVE
    };
    if allowed.contains(requested) {
        requested
    } else {
        DROPEFFECT_NONE
    }
}
impl Target {
    fn reply(
        &self,
        keys: MODIFIERKEYS_FLAGS,
        p: &POINTL,
        effect: *mut DROPEFFECT,
        text: Option<&str>,
    ) -> Result<()> {
        let out = unsafe { effect.as_mut() }.ok_or(E_INVALIDARG)?;
        let candidate = choose(*out, keys);
        *out = DROPEFFECT_NONE;
        if self.supported.get()
            && candidate != DROPEFFECT_NONE
            && std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (self.receive)(*p, text)))
                .unwrap_or(false)
        {
            *out = candidate;
        }
        Ok(())
    }
}
impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(
        &self,
        data: Ref<IDataObject>,
        keys: MODIFIERKEYS_FLAGS,
        p: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        self.supported.set(
            data.as_ref()
                .is_some_and(|d| unsafe { d.QueryGetData(&format()) }.is_ok()),
        );
        self.reply(keys, p, effect, None)
    }
    fn DragOver(
        &self,
        keys: MODIFIERKEYS_FLAGS,
        p: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        self.reply(keys, p, effect, None)
    }
    fn DragLeave(&self) -> Result<()> {
        self.supported.set(false);
        Ok(())
    }
    fn Drop(
        &self,
        data: Ref<IDataObject>,
        keys: MODIFIERKEYS_FLAGS,
        p: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        let text = data.as_ref().and_then(read_text);
        self.supported.set(text.is_some());
        let result = self.reply(keys, p, effect, text.as_deref());
        self.supported.set(false);
        result
    }
}
pub(super) struct Registration {
    hwnd: HWND,
    _window: Arc<Window>,
    _apartment: Apartment,
}
impl Registration {
    pub fn attach(
        window: Arc<Window>,
        receive: impl Fn(Point, Option<&str>) -> bool + 'static,
    ) -> Result<Self> {
        let apartment = Apartment::enter()?;
        let RawWindowHandle::Win32(h) = window.window_handle().map_err(|_| E_INVALIDARG)?.as_raw()
        else {
            return Err(E_INVALIDARG.into());
        };
        let hwnd = HWND(h.hwnd.get() as *mut _);
        let geometry = window.clone();
        let target: IDropTarget = Target {
            supported: Cell::new(false),
            receive: Box::new(move |p, text| {
                let mut pt = POINT { x: p.x, y: p.y };
                if !unsafe { ScreenToClient(hwnd, &mut pt) }.as_bool() {
                    return false;
                }
                let scale = geometry.scale_factor() as f32;
                receive(Point::new(pt.x as f32 / scale, pt.y as f32 / scale), text)
            }),
        }
        .into();
        unsafe { RegisterDragDrop(hwnd, &target) }?;
        Ok(Self {
            hwnd,
            _window: window,
            _apartment: apartment,
        })
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        unsafe {
            let _ = RevokeDragDrop(self.hwnd);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ole_object_negotiates_and_commits_before_move_acknowledgement() {
        let _apt = Apartment::enter().unwrap();
        let data = data_object("Я\tпроверка\r\n🦀").unwrap();
        assert_eq!(read_text(&data).as_deref(), Some("Я\tпроверка\r\n🦀"));
        let accepted = Rc::new(Cell::new(false));
        let flag = accepted.clone();
        let target: IDropTarget = Target {
            supported: Cell::new(false),
            receive: Box::new(move |_, text| {
                if let Some(t) = text {
                    flag.set(t.contains('🦀'));
                }
                true
            }),
        }
        .into();
        let p = POINTL { x: 0, y: 0 };
        let mut effect = DROPEFFECT_COPY | DROPEFFECT_MOVE;
        unsafe {
            target.DragEnter(&data, MK_CONTROL, p, &mut effect).unwrap();
        }
        assert_eq!(effect, DROPEFFECT_COPY);
        assert!(!accepted.get());
        effect = DROPEFFECT_COPY;
        unsafe {
            target.DragOver(MK_SHIFT, p, &mut effect).unwrap();
        }
        assert_eq!(effect, DROPEFFECT_NONE);
        effect = DROPEFFECT_MOVE;
        unsafe {
            target.Drop(&data, MK_SHIFT, p, &mut effect).unwrap();
        }
        assert_eq!(effect, DROPEFFECT_MOVE);
        assert!(accepted.get());
        assert!(data_object("bad\0text").is_err());
    }
}
