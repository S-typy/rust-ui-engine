//! Join the retained Ribbon header to the host's native window frame.
use crate::view_ribbon::MountedRibbon;
use rust_desktop_ui_controls::{Controls, Ribbon, RibbonHeader, Theme};
use rust_desktop_ui_core::{Rect, Size};
use rust_desktop_ui_platform_winit::{TitleBar, WindowChrome};

// A 32px QAT starts below the native 8px top resize band.
const HEADER_HEIGHT: f32 = 48.0;

pub(crate) struct FormChrome {
    requested: bool,
    native: WindowChrome,
}

impl FormChrome {
    pub(crate) fn new(mode: rust_desktop_ui_xaml::TitleBar) -> Self {
        Self {
            requested: mode == rust_desktop_ui_xaml::TitleBar::Ribbon,
            native: WindowChrome::default(),
        }
    }

    pub(crate) fn set_native(&mut self, native: WindowChrome, diagnostics: bool) {
        if diagnostics && self.native != native {
            println!(
                "window-chrome-state {{\"custom\":{},\"caption_buttons\":{},\"resize_border_top\":{},\"maximized\":{},\"focused\":{}}}",
                native.custom_title_bar,
                native
                    .caption_buttons
                    .map_or_else(|| "null".into(), crate::popup_diagnostics::rectangle),
                native.resize_border_top,
                native.maximized,
                native.focused,
            );
        }
        self.native = native;
    }

    pub(crate) fn configure(
        &self,
        ribbon: &mut Ribbon,
        title: &str,
        viewport: Size,
    ) -> Result<(), String> {
        let header = (self.requested && self.native.custom_title_bar).then(|| RibbonHeader {
            title: title.to_owned(),
            height: self.height(),
            leading_inset: 12.0,
            // Reserve the native caption area relative to the client edge,
            // including any gap between the buttons and the right border.
            trailing_inset: self
                .native
                .caption_buttons
                .map_or(144.0, |rect| (viewport.width - rect.x).max(0.0) + 8.0),
        });
        ribbon.set_header(header).map_err(|error| error.to_string())
    }

    fn height(&self) -> f32 {
        let minimum = HEADER_HEIGHT.max(32.0 + 2.0 * self.native.resize_border_top);
        self.native
            .caption_buttons
            .map_or(minimum, |rect| minimum.max(rect.y + rect.height))
    }

    pub(crate) fn title_bar(
        &self,
        ui: &Controls,
        mounted: Option<&MountedRibbon>,
    ) -> Option<TitleBar> {
        if !self.requested {
            return None;
        }
        let header = mounted.and_then(|mounted| mounted.ribbon.header_regions(ui));
        let height = header.as_ref().map_or(self.height(), |regions| {
            self.height().max(regions.bounds.y + regions.bounds.height)
        });
        let mut drag_regions = Vec::new();
        // While a menu is open the header remains client input: clicking its
        // empty space dismisses the menu instead of starting a window drag.
        if !ui.popup_open()
            && let Some(regions) = header
        {
            drag_regions.push(regions.bounds);
            let reserved = mounted
                .and_then(|mounted| mounted.ribbon.header())
                .map(|header| {
                    let width = header.trailing_inset.min(regions.bounds.width);
                    Rect::new(
                        regions.bounds.x + regions.bounds.width - width,
                        regions.bounds.y,
                        width,
                        regions.bounds.height,
                    )
                });
            for rect in regions
                .interactive
                .into_iter()
                .chain(reserved)
                .chain(self.native.caption_buttons)
            {
                drag_regions = drag_regions
                    .into_iter()
                    .flat_map(|region| subtract(region, rect))
                    .collect();
            }
        }
        Some(TitleBar {
            height,
            drag_regions,
            dark: ui.theme() == Theme::FluentDark,
        })
    }
}

