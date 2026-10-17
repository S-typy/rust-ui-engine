//! Bounded lexical resources and reusable compiled presentations.
use crate::*;
use std::{collections::BTreeMap, rc::Rc};

/// The behavioral owners supported by decorative control templates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlTemplateTarget {
    Button,
    ToggleButton,
}

/// A validated decorative tree. The host retains the existing behavioral owner
/// and replaces only its content subtree. Names are isolated per template use.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlTemplateDefinition {
    pub target: ControlTemplateTarget,
    pub content_part: String,
    pub visual: Box<ElementDefinition>,
}

/// Validate before replacing any live presentation. The content part must be a
/// unique TextBlock; interactive descendants and nested templates are forbidden.
pub fn validate_control_template(template: &ControlTemplateDefinition) -> Result<(), String> {
    if !binding::valid_identifier(&template.content_part) {
        return Err("ControlTemplate requires a valid PART_Content identity".into());
    }
    validate_header_content(&template.visual)?;
    let mut pending = vec![template.visual.as_ref()];
    let mut parts = 0;
    while let Some(element) = pending.pop() {
        if element.common.control_template.is_some() {
            return Err("ControlTemplate cannot contain another control template".into());
        }
        if element.common.name.as_ref() == Some(&template.content_part) {
            if !matches!(element.kind, ElementKind::TextBlock(_)) {
                return Err("ControlTemplate PART_Content must be a TextBlock presenter".into());
            }
            parts += 1;
        }
        pending.extend(element.children());
    }
    if parts != 1 {
        return Err("ControlTemplate requires exactly one PART_Content presenter".into());
    }
    Ok(())
}

pub(crate) fn parse_template(
    node: Node<'_, '_>,
    target: ControlTemplateTarget,
    depth: usize,
    parent: usize,
    count: &mut usize,
    names: &mut HashSet<String>,
    groups: &mut HashSet<RadioGroupKey>,
) -> Result<Box<ControlTemplateDefinition>, Diagnostic> {
    if node.attributes().len() != 1 || node.attribute("ContentPart").is_none() {
        return Err(at_node(
            node,
            "ControlTemplate requires one compiled ContentPart",
        ));
    }
    let mut content = None;
    for visual in node.children() {
        if visual.is_element() {
            if content.is_some() {
                return Err(at_node(visual, "ControlTemplate requires one visual root"));
            }
            content = Some(Box::new(parse_element(
                visual,
                depth + 1,
                parent,
                count,
                names,
                groups,
            )?));
        } else {
            require_empty_content(visual)?;
        }
    }
    let template = Box::new(ControlTemplateDefinition {
        target,
        content_part: node.attribute("ContentPart").unwrap().into(),
        visual: content.ok_or_else(|| at_node(node, "ControlTemplate requires one visual root"))?,
    });
    validate_control_template(&template).map_err(|message| at_node(node, message))?;
    Ok(template)
}

