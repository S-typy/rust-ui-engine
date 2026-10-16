//! Status surfaces and numeric indicators in the compiled form profile.
use crate::{BindingMode, ElementDefinition, ElementKind, MAX_ELEMENT_DEPTH, MAX_ELEMENTS};

/// A status surface containing explicit left- or right-aligned sections.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StatusBarDefinition {
    pub items: Vec<ElementDefinition>,
}

/// Placement within a status surface. Source order is preserved within a side.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StatusBarPlacement {
    #[default]
    Left,
    Right,
}

/// One section of a StatusBar or RibbonStatusBar, with optional content.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StatusBarItemDefinition {
    pub placement: StatusBarPlacement,
    pub child: Option<Box<ElementDefinition>>,
}

/// A determinate, non-editable progress indicator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProgressBarDefinition {
    pub is_indeterminate: bool,
    pub reduced_motion: bool,
    pub minimum: f64,
    pub maximum: f64,
    pub value: f64,
}

impl Default for ProgressBarDefinition {
    fn default() -> Self {
        Self {
            is_indeterminate: false,
            reduced_motion: false,
            minimum: 0.0,
            maximum: 100.0,
            value: 0.0,
        }
    }
}

/// An interactive percentage selector with decrement/increment actions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomControlDefinition {
    pub minimum: f64,
    pub maximum: f64,
    pub value: f64,
    pub step: f64,
}

impl Default for ZoomControlDefinition {
    fn default() -> Self {
        Self {
            minimum: 25.0,
            maximum: 400.0,
            value: 100.0,
            step: 10.0,
        }
    }
}

fn validate_range(minimum: f64, maximum: f64, value: f64, name: &str) -> Result<(), String> {
    if !minimum.is_finite()
        || !maximum.is_finite()
        || !value.is_finite()
        || minimum >= maximum
        || !(maximum - minimum).is_finite()
        || !(minimum..=maximum).contains(&value)
    {
        return Err(format!(
            "{name} requires finite Minimum < Maximum, finite range and Minimum <= Value <= Maximum"
        ));
    }
    Ok(())
}

/// Validate a manually constructed determinate progress indicator.
pub fn validate_progress_bar(definition: &ProgressBarDefinition) -> Result<(), String> {
    validate_range(
        definition.minimum,
        definition.maximum,
        definition.value,
        "ProgressBar",
    )
}

/// Validate a manually constructed zoom selector.
pub fn validate_zoom_control(definition: &ZoomControlDefinition) -> Result<(), String> {
    validate_range(
        definition.minimum,
        definition.maximum,
        definition.value,
        "ZoomControl",
    )?;
    if !definition.step.is_finite() || definition.step <= 0.0 {
        return Err("ZoomControl Step must be finite and positive".into());
    }
    Ok(())
}

/// Validate status structure and numeric settings throughout an element tree.
///
/// StatusBarItem is valid only directly inside a StatusBar or RibbonStatusBar.
/// Each bar accepts only items, and another bar cannot be nested in its content.
/// Progress Value accepts OneWay/OneTime bindings only. This also checks nodes
/// in inactive group pages and docking panes. Other control settings, shared
/// names and the full metadata budget still require the normal host validation.
pub fn validate_status_bars(root: &ElementDefinition) -> Result<(), String> {
    let mut pending = vec![(root, false, false, 1)];
    let mut count = 0;
    while let Some((element, parent_is_bar, inside_bar, depth)) = pending.pop() {
        count += 1;
        if count > MAX_ELEMENTS || depth > MAX_ELEMENT_DEPTH {
            return Err("status content exceeds the element count or depth limit".into());
        }
        let is_bar = matches!(
            element.kind,
            ElementKind::StatusBar(_) | ElementKind::RibbonStatusBar(_)
        );
        let is_item = matches!(element.kind, ElementKind::StatusBarItem(_));
        if parent_is_bar != is_item {
            return Err("StatusBar and RibbonStatusBar accept only direct StatusBarItem children; StatusBarItem requires a status bar parent".into());
        }
        if is_bar && inside_bar {
            return Err("status bars cannot be nested inside another status bar".into());
        }
        match &element.kind {
            ElementKind::ProgressBar(progress) | ElementKind::BusyIndicator(progress) => {
                validate_progress_bar(progress)?;
                if element.common.bindings.get("Value").is_some_and(|binding| {
                    !matches!(binding.mode, BindingMode::OneWay | BindingMode::OneTime)
                }) {
                    return Err("ProgressBar Value supports only OneWay or OneTime binding".into());
                }
            }
            ElementKind::ZoomControl(zoom) => validate_zoom_control(zoom)?,
            ElementKind::DockWorkspace(workspace) => crate::validate_docking(workspace)?,
            _ => {}
        }
        let children = element.children();
        if children.len() > MAX_ELEMENTS.saturating_sub(count) {
            return Err("status content exceeds the element count limit".into());
        }
        pending.extend(
            children
                .into_iter()
                .map(|child| (child, is_bar, inside_bar || is_bar, depth + 1)),
        );
    }
    Ok(())
}
