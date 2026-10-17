//! Retained navigation page declarations with an explicit route registry.
use crate::*;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NavigationViewDefinition {
    #[default]
    Side,
    Accordion,
    Hamburger,
    Office,
    Frame,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NavigationSectionDefinition {
    pub id: String,
    pub header: String,
    pub icon: Option<IconSymbol>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NavigationPageDefinition {
    pub route: String,
    pub section: String,
    pub header: String,
    pub icon: Option<IconSymbol>,
    pub is_enabled: bool,
    pub content: Box<ElementDefinition>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NavigationDefinition {
    pub view: NavigationViewDefinition,
    pub route: String,
    pub pane_width: f32,
    pub history_limit: usize,
    pub show_history: bool,
    pub sections: Vec<NavigationSectionDefinition>,
    pub pages: Vec<NavigationPageDefinition>,
}
impl Default for NavigationDefinition {
    fn default() -> Self {
        Self {
            view: NavigationViewDefinition::Side,
            route: String::new(),
            pane_width: 220.0,
            history_limit: 64,
            show_history: true,
            sections: vec![],
            pages: vec![],
        }
    }
}
pub fn validate_navigation(
    value: &NavigationDefinition,
    common: &CommonProperties,
) -> Result<(), String> {
    let id = |s: &str| {
        !s.is_empty()
            && s.len() <= 128
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/_.-".contains(&b))
    };
    let label = |s: &str| {
        !s.trim().is_empty()
            && s.len() <= 512
            && s.graphemes(true).count() <= 128
            && !s.chars().any(char::is_control)
            && !s.contains(['\u{2028}', '\u{2029}'])
    };
    if !(1..=16).contains(&value.sections.len())
        || !(1..=64).contains(&value.pages.len())
        || !(1..=128).contains(&value.history_limit)
        || !value.pane_width.is_finite()
        || !(120.0..=640.0).contains(&value.pane_width)
    {
        return Err(
            "Navigation requires 1..16 sections, 1..64 pages, history1..128 and pane width120..640"
                .into(),
        );
    }
    let mut sections = HashSet::new();
    let mut routes = HashSet::new();
    let mut bytes: usize = value
        .sections
        .iter()
        .map(|s| s.id.len() + s.header.len())
        .sum();
    for s in &value.sections {
        if !id(&s.id)
            || !label(&s.header)
            || !sections.insert(&s.id)
            || !value.pages.iter().any(|p| p.section == s.id)
        {
            return Err(
                "Navigation sections require unique bounded IDs, nonempty captions and pages"
                    .into(),
            );
        }
    }
    for p in &value.pages {
        if !id(&p.route)
            || !label(&p.header)
            || !sections.contains(&p.section)
            || !routes.insert(&p.route)
        {
            return Err(
                "Navigation pages require unique routes, declared sections and bounded captions"
                    .into(),
            );
        }
        bytes += p.route.len() + p.section.len() + p.header.len();
    }
    if bytes > 32768 {
        return Err("Navigation metadata exceeds 32 KiB".into());
    }
    if !value.route.is_empty()
        && !value
            .pages
            .iter()
            .any(|p| p.route == value.route && p.is_enabled)
    {
        return Err("Initial navigation Route must reference an enabled page".into());
    }
    if !value.pages.iter().any(|p| p.is_enabled) {
        return Err("Navigation requires an initially enabled page".into());
    }
    if common.bindings.keys().any(|p| {
        !badges::binding_property(p)
            && !matches!(
                p.as_str(),
                "Route" | "IsEnabled" | "Visibility" | "Width" | "Height"
            )
    }) {
        return Err("Navigation supports Route and common bindings".into());
    }
    Ok(())
}
pub(crate) fn property(
    document: &Document<'_>,
    attr: Attribute<'_, '_>,
    value: &mut NavigationDefinition,
) -> Result<bool, Diagnostic> {
    match attr.name() {
        "View" => {
            value.view = match attr.value() {
                "Side" => NavigationViewDefinition::Side,
                "Accordion" => NavigationViewDefinition::Accordion,
                "Hamburger" => NavigationViewDefinition::Hamburger,
                "Office" => NavigationViewDefinition::Office,
                "Frame" => NavigationViewDefinition::Frame,
                _ => {
                    return Err(at_attribute(
                        document,
                        attr,
                        "Navigation View must be Side, Accordion, Hamburger, Office or Frame",
                    ));
                }
            }
        }
        "Route" => value.route = attr.value().into(),
        "PaneWidth" => value.pane_width = editor_metric(document, attr, 640.0, false)?,
        "HistoryLimit" => {
            value.history_limit = attr
                .value()
                .parse()
                .map_err(|_| at_attribute(document, attr, "HistoryLimit requires an integer"))?
        }
        "ShowHistory" => value.show_history = boolean(document, attr)?,
        _ => return Ok(false),
    }
    Ok(true)
}
pub(crate) fn parse_children(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
    value: &mut NavigationDefinition,
    names: &mut HashSet<String>,
    checked: &mut HashSet<RadioGroupKey>,
) -> Result<(), Diagnostic> {
    for section in node.children() {
        if !section.is_element() {
            require_empty_content(section)?;
            continue;
        }
        if section.tag_name().namespace() != Some(UI_NAMESPACE)
            || section.tag_name().name() != "NavigationSection"
        {
            return Err(at_node(
                section,
                "NavigationPane accepts NavigationSection children",
            ));
        }
        metadata_limit(section, depth + 1, count)?;
        if value.sections.len() >= 16 {
            return Err(at_node(
                section,
                "At most16 navigation sections are supported",
            ));
        }
        let mut definition = NavigationSectionDefinition {
            id: String::new(),
            header: String::new(),
            icon: None,
        };
        for attr in section.attributes() {
            if attr.namespace().is_some() {
                return Err(at_attribute(
                    node.document(),
                    attr,
                    "Navigation metadata directives are unsupported",
                ));
            }
            reject_extension(node.document(), attr)?;
            match attr.name() {
                "Id" => definition.id = attr.value().into(),
                "Header" => definition.header = attr.value().into(),
                "Icon" => definition.icon = Some(icon_symbol(node.document(), attr)?),
                _ => {
                    return Err(at_attribute(
                        node.document(),
                        attr,
                        "Unsupported NavigationSection attribute",
                    ));
                }
            }
        }
        for page in section.children() {
            if !page.is_element() {
                require_empty_content(page)?;
                continue;
            }
            if page.tag_name().namespace() != Some(UI_NAMESPACE)
                || page.tag_name().name() != "NavigationPage"
            {
                return Err(at_node(
                    page,
                    "NavigationSection accepts NavigationPage children",
                ));
            }
            metadata_limit(page, depth + 2, count)?;
            let parent = *count;
            if value.pages.len() >= 64 {
                return Err(at_node(page, "At most64 navigation pages are supported"));
            }
            let mut item = NavigationPageDefinition {
                route: String::new(),
                section: definition.id.clone(),
                header: String::new(),
                icon: None,
                is_enabled: true,
                content: Box::default(),
            };
            for attr in page.attributes() {
                if attr.namespace().is_some() {
                    return Err(at_attribute(
                        node.document(),
                        attr,
                        "NavigationPage directives are unsupported; name its content instead",
                    ));
                }
                reject_extension(node.document(), attr)?;
                match attr.name() {
                    "Route" => item.route = attr.value().into(),
                    "Header" => item.header = attr.value().into(),
                    "Icon" => item.icon = Some(icon_symbol(node.document(), attr)?),
                    "IsEnabled" => item.is_enabled = boolean(node.document(), attr)?,
                    _ => {
                        return Err(at_attribute(
                            node.document(),
                            attr,
                            "Unsupported NavigationPage attribute",
                        ));
                    }
                }
            }
            let mut content = None;
            for child in page.children() {
                if child.is_element() {
                    if content.is_some() {
                        return Err(at_node(
                            child,
                            "NavigationPage requires exactly one content element",
                        ));
                    }
                    content = Some(Box::new(parse_element(
                        child,
                        depth + 3,
                        parent,
                        count,
                        names,
                        checked,
                    )?));
                } else {
                    require_empty_content(child)?;
                }
            }
            item.content = content.ok_or_else(|| {
                at_node(page, "NavigationPage requires exactly one content element")
            })?;
            value.pages.push(item);
        }
        value.sections.push(definition);
    }
    Ok(())
}
fn metadata_limit(node: Node<'_, '_>, depth: usize, count: &mut usize) -> Result<(), Diagnostic> {
    *count += 1;
    if *count > MAX_ELEMENTS || depth > MAX_ELEMENT_DEPTH {
        Err(at_node(node, "Navigation metadata count/depth exceeded"))
    } else {
        Ok(())
    }
}
pub(crate) fn code(value: &NavigationDefinition) -> String {
    let icon = |v: Option<IconSymbol>| {
        v.map_or_else(
            || "None".into(),
            |v| format!("Some(rust_desktop_ui_xaml::IconSymbol::{v:?})"),
        )
    };
    let sections=value.sections.iter().map(|s|format!("rust_desktop_ui_xaml::NavigationSectionDefinition{{id:{:?}.into(),header:{:?}.into(),icon:{}}}",s.id,s.header,icon(s.icon))).collect::<Vec<_>>().join(",");
    let pages=value.pages.iter().map(|p|format!("rust_desktop_ui_xaml::NavigationPageDefinition{{route:{:?}.into(),section:{:?}.into(),header:{:?}.into(),icon:{},is_enabled:{},content:Box::new({})}}",p.route,p.section,p.header,icon(p.icon),p.is_enabled,codegen::element(&p.content))).collect::<Vec<_>>().join(",");
    format!(
        "Box::new(rust_desktop_ui_xaml::NavigationDefinition{{view:rust_desktop_ui_xaml::NavigationViewDefinition::{:?},route:{:?}.into(),pane_width:{:?},history_limit:{},show_history:{},sections:vec![{}],pages:vec![{}]}})",
        value.view,
        value.route,
        value.pane_width,
        value.history_limit,
        value.show_history,
        sections,
        pages
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse_body(body: &str) -> Result<WindowDefinition, Diagnostic> {
        parse(&format!(
            "<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'>{body}</Window>"
        ))
    }
    #[test]
    fn navigation_metadata_names_bindings_and_codegen_preserve_page_tree() {
        let w=parse_body("<NavigationPane Route='{Binding Route}' View='Office'><NavigationSection Id='work' Header='Работа'><NavigationPage Route='home' Header='Главная'><TextBox x:Name='Draft' Text='{Binding Draft}'/></NavigationPage><NavigationPage Route='notes' Header='Заметки'><TextBlock Text='Notes'/></NavigationPage></NavigationSection></NavigationPane>").unwrap();
        let ElementKind::NavigationPane(n) = &w.content.kind else {
            panic!()
        };
        assert_eq!(n.pages.len(), 2);
        assert_eq!(n.view, NavigationViewDefinition::Office);
        assert_eq!(
            w.content.children()[0].common.name.as_deref(),
            Some("Draft")
        );
        let binding = &w.content.common.bindings["Route"];
        assert_eq!(binding.mode, BindingMode::TwoWay);
        assert_eq!(
            binding.update_source_trigger,
            UpdateSourceTrigger::PropertyChanged
        );
        let code = codegen::window(&w);
        assert!(code.contains("NavigationPageDefinition"));
        assert!(code.contains("NavigationViewDefinition::Office"));
    }
    #[test]
    fn navigation_rejects_unknown_disabled_duplicate_routes_and_metadata_bindings() {
        let base = "<NavigationPane Route='home'><NavigationSection Id='work' Header='Work'><NavigationPage Route='home' Header='Home'><TextBox/></NavigationPage></NavigationSection></NavigationPane>";
        for xml in [base.replacen("Route='home'","Route='unknown'",1),base.replace("Header='Home'","Header='Home' IsEnabled='False'"),base.replace("<TextBox/>","<TextBox/><TextBox/>"),base.replace("Header='Home'","Header='{Binding Caption}'"),base.replace("</NavigationSection>","<NavigationPage Route='home' Header='Duplicate'><TextBox/></NavigationPage></NavigationSection>"),base.replace("Route='home'", "Route='../home?x=1'")] { assert!(parse_body(&xml).is_err(),"{xml}"); }
        let w = parse_body(&base.replace("NavigationPane", "NavigationFrame")).unwrap();
        let ElementKind::NavigationPane(n) = w.content.kind else {
            panic!()
        };
        assert_eq!(n.view, NavigationViewDefinition::Frame);
    }
    #[test]
    fn navigation_section_captions_are_counted_in_total_metadata_budget() {
        let mut xml = String::from("<NavigationPane>");
        for section in 0..16 {
            xml.push_str(&format!(
                "<NavigationSection Id='s{section}' Header='{}'>",
                "𐐀".repeat(128)
            ));
            for page in 0..4 {
                xml.push_str(&format!("<NavigationPage Route='p{section}_{page}' Header='{}'><TextBlock/></NavigationPage>","𐐀".repeat(96)));
            }
            xml.push_str("</NavigationSection>");
        }
        xml.push_str("</NavigationPane>");
        assert!(
            parse_body(&xml)
                .unwrap_err()
                .message
                .contains("metadata exceeds")
        );
    }
}
