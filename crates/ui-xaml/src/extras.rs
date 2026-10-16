use crate::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolBarDefinition {
    pub key: String,
    pub children: Vec<ElementDefinition>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MenuBarDefinition {
    pub items: Vec<ElementDefinition>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PopupPlacementDefinition {
    #[default]
    Below,
    Beside,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PopupDefinition {
    pub placement_target: String,
    pub placement: PopupPlacementDefinition,
    pub is_open: bool,
    pub popup_width: f32,
    pub popup_height: f32,
    pub child: Option<Box<ElementDefinition>>,
}
impl Default for PopupDefinition {
    fn default() -> Self {
        Self {
            placement_target: String::new(),
            placement: PopupPlacementDefinition::Below,
            is_open: false,
            popup_width: 240.0,
            popup_height: 180.0,
            child: None,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichToolTipDefinition {
    pub header: String,
    pub icon: Option<IconSymbol>,
    pub image: Option<String>,
    pub image_description: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SplitPaneDefinition {
    #[default]
    None,
    First,
    Second,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SplitContainerDefinition {
    pub orientation: Orientation,
    pub ratio: f32,
    pub first_min_size: f32,
    pub second_min_size: f32,
    pub first_max_size: Option<f32>,
    pub second_max_size: Option<f32>,
    pub divider_width: f32,
    pub collapsed_pane: SplitPaneDefinition,
    pub children: Vec<ElementDefinition>,
}
impl Default for SplitContainerDefinition {
    fn default() -> Self {
        Self {
            orientation: Orientation::Horizontal,
            ratio: 0.5,
            first_min_size: 0.0,
            second_min_size: 0.0,
            first_max_size: None,
            second_max_size: None,
            divider_width: 6.0,
            collapsed_pane: SplitPaneDefinition::None,
            children: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RangeCrossingPolicyDefinition {
    #[default]
    Clamp,
    Swap,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SliderDefinition {
    pub minimum: f64,
    pub maximum: f64,
    pub value: f64,
    pub step: f64,
    pub orientation: Orientation,
    pub tick_frequency: f64,
    pub snap_to_ticks: bool,
    pub is_reversed: bool,
}
impl Default for SliderDefinition {
    fn default() -> Self {
        Self {
            minimum: 0.0,
            maximum: 100.0,
            value: 0.0,
            step: 1.0,
            orientation: Orientation::Horizontal,
            tick_frequency: 10.0,
            snap_to_ticks: false,
            is_reversed: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct RangeSliderDefinition {
    pub slider: SliderDefinition,
    pub lower_value: f64,
    pub upper_value: f64,
    pub crossing_policy: RangeCrossingPolicyDefinition,
}
impl Default for RangeSliderDefinition {
    fn default() -> Self {
        Self {
            slider: SliderDefinition::default(),
            lower_value: 0.0,
            upper_value: 100.0,
            crossing_policy: RangeCrossingPolicyDefinition::Clamp,
        }
    }
}

pub(crate) fn new_kind(name: &str) -> Option<ElementKind> {
    Some(match name {
        "ToolBar" => ElementKind::ToolBar(ToolBarDefinition::default()),
        "MenuBar" => ElementKind::MenuBar(MenuBarDefinition::default()),
        "Popup" => ElementKind::Popup(PopupDefinition::default()),
        "SplitContainer" => ElementKind::SplitContainer(SplitContainerDefinition::default()),
        "Slider" => ElementKind::Slider(SliderDefinition::default()),
        "RangeSlider" => ElementKind::RangeSlider(RangeSliderDefinition::default()),
        "Overlay" => ElementKind::Overlay(StackPanelDefinition::default()),
        _ => return None,
    })
}
pub(crate) fn property(
    doc: &Document<'_>,
    attr: Attribute<'_, '_>,
    kind: &mut ElementKind,
) -> Result<bool, Diagnostic> {
    let number = || {
        attr.value()
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .ok_or_else(|| at_attribute(doc, attr, "Expected finite number"))
    };
    let metric = || editor_metric(doc, attr, MAX_DIMENSION, true);
    let orientation = || match attr.value() {
        "Horizontal" => Ok(Orientation::Horizontal),
        "Vertical" => Ok(Orientation::Vertical),
        _ => Err(at_attribute(
            doc,
            attr,
            "Orientation must be Horizontal or Vertical",
        )),
    };
    if let ElementKind::ToolBar(bar) = kind {
        if attr.name() == "Key" && binding::valid_identifier(attr.value()) {
            bar.key = attr.value().into();
            return Ok(true);
        }
        return Ok(false);
    }
    if let ElementKind::Popup(popup) = kind {
        match attr.name() {
            "PlacementTarget" if binding::valid_identifier(attr.value()) => {
                popup.placement_target = attr.value().into()
            }
            "Placement" => {
                popup.placement = match attr.value() {
                    "Below" => PopupPlacementDefinition::Below,
                    "Beside" => PopupPlacementDefinition::Beside,
                    _ => return Err(at_attribute(doc, attr, "Placement must be Below or Beside")),
                }
            }
            "IsOpen" => popup.is_open = boolean(doc, attr)?,
            "PopupWidth" => popup.popup_width = editor_dimension(doc, attr)?,
            "PopupHeight" => popup.popup_height = editor_dimension(doc, attr)?,
            _ => return Ok(false),
        }
        return Ok(true);
    }
    if let ElementKind::SplitContainer(split) = kind {
        match attr.name() {
            "Orientation" => split.orientation = orientation()?,
            "Ratio" => split.ratio = number()? as f32,
            "FirstMinSize" => split.first_min_size = metric()?,
            "SecondMinSize" => split.second_min_size = metric()?,
            "FirstMaxSize" => split.first_max_size = Some(metric()?),
            "SecondMaxSize" => split.second_max_size = Some(metric()?),
            "DividerWidth" => split.divider_width = metric()?,
            "CollapsedPane" => {
                split.collapsed_pane = match attr.value() {
                    "None" => SplitPaneDefinition::None,
                    "First" => SplitPaneDefinition::First,
                    "Second" => SplitPaneDefinition::Second,
                    _ => {
                        return Err(at_attribute(
                            doc,
                            attr,
                            "CollapsedPane must be None, First or Second",
                        ));
                    }
                }
            }
            _ => return Ok(false),
        }
        return Ok(true);
    }
    if let ElementKind::RangeSlider(range) = kind {
        match attr.name() {
            "LowerValue" => {
                range.lower_value = number()?;
                return Ok(true);
            }
            "UpperValue" => {
                range.upper_value = number()?;
                return Ok(true);
            }
            "CrossingPolicy" => {
                range.crossing_policy = match attr.value() {
                    "Clamp" => RangeCrossingPolicyDefinition::Clamp,
                    "Swap" => RangeCrossingPolicyDefinition::Swap,
                    _ => {
                        return Err(at_attribute(
                            doc,
                            attr,
                            "CrossingPolicy must be Clamp or Swap",
                        ));
                    }
                };
                return Ok(true);
            }
            _ => {}
        }
    }
    let slider = match kind {
        ElementKind::Slider(slider) => slider,
        ElementKind::RangeSlider(range) => &mut range.slider,
        _ => return Ok(false),
    };
    match attr.name() {
        "Minimum" => slider.minimum = number()?,
        "Maximum" => slider.maximum = number()?,
        "Value" => slider.value = number()?,
        "Step" => slider.step = number()?,
        "Orientation" => slider.orientation = orientation()?,
        "TickFrequency" => slider.tick_frequency = number()?,
        "SnapToTicks" => slider.snap_to_ticks = boolean(doc, attr)?,
        "IsReversed" => slider.is_reversed = boolean(doc, attr)?,
        _ => return Ok(false),
    }
    Ok(true)
}
pub fn validate_split_container(split: &SplitContainerDefinition) -> Result<(), String> {
    if !split.ratio.is_finite()
        || !(0.0..=1.0).contains(&split.ratio)
        || !split.divider_width.is_finite()
        || !(1.0..=64.0).contains(&split.divider_width)
        || split.children.len() != 2
    {
        return Err(
            "SplitContainer requires two children, Ratio in [0,1], DividerWidth in [1,64]".into(),
        );
    }
    for (min, max) in [
        (split.first_min_size, split.first_max_size),
        (split.second_min_size, split.second_max_size),
    ] {
        if !min.is_finite()
            || !(0.0..=MAX_DIMENSION as f32).contains(&min)
            || max.is_some_and(|max| !max.is_finite() || max < min || max > MAX_DIMENSION as f32)
        {
            return Err(
                "SplitContainer pane constraints must be finite, nonnegative and ordered".into(),
            );
        }
    }
    Ok(())
}
pub fn validate_slider(slider: &SliderDefinition) -> Result<(), String> {
    if [
        slider.minimum,
        slider.maximum,
        slider.value,
        slider.step,
        slider.tick_frequency,
    ]
    .iter()
    .any(|n| !n.is_finite())
        || slider.minimum >= slider.maximum
        || !(slider.maximum - slider.minimum).is_finite()
        || !((slider.maximum - slider.minimum) / slider.tick_frequency).is_finite()
        || !((slider.minimum)..=slider.maximum).contains(&slider.value)
        || slider.step <= 0.0
        || slider.tick_frequency <= 0.0
    {
        return Err(
            "Slider requires finite ordered bounds, bounded Value and positive Step/TickFrequency"
                .into(),
        );
    }
    Ok(())
}
pub fn validate_range_slider(range: &RangeSliderDefinition) -> Result<(), String> {
    validate_slider(&range.slider)?;
    if !range.lower_value.is_finite()
        || !range.upper_value.is_finite()
        || range.lower_value < range.slider.minimum
        || range.upper_value > range.slider.maximum
        || range.lower_value > range.upper_value
    {
        return Err("RangeSlider requires Minimum <= LowerValue <= UpperValue <= Maximum".into());
    }
    Ok(())
}
pub fn validate_additional_element(kind: &ElementKind) -> Result<(), String> {
    match kind {
        ElementKind::ToolBar(bar) => {
            if !binding::valid_identifier(&bar.key)
                || bar.children.is_empty()
                || bar.children.len() > 128
            {
                Err("ToolBar requires Key and 1..128 children".into())
            } else {
                Ok(())
            }
        }
        ElementKind::MenuBar(bar) => {
            if bar.items.is_empty()
                || bar.items.len() > 32
                || bar
                    .items
                    .iter()
                    .any(|item| !matches!(item.kind, ElementKind::DropDownButton(_)))
            {
                Err("MenuBar accepts 1..32 DropDownButton items".into())
            } else {
                Ok(())
            }
        }
        ElementKind::Popup(popup) => {
            if !binding::valid_identifier(&popup.placement_target)
                || popup.child.is_none()
                || [popup.popup_width, popup.popup_height]
                    .iter()
                    .any(|n| !n.is_finite() || *n <= 0.0 || *n > MAX_DIMENSION as f32)
            {
                Err(
                    "Popup requires PlacementTarget, one child and positive finite dimensions"
                        .into(),
                )
            } else {
                Ok(())
            }
        }
        ElementKind::SplitContainer(s) => validate_split_container(s),
        ElementKind::Slider(s) => validate_slider(s),
        ElementKind::RangeSlider(s) => validate_range_slider(s),
        _ => Ok(()),
    }
}

/// Resolves retained popup owners before mounting, including split disclosures.
/// A menu button uses either menu entries or one named retained Popup.
pub fn validate_popup_links(root: &ElementDefinition) -> Result<(), String> {
    use std::collections::{HashMap, HashSet};
    let mut pending = vec![(root, 1usize, None, false)];
    let mut names = HashMap::new();
    let mut popups = Vec::new();
    let mut menus = Vec::new();
    let mut count = 0;
    while let Some((element, depth, owner, in_menu_bar)) = pending.pop() {
        count += 1;
        if count > MAX_ELEMENTS || depth > MAX_ELEMENT_DEPTH {
            return Err("Popup tree exceeds the element count or depth limit".into());
        }
        let current_owner = if matches!(element.kind, ElementKind::Popup(_)) {
            Some(element as *const ElementDefinition as usize)
        } else {
            owner
        };
        if let Some(name) = &element.common.name
            && names.insert(name.as_str(), current_owner).is_some()
        {
            return Err(format!("Duplicate XAML name '{name}'"));
        }
        match &element.kind {
            ElementKind::Popup(popup) => {
                popups.push((current_owner.unwrap(), popup.placement_target.as_str()))
            }
            ElementKind::DropDownButton(menu) | ElementKind::SplitButton(menu) => menus.push((
                element.common.name.as_deref(),
                menu.items.is_empty(),
                in_menu_bar,
            )),
            _ => {}
        }
        pending.extend(element.children().into_iter().map(|child| {
            (
                child,
                depth + 1,
                current_owner,
                matches!(element.kind, ElementKind::MenuBar(_)),
            )
        }));
    }
    let mut targets = HashSet::new();
    let mut owners = HashMap::new();
    for (id, target) in popups {
        let owner = names
            .get(target)
            .ok_or_else(|| format!("Unknown Popup PlacementTarget '{target}'"))?;
        if !targets.insert(target) {
            return Err(format!(
                "PlacementTarget '{target}' has more than one Popup"
            ));
        }
        owners.insert(id, *owner);
    }
    for (name, empty, in_menu_bar) in menus {
        let retained = name.is_some_and(|name| targets.contains(name));
        let invalid = if empty {
            !retained || in_menu_bar
        } else {
            retained
        };
        if invalid {
            return Err("A menu button requires nonempty menu items or exactly one retained Popup; MenuBar requires menu items".into());
        }
    }
    for &id in owners.keys() {
        let mut seen = HashSet::new();
        let mut next = Some(id);
        while let Some(id) = next {
            if !seen.insert(id) {
                return Err("Popup ownership cannot contain a dependency cycle".into());
            }
            next = owners.get(&id).copied().flatten();
        }
    }
    Ok(())
}

pub(crate) fn codegen(kind: &ElementKind) -> Option<String> {
    Some(match kind {
        ElementKind::ToolBar(bar) => format!(
            "rust_desktop_ui_xaml::ElementKind::ToolBar(rust_desktop_ui_xaml::ToolBarDefinition {{ key: {:?}.into(), children: vec![{}] }})",
            bar.key,
            bar.children
                .iter()
                .map(crate::codegen::element)
                .collect::<Vec<_>>()
                .join(",")
        ),
        ElementKind::MenuBar(bar) => format!(
            "rust_desktop_ui_xaml::ElementKind::MenuBar(rust_desktop_ui_xaml::MenuBarDefinition {{ items: vec![{}] }})",
            bar.items
                .iter()
                .map(crate::codegen::element)
                .collect::<Vec<_>>()
                .join(",")
        ),
        ElementKind::Popup(popup) => format!(
            "rust_desktop_ui_xaml::ElementKind::Popup(rust_desktop_ui_xaml::PopupDefinition {{ placement_target: {:?}.into(), placement: rust_desktop_ui_xaml::PopupPlacementDefinition::{:?}, is_open: {}, popup_width: {:?}, popup_height: {:?}, child: {} }})",
            popup.placement_target,
            popup.placement,
            popup.is_open,
            popup.popup_width,
            popup.popup_height,
            popup.child.as_ref().map_or("None".into(), |child| format!(
                "Some(Box::new({}))",
                crate::codegen::element(child)
            ))
        ),
        ElementKind::SplitContainer(s) => format!(
            "rust_desktop_ui_xaml::ElementKind::SplitContainer(rust_desktop_ui_xaml::SplitContainerDefinition {{ orientation: rust_desktop_ui_xaml::Orientation::{:?}, ratio: {:?}, first_min_size: {:?}, second_min_size: {:?}, first_max_size: {:?}, second_max_size: {:?}, divider_width: {:?}, collapsed_pane: rust_desktop_ui_xaml::SplitPaneDefinition::{:?}, children: vec![{}] }})",
            s.orientation,
            s.ratio,
            s.first_min_size,
            s.second_min_size,
            s.first_max_size,
            s.second_max_size,
            s.divider_width,
            s.collapsed_pane,
            s.children
                .iter()
                .map(crate::codegen::element)
                .collect::<Vec<_>>()
                .join(",")
        ),
        ElementKind::Slider(s) => format!(
            "rust_desktop_ui_xaml::ElementKind::Slider({})",
            slider_code(s)
        ),
        ElementKind::RangeSlider(s) => format!(
            "rust_desktop_ui_xaml::ElementKind::RangeSlider(rust_desktop_ui_xaml::RangeSliderDefinition {{ slider: {}, lower_value: {:?}, upper_value: {:?}, crossing_policy: rust_desktop_ui_xaml::RangeCrossingPolicyDefinition::{:?} }})",
            slider_code(&s.slider),
            s.lower_value,
            s.upper_value,
            s.crossing_policy
        ),
        _ => return None,
    })
}
fn slider_code(s: &SliderDefinition) -> String {
    format!(
        "rust_desktop_ui_xaml::SliderDefinition {{ minimum: {:?}, maximum: {:?}, value: {:?}, step: {:?}, orientation: rust_desktop_ui_xaml::Orientation::{:?}, tick_frequency: {:?}, snap_to_ticks: {}, is_reversed: {} }}",
        s.minimum,
        s.maximum,
        s.value,
        s.step,
        s.orientation,
        s.tick_frequency,
        s.snap_to_ticks,
        s.is_reversed
    )
}
