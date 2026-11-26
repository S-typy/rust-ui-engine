use super::*;
use crate::barcode::BarcodeSymbology;
use rust_desktop_ui_data::FieldType;
use std::borrow::Cow;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Slider,
    Zoom,
    Range,
    Rating,
    Progress,
    Barcode,
}

impl Kind {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "Slider" => Self::Slider,
            "ZoomSlider" => Self::Zoom,
            "RangeSlider" => Self::Range,
            "Rating" => Self::Rating,
            "ProgressBar" => Self::Progress,
            "Barcode" => Self::Barcode,
            _ => return None,
        })
    }
}

pub(super) fn supports(name: &str) -> bool {
    Kind::parse(name).is_some()
}

#[derive(Clone, Copy)]
struct Limits {
    minimum: i64,
    maximum: i64,
    step: i64,
}

impl Limits {
    fn from_descriptor(d: &PropertyDescriptor, kind: Kind) -> Result<Self, ControlError> {
        let limits = Self {
            minimum: d.minimum.unwrap_or(0),
            maximum: d
                .maximum
                .unwrap_or(if kind == Kind::Rating { 5 } else { 100 }),
            step: d.step.unwrap_or(1),
        };
        // The existing controls compute positions with f64. Within this domain
        // every integer endpoint, step and the full span are represented exactly.
        const LIMIT: i64 = 1 << 52;
        if !(-LIMIT..=LIMIT).contains(&limits.minimum)
            || !(-LIMIT..=LIMIT).contains(&limits.maximum)
            || limits.minimum >= limits.maximum
            || !(1..=LIMIT).contains(&limits.step)
        {
            return Err(invalid(
                "Invalid integer range or step for this property editor",
            ));
        }
        if kind == Kind::Rating
            && (limits.minimum != 0 || !(1..=20).contains(&limits.maximum) || limits.step != 1)
        {
            return Err(invalid(
                "Rating requires minimum 0, maximum 1..20 and step 1",
            ));
        }
        Ok(limits)
    }

    fn scalar(self, value: &CellValue, integer: bool) -> Result<f64, ControlError> {
        let number = match value {
            CellValue::Null => return Ok(self.minimum as f64),
            CellValue::Integer(n) if integer && (self.minimum..=self.maximum).contains(n) => {
                *n as f64
            }
            CellValue::Real(n) if !integer => *n,
            _ => {
                return Err(invalid(
                    "Property value has an incompatible numeric type or range",
                ));
            }
        };
        if !(self.minimum as f64..=self.maximum as f64).contains(&number) {
            return Err(invalid("Property value is outside the editor range"));
        }
        Ok(number)
    }

    fn pair(self, value: &CellValue) -> Result<(f64, f64), ControlError> {
        if *value == CellValue::Null {
            return Ok((self.minimum as f64, self.maximum as f64));
        }
        let pair = match value {
            CellValue::Text(s) => s.split_once(".."),
            _ => None,
        };
        let parse = |raw: &str| raw.parse::<i64>().ok().filter(|n| n.to_string() == raw);
        if let Some((lower, upper)) = pair
            && let (Some(l), Some(u)) = (parse(lower), parse(upper))
            && self.minimum <= l
            && l <= u
            && u <= self.maximum
        {
            return Ok((l as f64, u as f64));
        }
        Err(invalid(
            "RangeSlider requires an ordered pair in canonical lower..upper notation within the configured bounds",
        ))
    }
}

fn barcode_settings() -> BarcodeSettings {
    BarcodeSettings {
        symbology: BarcodeSymbology::Code128B,
        module_size: 1,
        bar_height: 18,
        show_text: false,
    }
}

fn barcode_text(
    value: &CellValue,
    integer: bool,
    nullable: bool,
) -> Result<Cow<'_, str>, ControlError> {
    match value {
        CellValue::Text(s) if !integer => {
            barcode_settings().validate_value(s)?;
            Ok(Cow::Borrowed(s))
        }
        CellValue::Integer(n) if integer => Ok(Cow::Owned(n.to_string())),
        CellValue::Null if nullable => Ok(Cow::Borrowed("")),
        _ => Err(invalid(
            "Barcode value must match the text or integer property",
        )),
    }
}