/// Subtract an interactive rectangle while retaining empty areas around it.
fn subtract(region: Rect, exclusion: Rect) -> Vec<Rect> {
    let left = region.x.max(exclusion.x);
    let top = region.y.max(exclusion.y);
    let right = (region.x + region.width).min(exclusion.x + exclusion.width);
    let bottom = (region.y + region.height).min(exclusion.y + exclusion.height);
    if right <= left || bottom <= top {
        return vec![region];
    }
    [
        Rect::new(region.x, region.y, region.width, top - region.y),
        Rect::new(
            region.x,
            bottom,
            region.width,
            region.y + region.height - bottom,
        ),
        Rect::new(region.x, top, left - region.x, bottom - top),
        Rect::new(right, top, region.x + region.width - right, bottom - top),
    ]
    .into_iter()
    .filter(|rect| rect.width > 0.0 && rect.height > 0.0)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::form::Form;
    use rust_desktop_ui_controls::{MenuItem, Role};
    use rust_desktop_ui_core::{InputEvent, Key, Modifiers, Mutation, Point, PointerButton};
    use rust_desktop_ui_platform_winit::{DesktopApp, PlatformEvent};

    fn custom(form: &mut Form, width: f32) -> Rect {
        let caption = Rect::new(width - 138.0, 0.0, 138.0, 32.0);
        form.window_chrome_changed(WindowChrome {
            custom_title_bar: true,
            caption_buttons: Some(caption),
            resize_border_top: 8.0,
            maximized: false,
            focused: true,
        });
        form.update(Size::new(width, 720.0), 1.5).unwrap();
        caption
    }

    fn overlaps(a: Rect, b: Rect) -> bool {
        a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y
    }

    #[test]
    fn native_fallback_has_no_duplicate_title_and_manual_ast_is_validated() {
        let mut form = Form::new(crate::generated::window_definition(), false).unwrap();
        assert!(form.title_bar().is_some()); // opt-in is available before HWND creation
        form.update(Size::new(1040.0, 720.0), 1.0).unwrap();
        assert!(
            form.view
                .ribbon
                .as_ref()
                .unwrap()
                .ribbon
                .header_regions(&form.ui)
                .is_none()
        );
        custom(&mut form, 1040.0);
        assert!(
            form.view
                .ribbon
                .as_ref()
                .unwrap()
                .ribbon
                .header_regions(&form.ui)
                .is_some()
        );
        form.window_chrome_changed(WindowChrome::default());
        form.update(Size::new(1040.0, 720.0), 1.0).unwrap();
        assert!(
            form.view
                .ribbon
                .as_ref()
                .unwrap()
                .ribbon
                .header_regions(&form.ui)
                .is_none()
        );
        let controls = Form::new(crate::generated_controls::window_definition(), false).unwrap();
        assert!(controls.title_bar().is_none());
        let mut invalid = crate::generated::window_definition();
        invalid.padding[1] = 12.0;
        assert!(Form::new(invalid, false).is_err());
        let mut invalid = crate::generated::window_definition();
        invalid.title = "я".repeat(2049);
        assert!(Form::new(invalid, false).is_err());
    }

    #[test]
    fn native_resize_band_and_unknown_caption_bounds_keep_qat_usable() {
        let mut form = Form::new(crate::generated::window_definition(), false).unwrap();
        form.window_chrome_changed(WindowChrome {
            custom_title_bar: true,
            caption_buttons: None,
            resize_border_top: 12.0,
            ..Default::default()
        });
        form.update(Size::new(480.0, 720.0), 2.0).unwrap();
        let regions = form
            .view
            .ribbon
            .as_ref()
            .unwrap()
            .ribbon
            .header_regions(&form.ui)
            .unwrap();
        assert_eq!(regions.bounds.height, 56.0);
        assert!(regions.interactive.iter().all(|rect| rect.y >= 12.0));
        let chrome = form.title_bar().unwrap();
        assert_eq!(chrome.height, regions.bounds.height);
        let reserve = Rect::new(480.0 - 144.0, 0.0, 144.0, chrome.height);
        assert!(
            chrome
                .drag_regions
                .iter()
                .all(|rect| !overlaps(*rect, reserve))
        );
    }

    #[test]
    fn header_reflow_reserves_native_buttons_and_preserves_editor_session() {
        let mut form = Form::new(crate::generated::window_definition(), false).unwrap();
        custom(&mut form, 1040.0);
        let editor = form.editor();
        form.ui
            .runtime
            .apply(Mutation::Focus(Some(editor)))
            .unwrap();
        form.ui.set_text(editor, "accepted").unwrap();
        form.ui.commit_edit(editor).unwrap();
        form.ui.set_text(editor, "accepted draft").unwrap();
        let selection = form.ui.document(editor).unwrap().selection();
        for theme in [Theme::FluentLight, Theme::FluentDark] {
            form.ui.set_theme(theme).unwrap();
            for width in [1040.0, 480.0, 280.0, 240.0, 1040.0] {
                let caption = custom(&mut form, width);
                let regions = form
                    .view
                    .ribbon
                    .as_ref()
                    .unwrap()
                    .ribbon
                    .header_regions(&form.ui)
                    .unwrap();
                assert_eq!(regions.bounds, Rect::new(0.0, 0.0, width, HEADER_HEIGHT));
                assert!(!regions.interactive.is_empty());
                for rect in &regions.interactive {
                    assert!(rect.x >= 0.0 && rect.x + rect.width <= caption.x);
                }
                let chrome = form.title_bar().unwrap();
                assert_eq!(chrome.dark, theme == Theme::FluentDark);
                assert!(!chrome.drag_regions.is_empty());
                for drag in chrome.drag_regions {
                    assert!(!overlaps(drag, caption));
                    assert!(
                        regions
                            .interactive
                            .iter()
                            .all(|rect| !overlaps(drag, *rect))
                    );
                }
                assert_eq!(form.editor(), editor);
                assert_eq!(form.text(), "accepted draft");
                assert_eq!(form.ui.accepted_value(editor), Some("accepted"));
                assert_eq!(form.ui.document(editor).unwrap().selection(), selection);
                assert_eq!(form.ui.runtime.focused(), Some(editor));
            }
        }
        form.view
            .ribbon
            .as_mut()
            .unwrap()
            .ribbon
            .set_minimized(true);
        custom(&mut form, 1040.0);
        assert!(
            form.view
                .ribbon
                .as_ref()
                .unwrap()
                .ribbon
                .header_regions(&form.ui)
                .is_some()
        );
        assert_eq!(form.ui.accepted_value(editor), Some("accepted"));
    }

    #[test]
    fn qat_remains_a_command_and_open_popup_suppresses_header_dragging() {
        let mut form = Form::new(crate::generated::window_definition(), false).unwrap();
        custom(&mut form, 1040.0);
        let qat = form
            .ui
            .semantics()
            .into_iter()
            .find(|node| {
                node.role == Role::ToggleButton
                    && node.label == "Закрепить"
                    && node.bounds.y < HEADER_HEIGHT
            })
            .unwrap();
        let position = Point::new(
            qat.bounds.x + qat.bounds.width / 2.0,
            qat.bounds.y + qat.bounds.height / 2.0,
        );
        for input in [
            InputEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            InputEvent::PointerUp {
                position,
                button: PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
        ] {
            form.event(PlatformEvent::Input(input)).unwrap();
        }
        assert_eq!(form.ui.commands().checked(&"Pin".into()), Some(true));
        form.ui
            .popup_menu(qat.id, &[MenuItem::new("Clear", "Clear")])
            .unwrap();
        assert!(form.title_bar().unwrap().drag_regions.is_empty());
        form.ui.close_popup().unwrap();
        assert!(!form.title_bar().unwrap().drag_regions.is_empty());
        for key in [Key::F10, Key::Character("2".into())] {
            form.event(PlatformEvent::Input(InputEvent::KeyDown {
                key,
                modifiers: Modifiers::default(),
                repeat: false,
            }))
            .unwrap();
        }
        assert_eq!(form.ui.commands().checked(&"Pin".into()), Some(false));
    }
}