#[derive(Clone)]
struct Attr {
    value: String,
    offset: usize,
    scope: Option<Rc<Scope>>,
}
#[derive(Clone)]
struct Markup {
    name: String,
    namespace: Option<String>,
    attrs: BTreeMap<String, Attr>,
    children: Vec<Markup>,
    text: String,
    offset: usize,
}
impl Markup {
    fn get(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(|attr| attr.value.as_str())
    }
    fn set(&mut self, name: &str, value: impl Into<String>) {
        self.attrs.insert(
            name.into(),
            Attr {
                value: value.into(),
                offset: self.offset,
                scope: None,
            },
        );
    }
}
#[derive(Clone)]
struct Scope {
    parent: Option<Rc<Scope>>,
    resources: BTreeMap<String, Markup>,
    styles: Vec<Markup>,
}
impl Scope {
    fn find(self: &Rc<Self>, key: &str) -> Option<(Rc<Self>, Markup)> {
        self.resources
            .get(key)
            .map(|value| (self.clone(), value.clone()))
            .or_else(|| self.parent.as_ref().and_then(|parent| parent.find(key)))
    }
}
pub(crate) struct Lowered {
    pub text: String,
    pub origins: Vec<usize>,
}
struct Compiler<'a> {
    source: &'a str,
    instances: usize,
    resources: usize,
    expanded: usize,
    payload_bytes: usize,
}
impl Compiler<'_> {
    fn error(&self, offset: usize, message: impl Into<String>) -> Diagnostic {
        crate::extensions::at_original(self.source, offset, message.into())
    }
    fn scope(
        &mut self,
        node: &mut Markup,
        parent: Option<Rc<Scope>>,
    ) -> Result<Rc<Scope>, Diagnostic> {
        let expected = format!("{}.Resources", node.name);
        let mut resources = BTreeMap::new();
        let mut styles = Vec::new();
        let mut found = false;
        let mut children = Vec::new();
        for child in std::mem::take(&mut node.children) {
            if child.name != expected {
                children.push(child);
                continue;
            }
            if found || !child.attrs.is_empty() || !child.text.trim().is_empty() {
                return Err(self.error(
                    child.offset,
                    "Resources requires one property element without attributes or text",
                ));
            }
            found = true;
            for mut resource in child.children {
                self.resources += 1;
                if self.resources > 256 {
                    return Err(self.error(
                        resource.offset,
                        "Resource declarations exceed the 256-resource limit",
                    ));
                }
                if resource.namespace.as_deref() != Some(UI_NAMESPACE) {
                    return Err(self.error(
                        resource.offset,
                        "Resource declarations require the UI namespace",
                    ));
                }
                let key = resource.attrs.remove("x:Key").map(|attr| attr.value);
                match resource.name.as_str() {
                    "Style" => {
                        self.check_style(&resource)?;
                        if key.is_none() || resource.get("Selector").is_some() {
                            styles.push(resource.clone());
                        }
                    }
                    "ControlTemplate" | "DataTemplate" | "HeaderTemplate" => {
                        self.check_template(&resource)?;
                    }
                    "String" | "Color" | "Double" | "Thickness" => {
                        if resource.attrs.len() != 1
                            || resource.get("Value").is_none()
                            || !resource.children.is_empty()
                            || !resource.text.trim().is_empty()
                        {
                            return Err(self.error(
                                resource.offset,
                                "Scalar resources require only Value and x:Key",
                            ));
                        }
                    }
                    _ => {
                        return Err(self.error(resource.offset, "Unsupported resource declaration"));
                    }
                }
                if let Some(key) = key {
                    if !binding::valid_identifier(&key) || key.len() > 128 {
                        return Err(self.error(
                            resource.offset,
                            "Resource keys require identifiers of at most 128 bytes",
                        ));
                    }
                    if resources.insert(key, resource.clone()).is_some() {
                        return Err(
                            self.error(resource.offset, "Duplicate resource key in the same scope")
                        );
                    }
                } else if resource.name != "Style" {
                    return Err(self.error(
                        resource.offset,
                        "Resources require x:Key except implicit Style",
                    ));
                }
            }
        }
        node.children = children;
        let scope = Rc::new(Scope {
            parent,
            resources,
            styles,
        });
        // Even unused scalar aliases and inheritance chains must be valid.
        for resource in scope.resources.values() {
            if resource.name == "Style" {
                let values = self.style_values(&scope, resource, &mut Vec::new())?;
                self.validate_style_values(resource, values)?;
            } else if matches!(
                resource.name.as_str(),
                "ControlTemplate" | "DataTemplate" | "HeaderTemplate"
            ) {
                self.validate_template_graph(&scope, resource, &mut Vec::new())?;
            } else if matches!(
                resource.name.as_str(),
                "String" | "Color" | "Double" | "Thickness"
            ) {
                let value =
                    self.resolve_value(&scope, &resource.attrs["Value"], &mut Vec::new())?;
                self.validate_scalar(resource, &value.value)?;
            }
        }
        for style in &scope.styles {
            let values = self.style_values(&scope, style, &mut Vec::new())?;
            self.validate_style_values(style, values)?;
        }
        Ok(scope)
    }
    fn validate_scalar(&self, resource: &Markup, value: &str) -> Result<(), Diagnostic> {
        let valid = match resource.name.as_str() {
            "Color" => {
                value.starts_with('#')
                    && matches!(value.len(), 7 | 9)
                    && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
            }
            "Double" => value.parse::<f64>().is_ok_and(f64::is_finite),
            "Thickness" => {
                let values: Vec<_> = value.split(',').map(str::trim).collect();
                matches!(values.len(), 1 | 2 | 4)
                    && values.iter().all(|value| {
                        value.parse::<f64>().is_ok_and(|value| {
                            value.is_finite() && (0.0..=MAX_DIMENSION).contains(&value)
                        })
                    })
            }
            _ => true,
        };
        if valid {
            Ok(())
        } else {
            Err(self.error(
                resource.offset,
                format!("Invalid {} resource Value", resource.name),
            ))
        }
    }
    fn validate_style_values(
        &self,
        style: &Markup,
        mut values: BTreeMap<String, Attr>,
    ) -> Result<(), Diagnostic> {
        values.retain(|key, _| {
            !matches!(
                key.as_str(),
                "Template" | "HeaderTemplate" | "ContentTemplate" | "ItemTemplate"
            )
        });
        let element = Markup {
            name: style.get("TargetType").unwrap().into(),
            namespace: Some(UI_NAMESPACE.into()),
            attrs: values,
            children: Vec::new(),
            text: String::new(),
            offset: style.offset,
        };
        let mut output = Lowered {
            text: String::new(),
            origins: Vec::new(),
        };
        write(&element, &mut output, true)
            .map_err(|offset| self.error(offset, "Expanded style exceeds source size limit"))?;
        output.origins.push(style.offset);
        let document = Document::parse(&output.text)
            .map_err(|error| self.error(style.offset, error.to_string()))?;
        parse_element_properties(document.root_element(), &mut HashSet::new()).map_err(
            |diagnostic| extensions::remap(self.source, &output.text, &output.origins, diagnostic),
        )?;
        Ok(())
    }
    fn validate_template_graph(
        &self,
        scope: &Rc<Scope>,
        template: &Markup,
        chain: &mut Vec<usize>,
    ) -> Result<(), Diagnostic> {
        if chain.contains(&template.offset) || chain.len() >= 32 {
            return Err(self.error(
                template.offset,
                "Template resource cycle or depth limit exceeded",
            ));
        }
        chain.push(template.offset);
        let mut pending = vec![&template.children[0]];
        while let Some(node) = pending.pop() {
            if node.name.ends_with(".Resources") {
                return Err(self.error(
                    node.offset,
                    "Declare template resources in their enclosing Resources scope",
                ));
            }
            for (property, attr) in &node.attrs {
                if matches!(
                    property.as_str(),
                    "Template" | "HeaderTemplate" | "ContentTemplate" | "ItemTemplate"
                ) {
                    let (declaring, dependency) = self.resource(scope, &attr.value, attr.offset)?;
                    let expected = if property == "Template" {
                        "ControlTemplate"
                    } else if matches!(property.as_str(), "ContentTemplate" | "ItemTemplate") {
                        "DataTemplate"
                    } else {
                        "HeaderTemplate"
                    };
                    if dependency.name != expected {
                        return Err(self.error(
                            attr.offset,
                            "Template resource kind does not match its property",
                        ));
                    }
                    self.validate_template_graph(&declaring, &dependency, chain)?;
                } else if attr.value.trim_start().starts_with("{StaticResource") {
                    self.resolve_value(scope, attr, &mut Vec::new())?;
                }
            }
            pending.extend(&node.children);
        }
        chain.pop();
        Ok(())
    }
    fn check_style(&self, style: &Markup) -> Result<(), Diagnostic> {
        if style
            .attrs
            .keys()
            .any(|key| !matches!(key.as_str(), "TargetType" | "BasedOn" | "Selector"))
            || !style.text.trim().is_empty()
        {
            return Err(self.error(
                style.offset,
                "Style accepts TargetType, BasedOn and Selector",
            ));
        }
        let target = style
            .get("TargetType")
            .ok_or_else(|| self.error(style.offset, "Style requires TargetType"))?;
        if !binding::valid_identifier(target) {
            return Err(self.error(
                style.offset,
                "Style TargetType requires a supported element name",
            ));
        }
        if let Some(selector) = style.get("Selector") {
            selector_specificity(selector, target)
                .map_err(|message| self.error(style.offset, message))?;
        }
        let mut properties = HashSet::new();
        for setter in &style.children {
            if setter.name != "Setter"
                || setter.namespace.as_deref() != Some(UI_NAMESPACE)
                || setter.attrs.len() != 2
                || setter.get("Property").is_none()
                || setter.get("Value").is_none()
                || !setter.children.is_empty()
                || !setter.text.trim().is_empty()
            {
                return Err(self.error(
                    setter.offset,
                    "Style accepts Setter with Property and Value only",
                ));
            }
            let property = setter.get("Property").unwrap();
            if !binding::valid_identifier(property)
                || matches!(property, "Style" | "Classes" | "DataContext" | "Name")
                || !properties.insert(property)
            {
                return Err(self.error(setter.offset, "Invalid or duplicate Style setter property"));
            }
        }
        Ok(())
    }
    fn check_template(&self, template: &Markup) -> Result<(), Diagnostic> {
        if template.attrs.keys().any(|key| key != "TargetType")
            || !template.text.trim().is_empty()
            || template.children.len() != 1
        {
            return Err(self.error(
                template.offset,
                "Templates require exactly one visual root and optional TargetType",
            ));
        }
        match template.name.as_str() {
            "ControlTemplate"
                if !matches!(template.get("TargetType"), Some("Button" | "ToggleButton")) =>
            {
                return Err(self.error(
                    template.offset,
                    "ControlTemplate TargetType supports Button or ToggleButton",
                ));
            }
            "HeaderTemplate"
                if template
                    .get("TargetType")
                    .is_some_and(|t| !matches!(t, "GroupBox" | "Expander" | "GroupTab")) =>
            {
                return Err(self.error(
                    template.offset,
                    "HeaderTemplate TargetType supports GroupBox, Expander or GroupTab",
                ));
            }
            "DataTemplate" if template.get("TargetType").is_some() => {
                return Err(self.error(template.offset, "DataTemplate does not accept TargetType"));
            }
            _ => {}
        }
        let mut pending = vec![&template.children[0]];
        let mut names = HashSet::new();
        let mut parts = 0;
        while let Some(node) = pending.pop() {
            if template.name != "DataTemplate"
                && !matches!(
                    node.name.as_str(),
                    "ContentPresenter"
                        | "TextBlock"
                        | "Icon"
                        | "Image"
                        | "Separator"
                        | "StackPanel"
                        | "WrapPanel"
                        | "Grid"
                        | "Overlay"
                        | "Border"
                        | "Run"
                        | "Inline"
                        | "Grid.RowDefinitions"
                        | "Grid.ColumnDefinitions"
                        | "RowDefinition"
                        | "ColumnDefinition"
                )
            {
                return Err(self.error(
                    node.offset,
                    "ControlTemplate and HeaderTemplate accept only noninteractive visual content",
                ));
            }
            if let Some(name) = node.get("x:Name") {
                if !binding::valid_identifier(name) || !names.insert(name) {
                    return Err(self.error(
                        node.offset,
                        "Template contains an invalid or duplicate name",
                    ));
                }
                if name == "PART_Content" {
                    if template.name != "ControlTemplate"
                        || node.name != "ContentPresenter"
                        || node.get("ContentTemplate").is_some()
                        || !node.children.is_empty()
                    {
                        return Err(self.error(
                            node.offset,
                            "PART_Content must be a leaf ContentPresenter in a ControlTemplate",
                        ));
                    }
                    parts += 1;
                }
            }
            pending.extend(&node.children);
        }
        if template.name == "ControlTemplate" && parts != 1 {
            return Err(self.error(
                template.offset,
                "ControlTemplate requires exactly one ContentPresenter named PART_Content",
            ));
        }
        Ok(())
    }
    fn resource(
        &self,
        scope: &Rc<Scope>,
        value: &str,
        offset: usize,
    ) -> Result<(Rc<Scope>, Markup), Diagnostic> {
        let key =
            reference(value).ok_or_else(|| self.error(offset, "Expected {StaticResource Key}"))?;
        scope
            .find(key)
            .ok_or_else(|| self.error(offset, format!("Unknown resource '{key}'")))
    }
    fn resolve_value(
        &self,
        scope: &Rc<Scope>,
        value: &Attr,
        stack: &mut Vec<usize>,
    ) -> Result<Attr, Diagnostic> {
        if !value.value.trim_start().starts_with("{StaticResource") {
            return Ok(value.clone());
        }
        let (declaring, resource) = self.resource(scope, &value.value, value.offset)?;
        if !matches!(
            resource.name.as_str(),
            "String" | "Color" | "Double" | "Thickness"
        ) {
            return Err(self.error(value.offset, "Scalar property requires a scalar resource"));
        }
        if stack.contains(&resource.offset) || stack.len() >= 32 {
            return Err(self.error(
                value.offset,
                "Resource reference cycle or depth limit exceeded",
            ));
        }
        stack.push(resource.offset);
        let resolved = self.resolve_value(&declaring, &resource.attrs["Value"], stack)?;
        stack.pop();
        Ok(Attr {
            value: resolved.value,
            offset: value.offset,
            scope: None,
        })
    }
    fn style_values(
        &self,
        scope: &Rc<Scope>,
        style: &Markup,
        stack: &mut Vec<usize>,
    ) -> Result<BTreeMap<String, Attr>, Diagnostic> {
        if stack.contains(&style.offset) || stack.len() >= 32 {
            return Err(self.error(style.offset, "Style BasedOn cycle or depth limit exceeded"));
        }
        stack.push(style.offset);
        let mut values = if let Some(base) = style.get("BasedOn") {
            let (declaring, inherited) = self.resource(scope, base, style.offset)?;
            if inherited.name != "Style" || inherited.get("TargetType") != style.get("TargetType") {
                return Err(self.error(
                    style.offset,
                    "Style BasedOn requires a Style with the same TargetType",
                ));
            }
            self.style_values(&declaring, &inherited, stack)?
        } else {
            BTreeMap::new()
        };
        for setter in &style.children {
            let property = setter.get("Property").unwrap();
            let attr = &setter.attrs["Value"];
            values.insert(
                property.into(),
                if matches!(
                    property,
                    "Template" | "HeaderTemplate" | "ContentTemplate" | "ItemTemplate"
                ) {
                    let mut attr = attr.clone();
                    attr.scope = Some(scope.clone());
                    attr
                } else {
                    self.resolve_value(scope, attr, &mut Vec::new())?
                },
            );
        }
        stack.pop();
        Ok(values)
    }
    fn apply_styles(&self, scope: &Rc<Scope>, node: &mut Markup) -> Result<(), Diagnostic> {
        let mut scopes = Vec::new();
        let mut current = Some(scope.clone());
        while let Some(scope) = current {
            current = scope.parent.clone();
            scopes.push(scope);
        }
        let mut values = BTreeMap::new();
        for scope in scopes.iter().rev() {
            let mut matching: Vec<_> = scope
                .styles
                .iter()
                .filter(|style| style.get("TargetType") == Some(node.name.as_str()))
                .filter(|style| selector_matches(style.get("Selector").unwrap_or(&node.name), node))
                .collect();
            matching.sort_by_key(|style| {
                selector_specificity(style.get("Selector").unwrap_or(&node.name), &node.name)
                    .unwrap_or(0)
            });
            for style in matching {
                values.extend(self.style_values(scope, style, &mut Vec::new())?);
            }
        }
        if let Some(style) = node.attrs.remove("Style") {
            let (declaring, style) = self.resource(scope, &style.value, style.offset)?;
            if style.name != "Style" || style.get("TargetType") != Some(node.name.as_str()) {
                return Err(self.error(
                    node.offset,
                    "Explicit Style TargetType does not match its owner",
                ));
            }
            values.extend(self.style_values(&declaring, &style, &mut Vec::new())?);
        }
        values.extend(std::mem::take(&mut node.attrs));
        node.attrs = values;
        if let Some(classes) = node.attrs.remove("Classes")
            && classes
                .value
                .split_whitespace()
                .any(|class| !binding::valid_identifier(class))
        {
            return Err(self.error(
                classes.offset,
                "Classes require whitespace-separated identifiers",
            ));
        }
        Ok(())
    }
    fn instantiate(
        &mut self,
        template: &Markup,
        owner: &Markup,
    ) -> Result<(Markup, String), Diagnostic> {
        self.instances += 1;
        if self.instances > 256 {
            return Err(self.error(owner.offset, "Template expansion exceeds 256 instances"));
        }
        let prefix = format!("__template_{}_", self.instances);
        let mut visual = template.children[0].clone();
        let mut pending = vec![&mut visual];
        while let Some(node) = pending.pop() {
            if let Some(name) = node.attrs.get_mut("x:Name") {
                name.value = format!("{prefix}{}", name.value);
            }
            if node.name == "ContentPresenter"
                && node.get("x:Name") == Some(format!("{prefix}PART_Content").as_str())
            {
                node.name = "TextBlock".into();
                node.set("Text", "{TemplateBinding Content}");
            }
            let mut remove = Vec::new();
            for (property, attr) in &mut node.attrs {
                let mut inherited = false;
                if let Some(name) = template_binding(&attr.value) {
                    if template.name == "DataTemplate" {
                        return Err(self.error(attr.offset, "TemplateBinding is only valid inside ControlTemplate or HeaderTemplate"));
                    }
                    if !matches!(
                        name,
                        "Content"
                            | "Header"
                            | "Foreground"
                            | "FontSize"
                            | "FontWeight"
                            | "Background"
                            | "BorderBrush"
                            | "Padding"
                            | "CornerRadius"
                    ) {
                        return Err(self.error(attr.offset, "Unsupported TemplateBinding property"));
                    }
                    if let Some(value) = owner.attrs.get(name) {
                        attr.value = value.value.clone();
                        inherited = true;
                    } else {
                        remove.push(property.clone());
                    }
                }
                // Template names form a private scope; ElementName references
                // intentionally cannot escape it and are renamed with their node.
                if attr.value.trim_start().starts_with("{Binding")
                    || attr.value.trim_start().starts_with("{MultiBinding")
                {
                    attr.value = rewrite_binding(
                        &node.name,
                        property,
                        &attr.value,
                        (!inherited).then_some(prefix.as_str()),
                        inherited,
                    )
                    .map_err(|message| self.error(attr.offset, message))?;
                }
            }
            for name in remove {
                node.attrs.remove(&name);
            }
            pending.extend(&mut node.children);
        }
        Ok((visual, format!("{prefix}PART_Content")))
    }
    fn expand(
        &mut self,
        mut node: Markup,
        parent: Option<Rc<Scope>>,
        chain: &mut Vec<usize>,
        depth: usize,
    ) -> Result<Markup, Diagnostic> {
        self.expanded += 1;
        if depth > MAX_ELEMENT_DEPTH + 8 || self.expanded > MAX_XML_NODES as usize {
            return Err(self.error(
                node.offset,
                "Expanded resource/template tree exceeds its bounded limits",
            ));
        }
        let scope = self.scope(&mut node, parent)?;
        self.apply_styles(&scope, &mut node)?;
        for (name, attr) in &mut node.attrs {
            if !matches!(
                name.as_str(),
                "Template" | "HeaderTemplate" | "ContentTemplate" | "ItemTemplate"
            ) {
                *attr = self.resolve_value(&scope, attr, &mut Vec::new())?;
            }
        }
        let payload = node.name.len()
            + node.text.len()
            + node
                .attrs
                .iter()
                .map(|(name, attr)| name.len() + attr.value.len())
                .sum::<usize>();
        self.payload_bytes = self.payload_bytes.saturating_add(payload);
        if self.payload_bytes > MAX_SOURCE_BYTES {
            return Err(self.error(
                node.offset,
                "Expanded resource/template payload exceeds source size limit",
            ));
        }
        if node.name == "ContentPresenter" {
            let attr = node.attrs.remove("ContentTemplate").ok_or_else(|| self.error(node.offset, "ContentPresenter requires ContentTemplate, except PART_Content inside ControlTemplate"))?;
            let (declaring, template) = self.resource(
                attr.scope.as_ref().unwrap_or(&scope),
                &attr.value,
                attr.offset,
            )?;
            if template.name != "DataTemplate" {
                return Err(self.error(
                    attr.offset,
                    "ContentTemplate requires a DataTemplate resource",
                ));
            }
            if chain.contains(&template.offset) || chain.len() >= 32 {
                return Err(self.error(
                    attr.offset,
                    "Template resource cycle or depth limit exceeded",
                ));
            }
            if !node.children.is_empty() || !node.text.trim().is_empty() {
                return Err(
                    self.error(node.offset, "ContentPresenter cannot also declare children")
                );
            }
            chain.push(template.offset);
            let (visual, _) = self.instantiate(&template, &node)?;
            let result = self.expand(visual, Some(declaring), chain, depth + 1);
            chain.pop();
            // Keep the consumer's public identity/context on a transparent host.
            // Replacing the template root name would invalidate its internal
            // ElementName references after namescope isolation.
            return Ok(Markup {
                name: "Border".into(),
                namespace: Some(UI_NAMESPACE.into()),
                attrs: node.attrs,
                children: vec![result?],
                text: String::new(),
                offset: node.offset,
            });
        }
        for (property, expected) in [
            ("Template", "ControlTemplate"),
            ("HeaderTemplate", "HeaderTemplate"),
            ("ContentTemplate", "DataTemplate"),
            ("ItemTemplate", "DataTemplate"),
        ] {
            if let Some(attr) = node.attrs.remove(property) {
                let (declaring, template) = self.resource(
                    attr.scope.as_ref().unwrap_or(&scope),
                    &attr.value,
                    attr.offset,
                )?;
                if template.name != expected
                    || template
                        .get("TargetType")
                        .is_some_and(|target| target != node.name)
                {
                    return Err(self.error(
                        attr.offset,
                        format!("{property} resource type or TargetType does not match its owner"),
                    ));
                }
                if property == "HeaderTemplate"
                    && !matches!(node.name.as_str(), "GroupBox" | "Expander" | "GroupTab")
                {
                    return Err(self.error(
                        attr.offset,
                        "HeaderTemplate requires GroupBox, Expander or GroupTab",
                    ));
                }
                if property == "ContentTemplate" && node.name != "EditorButton" {
                    return Err(self.error(
                        attr.offset,
                        "ContentTemplate requires ContentPresenter or EditorButton",
                    ));
                }
                if property == "ItemTemplate"
                    && !matches!(
                        node.name.as_str(),
                        "ListBox" | "ComboBox" | "LookupEdit" | "TileCollection" | "Gallery"
                    )
                {
                    return Err(self.error(
                        attr.offset,
                        "ItemTemplate requires ListBox, ComboBox or LookupEdit",
                    ));
                }
                if chain.contains(&template.offset) || chain.len() >= 32 {
                    return Err(self.error(
                        attr.offset,
                        "Template resource cycle or depth limit exceeded",
                    ));
                }
                chain.push(template.offset);
                let (visual, part) = self.instantiate(&template, &node)?;
                let mut visual = self.expand(visual, Some(declaring), chain, depth + 1)?;
                chain.pop();
                if !visual.attrs.contains_key("VerticalAlignment") {
                    visual.set("VerticalAlignment", "Center");
                }
                let mut wrapper = Markup {
                    name: format!("{}.{property}", node.name),
                    namespace: Some(UI_NAMESPACE.into()),
                    attrs: BTreeMap::new(),
                    children: vec![visual],
                    text: String::new(),
                    offset: node.offset,
                };
                if property == "Template" {
                    wrapper.set("ContentPart", part);
                }
                node.children.push(wrapper);
            }
        }
        let mut children = Vec::new();
        for child in std::mem::take(&mut node.children) {
            // The generated template is already recursively expanded in its
            // declaration scope, not the consumer's potentially shadowing scope.
            if child.name == format!("{}.Template", node.name)
                || ((child.name == format!("{}.HeaderTemplate", node.name)
                    || child.name == format!("{}.ContentTemplate", node.name)
                    || child.name == format!("{}.ItemTemplate", node.name))
                    && child.offset == node.offset)
            {
                children.push(child);
            } else if node.name == "EditorButton" && child.name == "EditorButton.ContentTemplate" {
                // Inline button visuals share the resource template's private
                // namescope policy while using their enclosing lexical scope.
                let mut template = child.clone();
                template.name = "DataTemplate".into();
                self.check_template(&template)?;
                let (visual, _) = self.instantiate(&template, &node)?;
                let mut visual = self.expand(visual, Some(scope.clone()), chain, depth + 1)?;
                if !visual.attrs.contains_key("VerticalAlignment") {
                    visual.set("VerticalAlignment", "Center");
                }
                let mut wrapper = child;
                wrapper.children = vec![visual];
                children.push(wrapper);
            } else {
                children.push(self.expand(child, Some(scope.clone()), chain, depth + 1)?);
            }
        }
        node.children = children;
        Ok(node)
    }
}
fn reference(value: &str) -> Option<&str> {
    let key = value
        .trim()
        .strip_prefix("{StaticResource ")?
        .strip_suffix('}')?
        .trim();
    (binding::valid_identifier(key) && key.len() <= 128).then_some(key)
}
fn template_binding(value: &str) -> Option<&str> {
    value
        .trim()
        .strip_prefix("{TemplateBinding ")?
        .strip_suffix('}')
        .map(str::trim)
}
fn selector_specificity(selector: &str, target: &str) -> Result<u8, &'static str> {
    if selector == target {
        return Ok(0);
    }
    for (separator, specificity) in [('.', 1), ('#', 2)] {
        if let Some((kind, name)) = selector.split_once(separator)
            && kind == target
            && binding::valid_identifier(name)
        {
            return Ok(specificity);
        }
    }
    Err("Selector requires TargetType, TargetType.class or TargetType#name")
}
fn selector_matches(selector: &str, node: &Markup) -> bool {
    if let Some((_, name)) = selector.split_once('#') {
        return node.get("x:Name") == Some(name);
    }
    if let Some((_, class)) = selector.split_once('.') {
        return node
            .get("Classes")
            .unwrap_or("")
            .split_whitespace()
            .any(|candidate| candidate == class);
    }
    selector == node.name
}
fn rewrite_binding(
    kind: &str,
    property: &str,
    value: &str,
    prefix: Option<&str>,
    read_only: bool,
) -> Result<String, String> {
    let xml = format!("<Value Property=\"{}\"/>", escape(value));
    let document = Document::parse(&xml).map_err(|error| error.to_string())?;
    let editable = matches!(
        property,
        "Text"
            | "Value"
            | "SelectedValue"
            | "SelectedKey"
            | "LowerValue"
            | "UpperValue"
            | "SelectedValues"
            | "IsChecked"
            | "IsExpanded"
            | "SelectedIndex"
            | "FilterText"
    ) && !matches!(kind, "TextBlock" | "ProgressBar" | "BusyIndicator");
    let parsed = binding::parse_binding(
        &document,
        document.root_element().attributes().next().unwrap(),
        editable,
        UpdateSourceTrigger::PropertyChanged,
    )
    .map_err(|error| error.message)?;
    let multiple = value.trim_start().starts_with("{MultiBinding");
    let body = value
        .trim()
        .strip_prefix(if multiple {
            "{MultiBinding"
        } else {
            "{Binding"
        })
        .unwrap()
        .strip_suffix('}')
        .unwrap();
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quote = None;
    for (offset, ch) in body.char_indices() {
        if matches!(ch, '\'' | '"') {
            if quote == Some(ch) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(ch);
            }
        } else if ch == ',' && quote.is_none() {
            parts.push(&body[start..offset]);
            start = offset + 1;
        }
    }
    parts.push(&body[start..]);
    let mut result = Vec::new();
    let mut has_mode = false;
    for part in parts {
        let key = part.split_once('=').map(|(key, _)| key.trim());
        match key {
            Some("ElementName") if prefix.is_some() => result.push(format!(
                "ElementName={}{}",
                prefix.unwrap(),
                parsed.element_name.as_deref().unwrap()
            )),
            Some("Paths") if prefix.is_some() => result.push(format!(
                "Paths='{}'",
                parsed
                    .inputs
                    .iter()
                    .map(|input| input.element_name.as_ref().map_or_else(
                        || input.path.clone(),
                        |name| format!("#{}{name}.{}", prefix.unwrap(), input.path)
                    ))
                    .collect::<Vec<_>>()
                    .join(";")
            )),
            Some("UpdateSourceTrigger") if read_only => {}
            Some("Mode") if read_only => {
                has_mode = true;
                result.push(format!(
                    "Mode={}",
                    if parsed.mode == BindingMode::OneTime {
                        "OneTime"
                    } else {
                        "OneWay"
                    }
                ));
            }
            _ => result.push(part.trim().into()),
        }
    }
    if read_only && !has_mode {
        result.push("Mode=OneWay".into());
    }
    Ok(format!(
        "{{{} {}}}",
        if multiple { "MultiBinding" } else { "Binding" },
        result.join(", ")
    ))
}
fn read(node: Node<'_, '_>) -> Markup {
    Markup {
        name: node.tag_name().name().into(),
        namespace: node.tag_name().namespace().map(str::to_owned),
        attrs: node
            .attributes()
            .map(|attribute| {
                let name = match attribute.namespace() {
                    Some(XAML_NAMESPACE) => format!("x:{}", attribute.name()),
                    Some(namespace) => format!("{{{namespace}}}{}", attribute.name()),
                    None => attribute.name().into(),
                };
                (
                    name,
                    Attr {
                        value: attribute.value().into(),
                        offset: attribute.range().start,
                        scope: None,
                    },
                )
            })
            .collect(),
        children: node.children().filter(Node::is_element).map(read).collect(),
        text: node
            .children()
            .filter(Node::is_text)
            .filter_map(|node| node.text())
            .collect(),
        offset: node.range().start,
    }
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\r', "&#13;")
        .replace('\n', "&#10;")
}
fn append(output: &mut Lowered, value: &str, offset: usize) -> Result<(), usize> {
    if value.len() > MAX_SOURCE_BYTES.saturating_sub(output.text.len()) {
        return Err(offset);
    }
    output.text.push_str(value);
    output
        .origins
        .extend(std::iter::repeat_n(offset, value.len()));
    Ok(())
}
fn write(node: &Markup, output: &mut Lowered, root: bool) -> Result<(), usize> {
    append(output, &format!("<{}", node.name), node.offset)?;
    if root {
        append(
            output,
            &format!(" xmlns=\"{UI_NAMESPACE}\" xmlns:x=\"{XAML_NAMESPACE}\""),
            node.offset,
        )?;
    }
    if node.namespace.as_deref() != Some(UI_NAMESPACE) {
        append(
            output,
            &format!(
                " xmlns=\"{}\"",
                escape(node.namespace.as_deref().unwrap_or(""))
            ),
            node.offset,
        )?;
    }
    for (name, attr) in &node.attrs {
        append(
            output,
            &format!(" {name}=\"{}\"", escape(&attr.value)),
            attr.offset,
        )?;
    }
    append(output, ">", node.offset)?;
    append(output, &escape(&node.text), node.offset)?;
    for child in &node.children {
        write(child, output, false)?;
    }
    append(output, &format!("</{}>", node.name), node.offset)
}
pub(crate) fn lower(source: &str) -> Result<Option<Lowered>, Diagnostic> {
    if source.len() > MAX_SOURCE_BYTES {
        return Ok(None);
    }
    let document = Document::parse_with_options(
        source,
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: MAX_XML_NODES,
            ..Default::default()
        },
    )
    .map_err(|error| Diagnostic {
        line: error.pos().row,
        column: error.pos().col,
        message: format!("invalid XML: {error}"),
    })?;
    if let Some(node) = document.descendants().find(Node::is_pi) {
        return Err(at_node(node, "processing instructions are not supported"));
    }
    let needed = document.descendants().filter(Node::is_element).any(|node| {
        node.tag_name().name().ends_with(".Resources")
            || node.tag_name().name() == "ContentPresenter"
            || node.tag_name().name() == "EditorButton.ContentTemplate"
            || node.attributes().any(|a| {
                matches!(
                    a.name(),
                    "Style" | "Classes" | "Template" | "HeaderTemplate"
                ) || a.value().trim_start().starts_with("{StaticResource")
            })
    });
    if !needed {
        return Ok(None);
    }
    if let Some(node) = document
        .descendants()
        .filter(Node::is_element)
        .find(|node| node.ancestors().take(MAX_ELEMENT_DEPTH + 10).count() > MAX_ELEMENT_DEPTH + 8)
    {
        return Err(at_node(
            node,
            "Resource/template source exceeds the nesting limit",
        ));
    }
    let mut compiler = Compiler {
        source,
        instances: 0,
        resources: 0,
        expanded: 0,
        payload_bytes: 0,
    };
    let root = compiler.expand(read(document.root_element()), None, &mut Vec::new(), 0)?;
    let mut output = Lowered {
        text: String::new(),
        origins: Vec::new(),
    };
    write(&root, &mut output, true)
        .map_err(|offset| compiler.error(offset, "Expanded form exceeds source size limit"))?;
    output.origins.push(source.len());
    if output.text.len() > MAX_SOURCE_BYTES {
        return Err(compiler.error(0, "Expanded form exceeds source size limit"));
    }
    Ok(Some(output))
}