pub(super) fn validate(
    d: &PropertyDescriptor,
    f: &FieldSchema,
    v: &CellValue,
) -> Result<(), ControlError> {
    let kind =
        Kind::parse(&d.metadata.editor).ok_or_else(|| invalid("Unknown inline property editor"))?;
    f.validate(v).map_err(|e| invalid(e.to_string()))?;
    if !d.choices.is_empty() {
        return Err(invalid(
            "Inline numeric and barcode editors do not accept Choices",
        ));
    }
    if kind == Kind::Barcode {
        if !matches!(f.kind, FieldType::Text | FieldType::Integer)
            || d.minimum.is_some()
            || d.maximum.is_some()
            || d.step.is_some()
        {
            return Err(invalid(
                "Barcode requires a text or integer field and no numeric range",
            ));
        }
        barcode_text(v, f.kind == FieldType::Integer, f.nullable)?;
        return Ok(());
    }
    let limits = Limits::from_descriptor(d, kind)?;
    if kind == Kind::Range {
        if f.kind != FieldType::Text {
            return Err(invalid(
                "RangeSlider requires a text field containing lower..upper",
            ));
        }
        limits.pair(v)?;
    } else {
        if !matches!(f.kind, FieldType::Integer | FieldType::Real) {
            return Err(invalid(
                "Slider, zoom, progress and rating require an integer or real property",
            ));
        }
        if kind == Kind::Progress && d.step.is_some() {
            return Err(invalid("ProgressBar does not have an editing step"));
        }
        limits.scalar(v, f.kind == FieldType::Integer)?;
    }
    Ok(())
}

struct NumericEditor {
    kind: Kind,
    owner: WidgetId,
    focus: WidgetId,
    limits: Limits,
    integer: bool,
    nullable: bool,
}

impl InspectorEditor for NumericEditor {
    fn owner(&self) -> WidgetId {
        self.owner
    }
    fn focus(&self) -> WidgetId {
        self.focus
    }
    fn inline(&self) -> bool {
        true
    }

    fn load(&self, ui: &mut Controls, value: &CellValue) -> Result<(), ControlError> {
        if *value == CellValue::Null && !self.nullable {
            return Err(invalid("This property does not allow NULL"));
        }
        if self.kind == Kind::Range {
            let (lower, upper) = self.limits.pair(value)?;
            return ui.set_range_slider_values(self.owner, lower, upper);
        }
        let number = self.limits.scalar(value, self.integer)?;
        match self.kind {
            Kind::Slider => ui.set_slider_value(self.owner, number),
            Kind::Zoom => ui.set_zoom_value(self.owner, number),
            Kind::Rating => {
                ui.set_rating_value(self.owner, (*value != CellValue::Null).then_some(number))
            }
            Kind::Progress => ui.set_progress_value(self.owner, number),
            _ => unreachable!(),
        }
    }

    fn read(&self, ui: &Controls) -> Result<CellValue, String> {
        let removed = || "Inline property editor was removed".to_string();
        let number = match self.kind {
            Kind::Slider => ui.slider_value(self.owner),
            Kind::Zoom => ui.zoom_value(self.owner),
            Kind::Rating => match ui.rating_value(self.owner).ok_or_else(removed)? {
                Some(n) => Some(n),
                None => return Ok(CellValue::Null),
            },
            Kind::Progress => ui.progress_value(self.owner),
            Kind::Range => {
                let (l, u) = ui.range_slider_values(self.owner).ok_or_else(removed)?;
                return Ok(CellValue::Text(format!("{l}..{u}")));
            }
            Kind::Barcode => unreachable!(),
        }
        .ok_or_else(removed)?;
        Ok(if self.integer {
            // The zoom track follows the pointer continuously, even on an
            // integer property. The committed value uses the nearest integer.
            CellValue::Integer(number.round() as i64)
        } else {
            CellValue::Real(number)
        })
    }
}

