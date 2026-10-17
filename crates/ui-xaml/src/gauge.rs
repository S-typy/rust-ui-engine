//! Read-only measurement controls and bounded scale intervals.
use crate::*;
use unicode_segmentation::UnicodeSegmentation;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GaugeViewDefinition {
    #[default]
    Circular,
    Linear,
    Digital,
    State,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GaugeToneDefinition {
    #[default]
    Neutral,
    Accent,
    Good,
    Warning,
    Danger,
}
#[derive(Clone, Debug, PartialEq)]
pub struct GaugeRangeDefinition {
    pub start: f64,
    pub end: f64,
    pub label: String,
    pub tone: GaugeToneDefinition,
}
#[derive(Clone, Debug, PartialEq)]
pub struct GaugeDefinition {
    pub view: GaugeViewDefinition,
    pub minimum: f64,
    pub maximum: f64,
    pub value: f64,
    pub major_ticks: u8,
    pub minor_ticks: u8,
    pub precision: u8,
    pub unit: String,
    pub ranges: Vec<GaugeRangeDefinition>,
}
impl Default for GaugeDefinition {
    fn default() -> Self {
        Self {
            view: Default::default(),
            minimum: 0.0,
            maximum: 100.0,
            value: 0.0,
            major_ticks: 6,
            minor_ticks: 3,
            precision: 1,
            unit: String::new(),
            ranges: Vec::new(),
        }
    }
}
fn bounded(text: &str, bytes: usize, graphemes: usize) -> bool {
    text.len() <= bytes
        && text.graphemes(true).count() <= graphemes
        && !text.chars().any(char::is_control)
}
pub fn validate_gauge(value: &GaugeDefinition, common: &CommonProperties) -> Result<(), String> {
    if !value.minimum.is_finite()
        || !value.maximum.is_finite()
        || value.minimum >= value.maximum
        || !(value.maximum - value.minimum).is_finite()
        || !(2..=21).contains(&value.major_ticks)
        || value.minor_ticks > 4
        || value.precision > 6
        || !bounded(&value.unit, 128, 32)
        || value.ranges.len() > 16
    {
        return Err("Invalid Gauge scale, ticks, unit or range budget".into());
    }
    let check = |v: f64| {
        if v.is_finite() && (value.minimum..=value.maximum).contains(&v) {
            Ok(())
        } else {
            Err("Gauge value must be finite and inside Minimum..Maximum".to_owned())
        }
    };
    if !common.bindings.contains_key("Value") {
        check(value.value)?;
    }
    let mut previous = value.minimum;
    for r in &value.ranges {
        if !r.start.is_finite()
            || !r.end.is_finite()
            || r.start < previous
            || r.start < value.minimum
            || r.start >= r.end
            || r.end > value.maximum
            || r.label.is_empty()
            || !bounded(&r.label, 256, 64)
        {
            return Err(
                "Gauge ranges require sorted nonoverlapping intervals and bounded labels".into(),
            );
        }
        previous = r.end;
    }
    for (property, binding) in &common.bindings {
        if !badges::binding_property(property)
            && !matches!(
                property.as_str(),
                "Value" | "IsEnabled" | "Visibility" | "Width" | "Height"
            )
        {
            return Err("Gauge supports Value and common bindings".into());
        }
        if property == "Value" {
            if !matches!(binding.mode, BindingMode::OneWay | BindingMode::OneTime) {
                return Err("Gauge Value supports OneWay or OneTime".into());
            }
            if let Some(fallback) = &binding.fallback_value {
                check(
                    fallback
                        .parse()
                        .map_err(|_| "Gauge fallback requires a number")?,
                )?;
            }
        }
    }
    Ok(())
}
pub(crate) fn property(
    document: &Document<'_>,
    attr: Attribute<'_, '_>,
    value: &mut GaugeDefinition,
) -> Result<bool, Diagnostic> {
    match attr.name() {
        "Minimum" => value.minimum = number(document, attr)?,
        "Maximum" => value.maximum = number(document, attr)?,
        "Value" => value.value = number(document, attr)?,
        "MajorTicks" => {
            value.major_ticks = attr
                .value()
                .parse()
                .map_err(|_| at_attribute(document, attr, "MajorTicks requires 2..21"))?
        }
        "MinorTicks" => {
            value.minor_ticks = attr
                .value()
                .parse()
                .map_err(|_| at_attribute(document, attr, "MinorTicks requires 0..4"))?
        }
        "Precision" => {
            value.precision = attr
                .value()
                .parse()
                .map_err(|_| at_attribute(document, attr, "Precision requires 0..6"))?
        }
        "Unit" => value.unit = attr.value().into(),
        _ => return Ok(false),
    }
    Ok(true)
}
fn number(document: &Document<'_>, attr: Attribute<'_, '_>) -> Result<f64, Diagnostic> {
    attr.value()
        .parse()
        .ok()
        .filter(|v: &f64| v.is_finite())
        .ok_or_else(|| at_attribute(document, attr, "Gauge requires a finite number"))
}
pub(crate) fn children(
    node: Node<'_, '_>,
    value: &mut GaugeDefinition,
    depth: usize,
    count: &mut usize,
) -> Result<(), Diagnostic> {
    for child in node.children() {
        if !child.is_element() {
            require_empty_content(child)?;
            continue;
        }
        require_element(child, "GaugeRange")?;
        if depth >= MAX_ELEMENT_DEPTH || value.ranges.len() >= 16 {
            return Err(at_node(child, "Gauge range limit exceeded"));
        }
        let mut start = None;
        let mut end = None;
        let mut label = None;
        let mut tone = GaugeToneDefinition::Neutral;
        for attr in child.attributes() {
            reject_extension(child.document(), attr)?;
            if attr.namespace().is_some() {
                return Err(at_attribute(
                    child.document(),
                    attr,
                    "GaugeRange does not accept directives",
                ));
            }
            match attr.name() {
                "Start" => start = Some(number(child.document(), attr)?),
                "End" => end = Some(number(child.document(), attr)?),
                "Label" => label = Some(attr.value().into()),
                "Tone" => {
                    tone = match attr.value() {
                        "Neutral" => GaugeToneDefinition::Neutral,
                        "Accent" => GaugeToneDefinition::Accent,
                        "Good" => GaugeToneDefinition::Good,
                        "Warning" => GaugeToneDefinition::Warning,
                        "Danger" => GaugeToneDefinition::Danger,
                        _ => {
                            return Err(at_attribute(
                                child.document(),
                                attr,
                                "Unknown GaugeRange Tone",
                            ));
                        }
                    }
                }
                _ => {
                    return Err(at_attribute(
                        child.document(),
                        attr,
                        "GaugeRange supports Start, End, Label and Tone",
                    ));
                }
            }
        }
        for content in child.children() {
            require_empty_content(content)?;
        }
        *count += 1;
        if *count > MAX_ELEMENTS {
            return Err(at_node(child, "Gauge metadata exceeds tree budget"));
        }
        value.ranges.push(GaugeRangeDefinition {
            start: start.ok_or_else(|| at_node(child, "Missing Start"))?,
            end: end.ok_or_else(|| at_node(child, "Missing End"))?,
            label: label.ok_or_else(|| at_node(child, "Missing Label"))?,
            tone,
        });
    }
    Ok(())
}
pub(crate) fn code(v: &GaugeDefinition) -> String {
    let ranges=v.ranges.iter().map(|r|format!("rust_desktop_ui_xaml::GaugeRangeDefinition{{start:{:?},end:{:?},label:{:?}.into(),tone:rust_desktop_ui_xaml::GaugeToneDefinition::{:?}}}",r.start,r.end,r.label,r.tone)).collect::<Vec<_>>().join(",");
    format!(
        "rust_desktop_ui_xaml::GaugeDefinition{{view:rust_desktop_ui_xaml::GaugeViewDefinition::{:?},minimum:{:?},maximum:{:?},value:{:?},major_ticks:{},minor_ticks:{},precision:{},unit:{:?}.into(),ranges:vec![{}]}}",
        v.view,
        v.minimum,
        v.maximum,
        v.value,
        v.major_ticks,
        v.minor_ticks,
        v.precision,
        v.unit,
        ranges
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn window(body: &str) -> Result<WindowDefinition, Diagnostic> {
        parse(&format!(
            "<Window xmlns='urn:rust-ui-engine:ui'>{body}</Window>"
        ))
    }
    #[test]
    fn gauge_four_views_typed_ranges_readonly_binding_and_codegen() {
        for tag in [
            "CircularGauge",
            "LinearGauge",
            "DigitalGauge",
            "StateIndicator",
        ] {
            let w=window(&format!("<{tag} Value='75' Unit='°C'><GaugeRange Start='0' End='100' Label='Ready' Tone='Good'/></{tag}>")).unwrap();
            let ElementKind::Gauge(v) = w.content.kind else {
                panic!()
            };
            assert_eq!(v.value, 75.0);
            assert_eq!(v.ranges.len(), 1);
            assert!(code(&v).contains("GaugeRangeDefinition"));
        }
        let w = window("<CircularGauge Minimum='50' Value='{Binding Temperature}'/>").unwrap();
        assert_eq!(w.content.common.bindings["Value"].mode, BindingMode::OneWay);
    }
    #[test]
    fn gauge_invalid_ranges_numbers_children_and_modes_rejected() {
        for body in [
            "<CircularGauge Value='NaN'/>",
            "<LinearGauge Minimum='1' Maximum='1'/>",
            "<DigitalGauge Value='101'/>",
            "<StateIndicator Precision='7'/>",
            "<CircularGauge MajorTicks='1'/>",
            "<CircularGauge Value='{Binding Value, Mode=TwoWay}'/>",
            "<CircularGauge Value='{Binding Value, FallbackValue=101}'/>",
            "<CircularGauge><GaugeRange Start='0' End='60' Label='A'/><GaugeRange Start='50' End='100' Label='B'/></CircularGauge>",
            "<CircularGauge><GaugeRange Start='0' End='100'/></CircularGauge>",
            "<CircularGauge><Button/></CircularGauge>",
        ] {
            assert!(window(body).is_err(), "{body}");
        }
    }
}
