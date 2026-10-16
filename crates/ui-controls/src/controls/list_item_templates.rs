//! Bounded, noninteractive data presentation for materialized list rows.
use super::*;

/// Values projected from the current stable-key item. A missing column is empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListItemTextSource {
    Text,
    Key,
    Column(usize),
    Literal(String),
}
impl ListItemTextSource {
    pub(super) fn text<'a>(&'a self, item: &'a SelectionItem) -> &'a str {
        match self {
            Self::Text => &item.text,
            Self::Key => &item.key,
            Self::Column(index) => item.columns.get(*index).map_or("", String::as_str),
            Self::Literal(value) => value,
        }
    }
}
/// A declarative readonly node. Width/height and spacing are logical pixels;
/// the owning list's explicit measured row height remains authoritative.
#[derive(Clone, Debug, PartialEq)]
pub struct ListTemplateNode {
    pub layout: LayoutStyle,
    pub kind: ListTemplateKind,
}
#[derive(Clone, Debug, PartialEq)]
pub enum ListTemplateKind {
    Stack {
        orientation: Axis,
        children: Vec<ListTemplateNode>,
    },
    Text {
        source: ListItemTextSource,
        style: TextStyle,
    },
    Icon {
        symbol: IconSymbol,
        style: IconStyle,
    },
    Separator(SeparatorSettings),
}
#[derive(Clone, Debug, PartialEq)]
pub struct ListItemTemplate {
    pub root: ListTemplateNode,
}
impl ListItemTemplate {
    /// At most 32 nodes and 8 levels, with no commands, editors or focus targets.
    pub fn validate(&self) -> Result<(), ControlError> {
        let mut count = 0;
        validate_node(&self.root, 1, &mut count)
    }
}
fn validate_node(
    node: &ListTemplateNode,
    depth: usize,
    count: &mut usize,
) -> Result<(), ControlError> {
    *count += 1;
    if *count > 32 || depth > 8 {
        return Err(ControlError::Invalid(
            "List template exceeds 32 nodes or 8 levels".into(),
        ));
    }
    node.layout
        .validate()
        .map_err(|e| ControlError::Invalid(e.into()))?;
    for length in [node.layout.width, node.layout.height] {
        if matches!(length,Length::Px(value) if value>4096.0)
            || matches!(length,Length::Percent(value) if value>1.0)
        {
            return Err(ControlError::Invalid(
                "List template lengths exceed 4096 pixels or 100 percent".into(),
            ));
        }
    }
    if node.layout.padding.horizontal() > 4096.0
        || node.layout.padding.vertical() > 4096.0
        || node.layout.gap > 4096.0
        || node.layout.min_size.width > 4096.0
        || node.layout.min_size.height > 4096.0
    {
        return Err(ControlError::Invalid(
            "List template spacing or minimum exceeds 4096 pixels".into(),
        ));
    }
    match &node.kind {
        ListTemplateKind::Stack { children, .. } => {
            for child in children {
                validate_node(child, depth + 1, count)?;
            }
        }
        ListTemplateKind::Text { source, style } => {
            style.validate()?;
            if matches!(source,ListItemTextSource::Literal(text) if text.len()>4096) {
                return Err(ControlError::Invalid(
                    "List template literal exceeds 4096 UTF-8 bytes".into(),
                ));
            }
        }
        ListTemplateKind::Icon { style, .. } => style.validate()?,
        ListTemplateKind::Separator(settings) => settings.validate()?,
    }
    Ok(())
}
#[derive(Clone)]
pub(super) struct TemplateText {
    pub widget: WidgetId,
    pub source: ListItemTextSource,
}
pub(super) struct MountedTemplate {
    pub root: WidgetId,
    pub texts: Vec<TemplateText>,
}
impl Controls {
    pub(super) fn preflight_item_template(
        &mut self,
        template: &ListItemTemplate,
        item: &SelectionItem,
        width: f32,
    ) -> Result<(), ControlError> {
        fn visit(
            ui: &mut Controls,
            node: &ListTemplateNode,
            item: &SelectionItem,
            width: f32,
        ) -> Result<(), ControlError> {
            match &node.kind {
                ListTemplateKind::Stack { children, .. } => {
                    for child in children {
                        visit(ui, child, item, width)?;
                    }
                }
                ListTemplateKind::Text { source, style } => {
                    ui.measure_styled_text(source.text(item), width.clamp(0.0, 4096.0), *style)?;
                }
                _ => {}
            }
            Ok(())
        }
        template.validate()?;
        visit(self, &template.root, item, width)
    }
    pub(super) fn create_list_item_template(
        &mut self,
        parent: WidgetId,
        template: &ListItemTemplate,
        item: &SelectionItem,
    ) -> Result<MountedTemplate, ControlError> {
        fn node(
            ui: &mut Controls,
            parent: WidgetId,
            template: &ListTemplateNode,
            item: &SelectionItem,
            texts: &mut Vec<TemplateText>,
        ) -> Result<WidgetId, ControlError> {
            let mut layout = template.layout.clone();
            let widget = match &template.kind {
                ListTemplateKind::Stack { orientation, .. } => {
                    layout.kind = LayoutKind::Stack(*orientation);
                    ui.transparent_panel(parent, layout)?
                }
                ListTemplateKind::Text { source, style } => {
                    let id = ui.label(parent, source.text(item), layout)?;
                    ui.set_text_style(id, *style)?;
                    texts.push(TemplateText {
                        widget: id,
                        source: source.clone(),
                    });
                    id
                }
                ListTemplateKind::Icon { symbol, style } => {
                    let id = ui.icon(parent, *symbol, layout)?;
                    ui.set_icon_style(id, *style)?;
                    id
                }
                ListTemplateKind::Separator(settings) => ui.separator(parent, *settings, layout)?,
            };
            ui.edit_props(widget, |p| {
                p.focusable = false;
                p.tab_stop = false;
                p.hit_test = false;
            })?;
            if let ListTemplateKind::Stack { children, .. } = &template.kind {
                for child in children {
                    node(ui, widget, child, item, texts)?;
                }
            }
            Ok(widget)
        }
        let host = self.transparent_panel(
            parent,
            LayoutStyle {
                kind: LayoutKind::Overlay,
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                ..Default::default()
            },
        )?;
        self.edit_props(host, |p| {
            p.hit_test = false;
            p.clip = true;
        })?;
        let mut texts = Vec::new();
        if let Err(error) = node(self, host, &template.root, item, &mut texts) {
            self.remove(host)?;
            return Err(error);
        }
        Ok(MountedTemplate { root: host, texts })
    }
}