struct BarcodeEditor {
    container: WidgetId,
    symbol: WidgetId,
    text: WidgetId,
    integer: bool,
    nullable: bool,
}

impl BarcodeEditor {
    fn parse(&self, raw: &str) -> Result<CellValue, String> {
        if self.integer {
            raw.parse::<i64>()
                .ok()
                .filter(|n| n.to_string() == raw)
                .map(CellValue::Integer)
                .ok_or_else(|| "Integer barcode requires a canonical signed integer".into())
        } else {
            barcode_settings()
                .validate_value(raw)
                .map_err(|e| e.to_string())?;
            Ok(CellValue::Text(raw.to_owned()))
        }
    }

    fn refresh_symbol(&self, ui: &mut Controls, raw: &str) -> Result<(), ControlError> {
        // Hide stale bars while correcting an invalid draft. Skip re-encoding
        // unchanged valid input on successive frames.
        let valid = ui.barcode_value(self.symbol) == Some(raw) || self.parse(raw).is_ok();
        if valid && ui.barcode_value(self.symbol) != Some(raw) {
            ui.set_barcode_value(self.symbol, raw)?;
        }
        show(ui, self.symbol, valid)
    }
}

impl InspectorEditor for BarcodeEditor {
    fn owner(&self) -> WidgetId {
        self.container
    }
    fn focus(&self) -> WidgetId {
        self.text
    }
    fn inline(&self) -> bool {
        true
    }

    fn load(&self, ui: &mut Controls, value: &CellValue) -> Result<(), ControlError> {
        let text = barcode_text(value, self.integer, self.nullable)?;
        ui.set_editor_value(self.text, &text)?;
        self.refresh_symbol(ui, &text)
    }

    fn read(&self, ui: &Controls) -> Result<CellValue, String> {
        let raw = ui
            .document(self.text)
            .ok_or("Barcode text editor was removed")?
            .text();
        self.parse(raw)
    }

    fn set_active(&self, ui: &mut Controls, active: bool) -> Result<(), ControlError> {
        show(ui, self.text, active)?;
        if let Some(node) = ui.runtime.tree().node(self.container)
            && node.props.focusable == active
        {
            ui.edit_props(self.container, |p| p.focusable = !active)?;
        }
        Ok(())
    }

    fn update(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        let raw = ui
            .document(self.text)
            .ok_or_else(|| invalid("Barcode editor was removed"))?
            .text();
        if ui.barcode_value(self.symbol) == Some(raw)
            && ui
                .runtime
                .tree()
                .node(self.symbol)
                .is_some_and(|n| n.props.visible)
        {
            return Ok(());
        }
        let raw = raw.to_owned();
        self.refresh_symbol(ui, &raw)
    }
}

