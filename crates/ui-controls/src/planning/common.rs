use crate::{
    ButtonAppearance, ButtonStyle, ControlError, Controls, ScrollBarVisibility, ScrollSettings,
    ScrollViewer,
};
use rust_desktop_ui_core::{
    Axis, Edges, LayoutKind, LayoutStyle, Length, Mutation, Point, Rect, Scene, Size, TextAlign,
    TextRun, WidgetId,
};
use rust_desktop_ui_data::{DateValue, ScheduleLocal, TimeValue};
pub(super) fn error(e: impl std::fmt::Display) -> ControlError {
    ControlError::Invalid(e.to_string())
}
pub(super) fn size(width: f32, height: f32) -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(width),
        height: Length::Px(height),
        ..Default::default()
    }
}
pub(super) fn position(bounds: Rect) -> LayoutStyle {
    LayoutStyle {
        offset: Point::new(bounds.x, bounds.y),
        ..size(bounds.width, bounds.height)
    }
}
pub(super) fn set_style(
    ui: &mut Controls,
    id: WidgetId,
    style: LayoutStyle,
) -> Result<bool, ControlError> {
    let old = &ui
        .runtime
        .tree()
        .node(id)
        .ok_or_else(|| error("planner control was removed"))?
        .props
        .style;
    if old == &style {
        return Ok(false);
    }
    ui.runtime
        .apply(Mutation::SetStyle { node: id, style })
        .map_err(ControlError::from)?;
    Ok(true)
}
pub(super) fn set_visible(
    ui: &mut Controls,
    id: WidgetId,
    visible: bool,
) -> Result<(), ControlError> {
    let mut props = ui
        .runtime
        .tree()
        .node(id)
        .ok_or_else(|| error("planner control was removed"))?
        .props
        .clone();
    if props.collapsed != visible {
        return Ok(());
    }
    props.collapsed = !visible;
    ui.runtime
        .apply(Mutation::SetProps { node: id, props })
        .map_err(ControlError::from)
}
pub(super) fn stack(
    ui: &mut Controls,
    parent: WidgetId,
    height: f32,
) -> Result<WidgetId, ControlError> {
    ui.transparent_panel(
        parent,
        LayoutStyle {
            kind: LayoutKind::Wrap(Axis::Horizontal),
            width: Length::Percent(1.0),
            height: Length::Px(height),
            gap: 4.0,
            ..Default::default()
        },
    )
}
pub(super) fn scroll(
    ui: &mut Controls,
    parent: WidgetId,
    height: f32,
) -> Result<ScrollViewer, ControlError> {
    let view = ui.scroll_view(
        parent,
        LayoutStyle {
            width: Length::Percent(1.0),
            height: Length::Px(height),
            ..Default::default()
        },
        height,
    )?;
    ui.configure_scroll_view(
        view.viewport,
        ScrollSettings {
            horizontal: ScrollBarVisibility::Auto,
            vertical: ScrollBarVisibility::Auto,
        },
    )?;
    set_style(
        ui,
        view.content,
        LayoutStyle {
            kind: LayoutKind::Overlay,
            width: Length::Px(800.0),
            height: Length::Px(height),
            ..Default::default()
        },
    )?;
    Ok(view)
}
pub(super) fn accent(
    ui: &mut Controls,
    id: WidgetId,
    selected: bool,
    critical: bool,
) -> Result<(), ControlError> {
    let t = ui.theme().tokens();
    ui.set_button_style(
        id,
        ButtonStyle {
            font_size: Some(12.0),
            padding: Some(Edges::all(4.0)),
            corner_radius: Some(4.0),
            normal: ButtonAppearance {
                background: Some(if selected { t.selected } else { t.surface }),
                border: Some(if critical {
                    t.error
                } else if selected {
                    t.focus
                } else {
                    t.border
                }),
                foreground: Some(t.text),
                ..Default::default()
            },
            ..Default::default()
        },
    )
}
#[derive(Clone, Debug, PartialEq)]
pub struct PlannerCaption {
    pub bounds: Rect,
    pub text: String,
    pub heading: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PlannerRule {
    pub bounds: Rect,
}
pub(super) fn scene(
    ui: &Controls,
    origin: Point,
    extent: Size,
    captions: &[PlannerCaption],
    rules: &[PlannerRule],
) -> Scene {
    let t = ui.theme().tokens();
    let clip = Rect::new(origin.x, origin.y, extent.width, extent.height);
    let mut scene = Scene::default();
    for rule in rules {
        let mut b = rule.bounds;
        b.x += origin.x;
        b.y += origin.y;
        scene.fill(b, t.border);
    }
    for caption in captions {
        let mut b = caption.bounds;
        b.x += origin.x;
        b.y += origin.y;
        scene.text(TextRun {
            sensitive: false,
            bounds: b,
            clip,
            text: caption.text.clone(),
            font_size: if caption.heading { 13.0 } else { 11.0 },
            color: if caption.heading { t.text } else { t.muted },
            weight: if caption.heading { 600 } else { 400 },
            align: TextAlign::Start,
            wrap: false,
        });
    }
    scene
}
pub(super) fn local_text(value: ScheduleLocal) -> String {
    let d = value.date();
    let t = value.time();
    format!(
        "{:02}.{:02}.{:04} {:02}:{:02}:{:02}",
        d.day(),
        d.month(),
        d.year(),
        t.hour(),
        t.minute(),
        t.second()
    )
}
pub(super) fn parse_local(text: &str) -> Result<ScheduleLocal, ControlError> {
    let parts = text.split(['.', ' ', ':']).collect::<Vec<_>>();
    if parts.len() != 6 {
        return Err(error("Дата и время: ДД.ММ.ГГГГ ЧЧ:ММ:СС"));
    }
    let parse = |s: &str| s.parse::<u16>().map_err(error);
    let to_u8 = |s: &str| u8::try_from(parse(s)?).map_err(error);
    let date =
        DateValue::new(parse(parts[2])?, to_u8(parts[1])?, to_u8(parts[0])?).map_err(error)?;
    let time =
        TimeValue::new(to_u8(parts[3])?, to_u8(parts[4])?, to_u8(parts[5])?, 0).map_err(error)?;
    let value = ScheduleLocal::new(date, time).map_err(error)?;
    if local_text(value) != text {
        return Err(error("Дата и время: ДД.ММ.ГГГГ ЧЧ:ММ:СС"));
    }
    Ok(value)
}
pub(super) fn utc_zone() -> Result<rust_desktop_ui_data::ScheduleTimeZone, ControlError> {
    use rust_desktop_ui_data::{ScheduleInstant, ScheduleTimeZone, ScheduleUtcInterval};
    ScheduleTimeZone::new(
        "UTC",
        ScheduleUtcInterval::new(ScheduleInstant(-62135596800), ScheduleInstant(253402300799))
            .map_err(error)?,
        0,
        vec![],
    )
    .map_err(error)
}
pub(super) fn utc_text(value: rust_desktop_ui_data::ScheduleInstant) -> String {
    utc_zone()
        .and_then(|z| z.to_local(value).map_err(error))
        .map(local_text)
        .unwrap_or_else(|_| format!("UTC {}", value.0))
}
pub(super) fn parse_utc(text: &str) -> Result<rust_desktop_ui_data::ScheduleInstant, ControlError> {
    utc_zone()?
        .resolve(parse_local(text)?, Default::default())
        .map_err(error)?
        .ok_or_else(|| error("UTC interval cannot be skipped"))
}
