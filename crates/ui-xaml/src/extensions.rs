//! Explicit compiler extensions lower application components into the validated profile.
use crate::{
    Diagnostic, MAX_SOURCE_BYTES, MAX_XML_NODES, UI_NAMESPACE, WindowDefinition, XAML_NAMESPACE,
};
use std::{collections::BTreeMap, sync::Arc};

/// Read-only source supplied to a registered component factory.
#[derive(Clone, Debug)]
pub struct ComponentMarkup {
    pub namespace: String,
    pub name: String,
    /// Namespaced attributes use `{namespace}local`; ordinary attributes retain their name.
    pub attributes: BTreeMap<String, String>,
    /// Original child markup, without the component's outer tag.
    pub content: String,
    pub line: u32,
    pub column: u32,
}
type Factory = dyn Fn(&ComponentMarkup) -> Result<String, String> + Send + Sync;
/// Application-owned build-time registry. No dynamic libraries or arbitrary Rust
/// source are accepted: factories return XML, then the regular parser validates it.
#[derive(Default, Clone)]
pub struct ComponentRegistry {
    factories: BTreeMap<(String, String), Arc<Factory>>,
}
impl ComponentRegistry {
    pub fn register(
        &mut self,
        namespace: &str,
        name: &str,
        factory: impl Fn(&ComponentMarkup) -> Result<String, String> + Send + Sync + 'static,
    ) -> Result<(), String> {
        if namespace.is_empty()
            || namespace.len() > 256
            || namespace == UI_NAMESPACE
            || namespace == XAML_NAMESPACE
            || !crate::binding::valid_identifier(name)
            || name.len() > 128
        {
            return Err("Custom components require a distinct namespace and an identifier of at most 128 bytes".into());
        }
        if self.factories.len() >= 128
            && !self
                .factories
                .contains_key(&(namespace.into(), name.into()))
        {
            return Err("Component registry is limited to 128 factories".into());
        }
        self.factories
            .insert((namespace.into(), name.into()), Arc::new(factory));
        Ok(())
    }
    /// Expand at most 256 factories across eight nested expansion rounds.
    /// Diagnostics inside generated content point to its original component use.
    pub fn parse(&self, source: &str) -> Result<WindowDefinition, Diagnostic> {
        if source.len() > MAX_SOURCE_BYTES {
            return crate::parse(source);
        }
        let mut text = source.to_owned();
        let mut origins: Vec<usize> = (0..=source.len()).collect();
        let mut expanded = 0;
        for round in 0..=8 {
            let document = roxmltree::Document::parse_with_options(
                &text,
                roxmltree::ParsingOptions {
                    allow_dtd: false,
                    nodes_limit: MAX_XML_NODES,
                    ..Default::default()
                },
            )
            .map_err(|error| {
                remap(
                    source,
                    &text,
                    &origins,
                    Diagnostic {
                        line: error.pos().row,
                        column: error.pos().col,
                        message: format!("invalid XML: {error}"),
                    },
                )
            })?;
            if let Some(node) = document.descendants().find(roxmltree::Node::is_pi) {
                return Err(at_original(
                    source,
                    origins[node.range().start],
                    "processing instructions are not supported".into(),
                ));
            }
            let nodes: Vec<_> = document
                .descendants()
                .filter(|node| {
                    node.is_element()
                        && node
                            .tag_name()
                            .namespace()
                            .is_some_and(|ns| ns != UI_NAMESPACE)
                })
                .filter(|node| {
                    !node.ancestors().skip(1).any(|parent| {
                        parent.is_element()
                            && parent
                                .tag_name()
                                .namespace()
                                .is_some_and(|ns| ns != UI_NAMESPACE)
                    })
                })
                .collect();
            if nodes.is_empty() {
                let mut window = crate::parse(&text)
                    .map_err(|diagnostic| remap(source, &text, &origins, diagnostic))?;
                remap_bindings(&mut window.content, source, &text, &origins);
                return Ok(window);
            }
            if round == 8 {
                return Err(at_original(
                    source,
                    origins[nodes[0].range().start],
                    "Component expansion exceeds eight nested rounds".into(),
                ));
            }
            let mut replacements = Vec::new();
            for node in nodes {
                expanded += 1;
                let position = origins[node.range().start];
                if expanded > 256 {
                    return Err(at_original(
                        source,
                        position,
                        "Component expansion exceeds 256 invocations".into(),
                    ));
                }
                let namespace = node.tag_name().namespace().unwrap();
                let name = node.tag_name().name();
                let factory = self
                    .factories
                    .get(&(namespace.into(), name.into()))
                    .ok_or_else(|| {
                        at_original(
                            source,
                            position,
                            format!("Unregistered component {{{namespace}}}{name}"),
                        )
                    })?;
                let location = at_original(source, position, String::new());
                let first = node.first_child().map(|child| child.range().start);
                let last = node.last_child().map(|child| child.range().end);
                let markup = ComponentMarkup {
                    namespace: namespace.into(),
                    name: name.into(),
                    attributes: node
                        .attributes()
                        .map(|a| {
                            (
                                a.namespace().map_or_else(
                                    || a.name().into(),
                                    |ns| format!("{{{ns}}}{}", a.name()),
                                ),
                                a.value().into(),
                            )
                        })
                        .collect(),
                    content: match (first, last) {
                        (Some(start), Some(end)) => text[start..end].into(),
                        _ => String::new(),
                    },
                    line: location.line,
                    column: location.column,
                };
                let replacement = factory(&markup).map_err(|message| {
                    at_original(
                        source,
                        position,
                        message[..message.floor_char_boundary(4096)].to_owned(),
                    )
                })?;
                if replacement.len() > MAX_SOURCE_BYTES {
                    return Err(at_original(
                        source,
                        position,
                        "Component output exceeds source limit".into(),
                    ));
                }
                replacements.push((node.range(), replacement, position));
            }
            for (range, replacement, position) in replacements.into_iter().rev() {
                if text.len() - range.len() + replacement.len() > MAX_SOURCE_BYTES {
                    return Err(at_original(
                        source,
                        position,
                        "Expanded form exceeds source limit".into(),
                    ));
                }
                origins.splice(
                    range.clone(),
                    std::iter::repeat_n(position, replacement.len()),
                );
                text.replace_range(range, &replacement);
            }
        }
        unreachable!()
    }
    pub fn compile(&self, source: &str) -> Result<String, Diagnostic> {
        Ok(crate::codegen::window(&self.parse(source)?))
    }
}
pub(crate) fn remap_bindings(
    element: &mut crate::ElementDefinition,
    source: &str,
    expanded: &str,
    origins: &[usize],
) {
    for binding in element.common.bindings.values_mut() {
        let diagnostic = remap(
            source,
            expanded,
            origins,
            Diagnostic {
                line: binding.source_line,
                column: binding.source_column,
                message: String::new(),
            },
        );
        binding.source_line = diagnostic.line;
        binding.source_column = diagnostic.column;
    }
    for child in element.children_mut() {
        remap_bindings(child, source, expanded, origins);
    }
}
pub(crate) fn at_original(source: &str, offset: usize, message: String) -> Diagnostic {
    let before = &source[..source.floor_char_boundary(offset.min(source.len()))];
    let line = before.bytes().filter(|b| *b == b'\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    Diagnostic {
        line: line as u32,
        column: column as u32,
        message,
    }
}
pub(crate) fn remap(
    source: &str,
    expanded: &str,
    origins: &[usize],
    diagnostic: Diagnostic,
) -> Diagnostic {
    let mut line = 1;
    let mut column = 1;
    let mut offset = expanded.len();
    for (index, ch) in expanded.char_indices() {
        if line == diagnostic.line && column == diagnostic.column {
            offset = index;
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    at_original(
        source,
        origins[offset.min(origins.len() - 1)],
        diagnostic.message,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn factory_compiles_validated_components_and_keeps_original_locations() {
        let mut registry = ComponentRegistry::default();
        registry
            .register("urn:app", "Greeting", |input| {
                Ok(format!(
                    "<TextBlock xmlns=\"{UI_NAMESPACE}\" Text=\"{}\"/>",
                    input.attributes.get("Text").map_or("Hello", String::as_str)
                ))
            })
            .unwrap();
        let source = "<Window xmlns=\"urn:rust-ui-engine:ui\" xmlns:app=\"urn:app\">\n  <app:Greeting Text=\"Hello\"/>\n</Window>";
        assert!(registry.compile(source).unwrap().contains("Hello"));
        registry
            .register("urn:app", "Greeting", |_| {
                Ok("<TextBlock Text=\"{Binding Name}\"/>".into())
            })
            .unwrap();
        let window = registry.parse(source).unwrap();
        let binding = &window.content.common.bindings["Text"];
        assert_eq!((binding.source_line, binding.source_column), (2, 3));
        registry
            .register("urn:app", "Greeting", |_| {
                Ok("<TextBlock Invalid=\"1\"/>".into())
            })
            .unwrap();
        let error = registry.parse(source).unwrap_err();
        assert_eq!((error.line, error.column), (2, 3));
    }
    #[test]
    fn extension_names_cycles_and_invalid_output_are_rejected() {
        let mut registry = ComponentRegistry::default();
        assert!(
            registry
                .register(UI_NAMESPACE, "TextBlock", |_| Ok(String::new()))
                .is_err()
        );
        registry
            .register("urn:app", "Loop", |_| Ok("<app:Loop/>".into()))
            .unwrap();
        assert!(registry.parse("<Window xmlns=\"urn:rust-ui-engine:ui\" xmlns:app=\"urn:app\"><app:Loop/></Window>").unwrap_err().message.contains("eight"));
    }
}