pub(super) fn mount(
    ui: &mut Controls,
    host: WidgetId,
    d: &PropertyDescriptor,
    f: &FieldSchema,
    v: &CellValue,
) -> Result<Box<dyn InspectorEditor>, ControlError> {
    validate(d, f, v)?;
    let kind = Kind::parse(&d.metadata.editor).unwrap();
    let title = d.metadata.label.as_deref().unwrap_or(&f.title);
    let style = LayoutStyle {
        width: Length::Percent(1.),
        height: Length::Px(28.),
        ..Default::default()
    };
    if kind == Kind::Barcode {
        let integer = f.kind == FieldType::Integer;
        let raw = barcode_text(v, integer, f.nullable)?;
        let container = ui.transparent_panel(host, vertical_stack())?;
        ui.edit_props(container, |p| p.focusable = true)?;
        let symbol = ui.barcode(
            container,
            if raw.is_empty() { "0" } else { &raw },
            barcode_settings(),
            style.clone(),
        )?;
        let text = ui.edit_box(container, title, raw.as_ref(), style)?;
        show(ui, text, false)?;
        show(ui, symbol, !raw.is_empty())?;
        ui.set_automation_name(container, title)?;
        return Ok(Box::new(BarcodeEditor {
            container,
            symbol,
            text,
            integer,
            nullable: f.nullable,
        }));
    }
    let limits = Limits::from_descriptor(d, kind)?;
    let minimum = limits.minimum as f64;
    let maximum = limits.maximum as f64;
    let step = limits.step as f64;
    let integer = f.kind == FieldType::Integer;
    let (owner, focus) = if kind == Kind::Range {
        let (lower, upper) = limits.pair(v)?;
        let range = ui.range_slider(
            host,
            RangeSliderSettings {
                minimum,
                maximum,
                lower,
                upper,
                step,
                ..Default::default()
            },
            style,
        )?;
        let (lower_thumb, upper_thumb) = ui.range_slider_thumbs(range).unwrap();
        ui.set_automation_name(lower_thumb, format!("{title} — нижняя граница"))?;
        ui.set_automation_name(upper_thumb, format!("{title} — верхняя граница"))?;
        (range, lower_thumb)
    } else {
        let value = limits.scalar(v, integer)?;
        let id = match kind {
            Kind::Slider => ui.slider(
                host,
                SliderSettings {
                    minimum,
                    maximum,
                    value,
                    step,
                    ..Default::default()
                },
                style,
            )?,
            Kind::Zoom => ui.zoom_control(
                host,
                ZoomControlSettings {
                    minimum,
                    maximum,
                    value,
                    step,
                },
                style,
            )?,
            Kind::Rating => ui.rating(
                host,
                title,
                RatingSettings {
                    maximum: limits.maximum as u8,
                    value: (*v != CellValue::Null).then_some(value),
                    allow_empty: f.nullable,
                    read_only: d.metadata.read_only || f.read_only,
                    glyph_size: 18.,
                    spacing: 3.,
                    ..Default::default()
                },
                style,
            )?,
            Kind::Progress => ui.progress_bar(
                host,
                ProgressBarSettings {
                    minimum,
                    maximum,
                    value,
                },
                style,
            )?,
            _ => unreachable!(),
        };
        (id, id)
    };
    ui.set_automation_name(owner, title)?;
    Ok(Box::new(NumericEditor {
        kind,
        owner,
        focus,
        limits,
        integer,
        nullable: f.nullable,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_core::Modifiers;

    fn mount_editor(
        ui: &mut Controls,
        name: &str,
        kind: FieldType,
        value: CellValue,
    ) -> Box<dyn InspectorEditor> {
        let field = FieldSchema::new("setting", kind);
        let mut d = PropertyDescriptor::new("setting");
        d.metadata.editor = name.into();
        mount(ui, ui.root(), &d, &field, &value).unwrap()
    }

    fn key(ui: &mut Controls, value: Key) {
        ui.dispatch(InputEvent::KeyDown {
            key: value,
            modifiers: Modifiers::default(),
            repeat: false,
        })
        .unwrap();
    }

    #[test]
    fn slider_zoom_rating_and_both_range_thumbs_accept_real_key_events() {
        for name in ["Slider", "ZoomSlider", "Rating"] {
            let mut ui = Controls::new(Theme::FluentLight).unwrap();
            let editor = mount_editor(&mut ui, name, FieldType::Integer, CellValue::Integer(2));
            ui.update(Size::new(560., 180.)).unwrap();
            ui.runtime
                .apply(Mutation::Focus(Some(editor.focus())))
                .unwrap();
            key(&mut ui, Key::ArrowRight);
            if name == "Rating" {
                assert_eq!(editor.read(&ui).unwrap(), CellValue::Integer(2));
                key(&mut ui, Key::Space);
            }
            assert_eq!(editor.read(&ui).unwrap(), CellValue::Integer(3), "{name}");
            assert!(editor.load(&mut ui, &CellValue::Integer(200)).is_err());
            assert_eq!(editor.read(&ui).unwrap(), CellValue::Integer(3));
        }
        let mut ui = Controls::new(Theme::FluentDark).unwrap();
        let editor = mount_editor(
            &mut ui,
            "RangeSlider",
            FieldType::Text,
            CellValue::Text("10..70".into()),
        );
        ui.update(Size::new(560., 180.)).unwrap();
        let (lower, upper) = ui.range_slider_thumbs(editor.owner()).unwrap();
        for (thumb, value) in [(lower, "11..70"), (upper, "11..71")] {
            ui.runtime.apply(Mutation::Focus(Some(thumb))).unwrap();
            key(&mut ui, Key::ArrowRight);
            assert_eq!(editor.read(&ui).unwrap(), CellValue::Text(value.into()));
        }
        for wrong in [
            "-1..20", "0..101", "45..21", "1.5..2", "1..", "02..3", "1..+2",
        ] {
            assert!(
                editor
                    .load(&mut ui, &CellValue::Text(wrong.into()))
                    .is_err(),
                "{wrong}"
            );
        }
        assert_eq!(editor.read(&ui).unwrap(), CellValue::Text("11..71".into()));
    }

    #[test]
    fn property_grid_escape_cancels_captured_range_drag_before_property_draft() {
        use rust_desktop_ui_data::{CollectionAdapter, DataRow, DataSource, DataTable};
        use std::{cell::RefCell, rc::Rc};

        let viewport = Size::new(760., 350.);
        let initial = CellValue::Text("10..70".into());
        let table = DataTable::new(
            TableSchema::new(vec![FieldSchema::new("range", FieldType::Text)]).unwrap(),
            vec![DataRow::new(1, vec![initial.clone()])],
        )
        .unwrap();
        let source = Rc::new(RefCell::new(
            DataSource::new(CollectionAdapter::new(table)).unwrap(),
        ));
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let root = ui.root();
        let mut descriptor = PropertyDescriptor::new("range");
        descriptor.metadata.editor = "RangeSlider".into();
        let mut grid = PropertyGrid::mount(
            &mut ui,
            root,
            source.clone(),
            &[1],
            PropertyGridSettings {
                properties: vec![descriptor],
                ..Default::default()
            },
            &DataFormFactories::default(),
            LayoutStyle {
                width: Length::Px(720.),
                ..Default::default()
            },
        )
        .unwrap();
        ui.update(viewport).unwrap();
        let slider = grid.property("range").unwrap();
        let lower = ui.range_slider_thumbs(slider).unwrap().0;
        let locate = |ui: &Controls, ratio: f32| {
            let track = ui.runtime.bounds(slider).unwrap();
            let diameter = ui.runtime.bounds(lower).unwrap().width;
            Point::new(
                track.x + diameter * 0.5 + (track.width - diameter) * ratio,
                track.y + track.height * 0.5,
            )
        };
        let route = |grid: &mut PropertyGrid, ui: &mut Controls, event: InputEvent| {
            let consumed = grid.input(ui, &event).unwrap();
            if !consumed {
                ui.dispatch(event).unwrap();
            }
            grid.update(ui).unwrap();
            ui.update(viewport).unwrap();
            consumed
        };
        let down = |position| InputEvent::PointerDown {
            position,
            button: rust_desktop_ui_core::PointerButton::Primary,
            modifiers: Modifiers::default(),
        };
        let escape = InputEvent::KeyDown {
            key: Key::Escape,
            modifiers: Modifiers::default(),
            repeat: false,
        };
        let initial_point = locate(&ui, 0.4);
        route(&mut grid, &mut ui, down(initial_point));
        assert_eq!(ui.runtime.captured(), Some(slider));
        let ten_units = locate(&ui, 0.1).x - locate(&ui, 0.).x;
        let release = Point::new(initial_point.x + ten_units, initial_point.y);
        route(
            &mut grid,
            &mut ui,
            InputEvent::PointerUp {
                position: release,
                button: rust_desktop_ui_core::PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
        );
        assert_eq!(ui.runtime.captured(), None);
        let prepared = CellValue::Text("20..80".into());
        assert_eq!(grid.draft(&ui, "range").unwrap().as_ref(), Some(&prepared));

        let next = locate(&ui, 0.5);
        route(&mut grid, &mut ui, down(next));
        route(
            &mut grid,
            &mut ui,
            InputEvent::PointerMoved {
                position: Point::new(next.x + ten_units, next.y),
            },
        );
        assert_eq!(ui.range_slider_values(slider), Some((30., 90.)));
        assert!(!route(&mut grid, &mut ui, escape.clone()));
        assert_eq!(ui.runtime.captured(), None);
        assert_eq!(grid.draft(&ui, "range").unwrap().as_ref(), Some(&prepared));
        assert!(route(&mut grid, &mut ui, escape));
        assert_eq!(grid.draft(&ui, "range").unwrap().as_ref(), Some(&initial));
        assert_eq!(source.borrow().value(1, "range"), Some(&initial));
        assert!(grid.take_changes().is_empty());
    }

    #[test]
    fn barcode_shows_real_encoded_geometry_and_never_shows_a_stale_symbol_for_an_invalid_draft() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let mut editor = mount_editor(
            &mut ui,
            "Barcode",
            FieldType::Text,
            CellValue::Text("DOC1".into()),
        );
        ui.update(Size::new(560., 180.)).unwrap();
        let symbol = ui
            .runtime
            .tree()
            .live_ids()
            .into_iter()
            .find(|&id| ui.barcode_settings(id).is_some())
            .unwrap();
        assert_eq!(
            ui.barcode_display_status(symbol),
            Some(BarcodeDisplayStatus::Ready)
        );
        let first = ui.barcode_geometry(symbol).unwrap().bars().to_vec();
        assert!(
            !ui.runtime
                .tree()
                .node(editor.focus())
                .unwrap()
                .props
                .visible
        );
        editor.set_active(&mut ui, true).unwrap();
        ui.set_text(editor.focus(), "DOC2").unwrap();
        editor.update(&mut ui).unwrap();
        assert_ne!(
            ui.barcode_geometry(symbol).unwrap().bars(),
            first.as_slice()
        );
        assert_eq!(editor.read(&ui).unwrap(), CellValue::Text("DOC2".into()));
        ui.set_text(editor.focus(), "Ш").unwrap();
        editor.update(&mut ui).unwrap();
        assert!(editor.read(&ui).is_err());
        assert!(!ui.runtime.tree().node(symbol).unwrap().props.visible);
        editor
            .load(&mut ui, &CellValue::Text("DOC1".into()))
            .unwrap();
        assert!(ui.runtime.tree().node(symbol).unwrap().props.visible);
        assert_eq!(
            ui.barcode_geometry(symbol).unwrap().bars(),
            first.as_slice()
        );
    }

    #[test]
    fn invalid_numeric_domains_and_unsupported_types_fail_before_mount() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let root = ui.root();
        let nodes = ui.runtime.tree().len();
        let mut d = PropertyDescriptor::new("score");
        d.metadata.editor = "ProgressBar".into();
        let field = FieldSchema::new("score", FieldType::Real);
        let result = mount(&mut ui, root, &d, &field, &CellValue::Text("90".into()));
        assert!(result.is_err());
        d.maximum = Some(i64::MAX);
        assert!(mount(&mut ui, root, &d, &field, &CellValue::Real(30.)).is_err());
        assert_eq!(ui.runtime.tree().len(), nodes);
        d.maximum = Some(200);
        let e = mount(&mut ui, root, &d, &field, &CellValue::Real(125.25)).unwrap();
        assert_eq!(e.read(&ui).unwrap(), CellValue::Real(125.25));
        assert_eq!(ui.progress_value(e.owner()), Some(125.25));
    }

    #[test]
    fn numeric_barcode_preserves_the_full_integer_without_a_float_conversion() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let mut editor = mount_editor(
            &mut ui,
            "Barcode",
            FieldType::Integer,
            CellValue::Integer(i64::MAX),
        );
        assert_eq!(editor.read(&ui).unwrap(), CellValue::Integer(i64::MAX));
        ui.set_text(editor.focus(), "-1842").unwrap();
        editor.update(&mut ui).unwrap();
        assert_eq!(editor.read(&ui).unwrap(), CellValue::Integer(-1842));
        ui.set_text(editor.focus(), "002").unwrap();
        assert!(editor.read(&ui).is_err());
    }
}
