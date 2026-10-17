//! Declarative retained workflows; application guards decide transitions.
use crate::*;
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, PartialEq)]
pub struct WizardStepDefinition {
    pub id: String,
    pub header: String,
    pub next: Vec<String>,
    pub is_final: bool,
    pub content: Box<ElementDefinition>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct WizardDefinition {
    pub start_step: String,
    pub show_progress: bool,
    pub allow_cancel: bool,
    pub is_read_only: bool,
    pub steps: Vec<WizardStepDefinition>,
}
impl Default for WizardDefinition {
    fn default() -> Self {
        Self {
            start_step: String::new(),
            show_progress: true,
            allow_cancel: true,
            is_read_only: false,
            steps: vec![],
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StepStateDefinition {
    #[default]
    Upcoming,
    Current,
    Completed,
    Error,
    Skipped,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepDefinition {
    pub id: String,
    pub header: String,
    pub state: StepStateDefinition,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StepProgressDefinition {
    pub current_step: String,
    pub steps: Vec<StepDefinition>,
}

fn valid_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control)
}
fn valid_header(s: &str) -> bool {
    !s.trim().is_empty()
        && s.len() <= 512
        && s.graphemes(true).count() <= 128
        && !s.chars().any(char::is_control)
        && !s.contains(['\u{2028}', '\u{2029}'])
}
fn common_bindings(common: &CommonProperties, readonly: bool) -> Result<(), String> {
    if common.bindings.keys().any(|p| {
        !(badges::binding_property(p)
            || matches!(p.as_str(), "IsEnabled" | "Visibility" | "Width" | "Height")
            || (readonly && p == "IsReadOnly"))
    }) {
        return Err("Workflow metadata is literal; Wizard transitions require application requests, not selection bindings".into());
    }
    Ok(())
}
pub fn validate_wizard(value: &WizardDefinition, common: &CommonProperties) -> Result<(), String> {
    common_bindings(common, true)?;
    if !(1..=64).contains(&value.steps.len()) {
        return Err("Wizard requires 1..64 steps".into());
    }
    let mut ids = HashMap::new();
    let mut bytes = 0usize;
    for (i, s) in value.steps.iter().enumerate() {
        if !valid_id(&s.id)
            || !valid_header(&s.header)
            || ids.insert(&s.id, i).is_some()
            || s.next.len() > 64
            || s.is_final != s.next.is_empty()
        {
            return Err("Wizard steps require unique IDs, bounded headers and targets; only final steps have no targets".into());
        }
        bytes += s.id.len() + s.header.len() + s.next.iter().map(String::len).sum::<usize>();
    }
    let start = if value.start_step.is_empty() {
        &value.steps[0].id
    } else {
        &value.start_step
    };
    let Some(&start_index) = ids.get(start) else {
        return Err("Unknown StartStep".into());
    };
    if bytes > 32768 {
        return Err("Wizard metadata exceeds32KiB".into());
    }
    let mut edges = vec![vec![]; value.steps.len()];
    let mut indegree = vec![0usize; value.steps.len()];
    for (i, s) in value.steps.iter().enumerate() {
        let mut unique = HashSet::new();
        for target in &s.next {
            let Some(&j) = ids.get(target) else {
                return Err("Unknown wizard Next target".into());
            };
            if i == j || !unique.insert(j) {
                return Err("Duplicate or self-referencing wizard target".into());
            }
            edges[i].push(j);
            indegree[j] += 1;
        }
    }
    let mut queue: Vec<_> = indegree
        .iter()
        .enumerate()
        .filter_map(|(i, n)| (*n == 0).then_some(i))
        .collect();
    let mut visited = 0;
    while let Some(i) = queue.pop() {
        visited += 1;
        for &j in &edges[i] {
            indegree[j] -= 1;
            if indegree[j] == 0 {
                queue.push(j);
            }
        }
    }
    if visited != value.steps.len() {
        return Err("Wizard Next graph must be acyclic".into());
    }
    let mut seen = HashSet::new();
    let mut queue = vec![start_index];
    while let Some(i) = queue.pop() {
        if seen.insert(i) {
            queue.extend(&edges[i]);
        }
    }
    if seen.len() != value.steps.len() {
        return Err("Every wizard step must be reachable from StartStep".into());
    }
    Ok(())
}
pub fn validate_step_progress(
    value: &StepProgressDefinition,
    common: &CommonProperties,
) -> Result<(), String> {
    common_bindings(common, false)?;
    let mut ids = HashSet::new();
    if !(1..=64).contains(&value.steps.len())
        || value.steps.iter().any(|s| {
            !valid_id(&s.id)
                || !valid_header(&s.header)
                || !ids.insert(&s.id)
                || (matches!(
                    s.state,
                    StepStateDefinition::Current | StepStateDefinition::Error
                ) && s.id != value.current_step)
        })
        || !value.steps.iter().any(|s| {
            s.id == value.current_step
                && matches!(
                    s.state,
                    StepStateDefinition::Current
                        | StepStateDefinition::Completed
                        | StepStateDefinition::Error
                )
        })
        || value
            .steps
            .iter()
            .map(|s| s.id.len() + s.header.len())
            .sum::<usize>()
            > 32768
    {
        return Err("StepProgress requires1..64 unique bounded steps and an existing CurrentStep with Current/Completed/Error state".into());
    }
    Ok(())
}
pub(crate) fn property(
    document: &Document<'_>,
    attr: Attribute<'_, '_>,
    value: &mut WizardDefinition,
) -> Result<bool, Diagnostic> {
    match attr.name() {
        "StartStep" => value.start_step = attr.value().into(),
        "ShowProgress" => value.show_progress = boolean(document, attr)?,
        "AllowCancel" => value.allow_cancel = boolean(document, attr)?,
        "IsReadOnly" => value.is_read_only = boolean(document, attr)?,
        _ => return Ok(false),
    }
    Ok(true)
}
pub(crate) fn progress_property(
    _document: &Document<'_>,
    attr: Attribute<'_, '_>,
    value: &mut StepProgressDefinition,
) -> Result<bool, Diagnostic> {
    if attr.name() == "CurrentStep" {
        value.current_step = attr.value().into();
        Ok(true)
    } else {
        Ok(false)
    }
}
pub(crate) fn parse_steps(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
    names: &mut HashSet<String>,
    checked: &mut HashSet<RadioGroupKey>,
    value: &mut WizardDefinition,
) -> Result<(), Diagnostic> {
    for step in node.children() {
        if !step.is_element() {
            require_empty_content(step)?;
            continue;
        }
        if step.tag_name().namespace() != Some(UI_NAMESPACE)
            || step.tag_name().name() != "WizardStep"
        {
            return Err(at_node(step, "Wizard accepts WizardStep children"));
        }
        metadata_limit(step, depth + 1, count)?;
        if value.steps.len() >= 64 {
            return Err(at_node(step, "At most64 wizard steps are supported"));
        }
        let parent = *count;
        let mut item = WizardStepDefinition {
            id: String::new(),
            header: String::new(),
            next: vec![],
            is_final: false,
            content: Box::default(),
        };
        for attr in step.attributes() {
            if attr.namespace().is_some() {
                return Err(at_attribute(
                    node.document(),
                    attr,
                    "Name step content instead of WizardStep metadata",
                ));
            }
            reject_extension(node.document(), attr)?;
            match attr.name() {
                "Id" => item.id = attr.value().into(),
                "Header" => item.header = attr.value().into(),
                "Next" => {
                    item.next = if attr.value().is_empty() {
                        vec![]
                    } else {
                        attr.value()
                            .split(',')
                            .map(|s| s.trim().to_owned())
                            .collect()
                    }
                }
                "IsFinal" => item.is_final = boolean(node.document(), attr)?,
                _ => {
                    return Err(at_attribute(
                        node.document(),
                        attr,
                        "Unsupported WizardStep attribute",
                    ));
                }
            }
        }
        let mut content = None;
        for child in step.children() {
            if child.is_element() {
                if content.is_some() {
                    return Err(at_node(
                        child,
                        "WizardStep requires exactly one content element",
                    ));
                }
                content = Some(Box::new(parse_element(
                    child,
                    depth + 2,
                    parent,
                    count,
                    names,
                    checked,
                )?));
            } else {
                require_empty_content(child)?;
            }
        }
        item.content = content
            .ok_or_else(|| at_node(step, "WizardStep requires exactly one content element"))?;
        value.steps.push(item);
    }
    Ok(())
}
pub(crate) fn parse_progress(
    node: Node<'_, '_>,
    depth: usize,
    count: &mut usize,
    value: &mut StepProgressDefinition,
) -> Result<(), Diagnostic> {
    for step in node.children() {
        if !step.is_element() {
            require_empty_content(step)?;
            continue;
        }
        if step.tag_name().namespace() != Some(UI_NAMESPACE) || step.tag_name().name() != "Step" {
            return Err(at_node(step, "StepProgress accepts Step metadata"));
        }
        metadata_limit(step, depth + 1, count)?;
        if value.steps.len() >= 64 {
            return Err(at_node(step, "At most64 progress steps are supported"));
        }
        let mut item = StepDefinition {
            id: String::new(),
            header: String::new(),
            state: StepStateDefinition::Upcoming,
        };
        for attr in step.attributes() {
            if attr.namespace().is_some() {
                return Err(at_attribute(
                    node.document(),
                    attr,
                    "Step directives are unsupported",
                ));
            }
            reject_extension(node.document(), attr)?;
            match attr.name() {
                "Id" => item.id = attr.value().into(),
                "Header" => item.header = attr.value().into(),
                "State" => {
                    item.state = match attr.value() {
                        "Upcoming" => StepStateDefinition::Upcoming,
                        "Current" => StepStateDefinition::Current,
                        "Completed" => StepStateDefinition::Completed,
                        "Error" => StepStateDefinition::Error,
                        "Skipped" => StepStateDefinition::Skipped,
                        _ => return Err(at_attribute(node.document(), attr, "Unknown step state")),
                    }
                }
                _ => {
                    return Err(at_attribute(
                        node.document(),
                        attr,
                        "Unsupported Step attribute",
                    ));
                }
            }
        }
        for child in step.children() {
            if child.is_element() {
                return Err(at_node(child, "Step metadata cannot contain controls"));
            }
            require_empty_content(child)?;
        }
        value.steps.push(item);
    }
    Ok(())
}
fn metadata_limit(node: Node<'_, '_>, depth: usize, count: &mut usize) -> Result<(), Diagnostic> {
    *count += 1;
    if *count > MAX_ELEMENTS || depth > MAX_ELEMENT_DEPTH {
        Err(at_node(node, "Workflow metadata count/depth exceeded"))
    } else {
        Ok(())
    }
}
pub(crate) fn code(value: &WizardDefinition) -> String {
    let steps=value.steps.iter().map(|s|format!("rust_desktop_ui_xaml::WizardStepDefinition{{id:{:?}.into(),header:{:?}.into(),next:vec![{}],is_final:{},content:Box::new({})}}",s.id,s.header,s.next.iter().map(|s|format!("{s:?}.into()")).collect::<Vec<_>>().join(","),s.is_final,codegen::element(&s.content))).collect::<Vec<_>>().join(",");
    format!(
        "Box::new(rust_desktop_ui_xaml::WizardDefinition{{start_step:{:?}.into(),show_progress:{},allow_cancel:{},is_read_only:{},steps:vec![{}]}})",
        value.start_step, value.show_progress, value.allow_cancel, value.is_read_only, steps
    )
}
pub(crate) fn progress_code(value: &StepProgressDefinition) -> String {
    format!("rust_desktop_ui_xaml::StepProgressDefinition{{current_step:{:?}.into(),steps:vec![{}]}}",value.current_step,value.steps.iter().map(|s|format!("rust_desktop_ui_xaml::StepDefinition{{id:{:?}.into(),header:{:?}.into(),state:rust_desktop_ui_xaml::StepStateDefinition::{:?}}}",s.id,s.header,s.state)).collect::<Vec<_>>().join(","))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn document(body: &str) -> Result<WindowDefinition, Diagnostic> {
        parse(&format!(
            "<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'>{body}</Window>"
        ))
    }
    const FLOW: &str = "<Wizard StartStep='start'><WizardStep Id='start' Header='Начало' Next='remote,local'><TextBox x:Name='Draft'/></WizardStep><WizardStep Id='remote' Header='Сеть' Next='done'><TextBlock Text='Remote'/></WizardStep><WizardStep Id='local' Header='Локально' Next='done'><TextBlock Text='Local'/></WizardStep><WizardStep Id='done' Header='Готово' IsFinal='True'><TextBlock Text='Review'/></WizardStep></Wizard>";
    #[test]
    fn wizard_compiled_branch_graph_preserves_named_retained_content_and_codegen() {
        let value = document(FLOW).unwrap();
        let ElementKind::Wizard(w) = &value.content.kind else {
            panic!()
        };
        assert_eq!(w.steps[0].next, ["remote", "local"]);
        assert_eq!(value.content.children().len(), 4);
        assert_eq!(
            value.content.children()[0].common.name.as_deref(),
            Some("Draft")
        );
        let code = codegen::window(&value);
        assert!(code.contains("WizardStepDefinition"));
        assert!(code.contains("next:vec![\"remote\".into(),\"local\".into()]"));
    }
    #[test]
    fn wizard_rejects_cycles_unreachable_duplicate_targets_dynamic_metadata_and_selection_bypass() {
        for bad in [
            FLOW.replace("Next='done'", "Next='start'"),
            FLOW.replace("remote,local", "remote"),
            FLOW.replace("remote,local", "remote,remote"),
            FLOW.replace("remote,local", "unknown"),
            FLOW.replace("IsFinal='True'", "IsFinal='False'"),
            FLOW.replace("StartStep='start'", "StartStep='absent'"),
            FLOW.replace("Header='Сеть'", "Header='{Binding Caption}'"),
            FLOW.replace("StartStep='start'", "CurrentStep='{Binding Current}'"),
            FLOW.replace("<TextBox x:Name='Draft'/>", "<TextBox/><TextBox/>"),
        ] {
            assert!(document(&bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn step_progress_compiles_readonly_states_and_rejects_multiple_current() {
        let body = "<StepProgress CurrentStep='b'><Step Id='a' Header='Первый' State='Completed'/><Step Id='b' Header='Второй' State='Error'/><Step Id='c' Header='Третий'/></StepProgress>";
        let w = document(body).unwrap();
        let ElementKind::StepProgress(s) = &w.content.kind else {
            panic!()
        };
        assert_eq!(s.steps[1].state, StepStateDefinition::Error);
        assert!(w.content.children().is_empty());
        assert!(codegen::window(&w).contains("StepStateDefinition::Error"));
        for bad in [
            body.replace("State='Completed'", "State='Current'"),
            body.replace("CurrentStep='b'", "CurrentStep='missing'"),
            body.replace("CurrentStep='b'", "CurrentStep='{Binding Current}'"),
            body.replace(
                "<Step Id='c' Header='Третий'/>",
                "<Step Id='c' Header='Третий'><Button/></Step>",
            ),
        ] {
            assert!(document(&bad).is_err(), "{bad}");
        }
    }
}
