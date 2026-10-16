//! Presentation replacement keeps the behavioral owner and its binding engine.
use super::*;
use rust_desktop_ui_controls::{BindingContext, BindingConverterRegistry};
use rust_desktop_ui_core::Size;
use rust_desktop_ui_xaml::{ControlTemplateDefinition, ControlTemplateTarget};
use std::{cell::RefCell, rc::Rc};

pub(super) type PresentationContext = (
    Rc<RefCell<BindingContext>>,
    Rc<RefCell<BindingConverterRegistry>>,
);
#[derive(Clone, Copy)]
enum Owner {
    Button,
    EditorButton,
    Group,
    Tab,
}
pub(super) struct Presentation {
    root: WidgetId,
    owner: Owner,
    inherited_context: String,
    view: Box<MountedView>,
}
impl MountedView {
    pub(super) fn mount_editor_button_presentation(
        &mut self,
        ui: &mut Controls,
        owner: WidgetId,
        definition: &ElementDefinition,
        width: f32,
        inherited_context: &str,
    ) -> Result<(), String> {
        self.mount_presentation(
            ui,
            (owner, owner, Owner::EditorButton),
            definition,
            width,
            inherited_context,
        )
    }

    fn mount_presentation(
        &mut self,
        ui: &mut Controls,
        placement: (WidgetId, WidgetId, Owner),
        definition: &ElementDefinition,
        width: f32,
        inherited_context: &str,
    ) -> Result<(), String> {
        let (owner, header, kind) = placement;
        let definition = self.isolate_presentation(definition, kind);
        let mut view = Box::new(MountedView::default());
        let root = view.mount_element(
            ui,
            header,
            &definition,
            Axis::Horizontal,
            width,
            inherited_context,
        )?;
        attach(ui, owner, root, kind)?;
        self.automation_names.extend(view.automation_names.clone());
        self.roles.extend(view.roles.clone());
        self.presentations.insert(
            owner,
            Presentation {
                root,
                owner: kind,
                inherited_context: inherited_context.into(),
                view,
            },
        );
        Ok(())
    }
    pub(super) fn mount_control_presentation(
        &mut self,
        ui: &mut Controls,
        owner: WidgetId,
        template: &ControlTemplateDefinition,
        width: f32,
        inherited_context: &str,
    ) -> Result<(), String> {
        self.mount_presentation(
            ui,
            (owner, owner, Owner::Button),
            &template.visual,
            width,
            inherited_context,
        )
    }
    pub(super) fn mount_header_presentation(
        &mut self,
        ui: &mut Controls,
        owner: WidgetId,
        header: WidgetId,
        definition: &ElementDefinition,
        width: f32,
        inherited_context: &str,
    ) -> Result<(), String> {
        self.mount_presentation(
            ui,
            (owner, header, Owner::Group),
            definition,
            width,
            inherited_context,
        )
    }
    pub(super) fn mount_tab_header_presentation(
        &mut self,
        ui: &mut Controls,
        owner: WidgetId,
        header: WidgetId,
        definition: &ElementDefinition,
        width: f32,
        inherited_context: &str,
    ) -> Result<(), String> {
        self.mount_presentation(
            ui,
            (owner, header, Owner::Tab),
            definition,
            width,
            inherited_context,
        )
    }
    pub(super) fn initialize_templates(
        &mut self,
        ui: &mut Controls,
        context: Rc<RefCell<BindingContext>>,
        converters: Rc<RefCell<BindingConverterRegistry>>,
    ) -> Result<(), String> {
        self.presentation_context = Some((context.clone(), converters.clone()));
        for presentation in self.presentations.values_mut() {
            let own_names = presentation.view.names.clone();
            presentation.view.names.extend(self.names.clone());
            presentation.view.names.extend(own_names.clone());
            presentation
                .view
                .initialize_bindings(ui, context.clone(), converters.clone())?;
            presentation.view.names = own_names;
        }
        Ok(())
    }
    pub(super) fn sync_template_bindings(&mut self, ui: &mut Controls) -> bool {
        let mut changed = false;
        for presentation in self.presentations.values_mut() {
            changed |= presentation.view.sync_bindings(ui);
        }
        changed
    }
    pub(super) fn update_template_layout(
        &mut self,
        ui: &mut Controls,
        viewport: Size,
    ) -> Result<(), String> {
        for presentation in self.presentations.values_mut() {
            presentation.view.update_form_layout(ui, viewport)?;
        }
        Ok(())
    }
    /// Preflight, mount and initialize the candidate before touching the existing
    /// content. The owner, command, focus, tooltip and main binding engine survive.
    pub(crate) fn replace_control_template(
        &mut self,
        ui: &mut Controls,
        owner: WidgetId,
        template: &ControlTemplateDefinition,
    ) -> Result<(), String> {
        rust_desktop_ui_xaml::validate_control_template(template)?;
        let compatible = matches!(
            (self.roles.get(&owner), template.target),
            (
                Some(Role::Button | Role::Link),
                ControlTemplateTarget::Button
            ) | (
                Some(Role::ToggleButton),
                ControlTemplateTarget::ToggleButton
            )
        );
        if !compatible {
            return Err("ControlTemplate target does not match its behavioral owner".into());
        }
        self.replace_presentation(ui, owner, owner, &template.visual, Owner::Button)
    }
    /// The group's content subtree and disclosure owner are never remounted.
    pub(crate) fn replace_editor_button_template(
        &mut self,
        ui: &mut Controls,
        button: WidgetId,
        definition: &ElementDefinition,
    ) -> Result<(), String> {
        if ui.editor_button_settings(button).is_none() {
            return Err("Unknown embedded editor button".into());
        }
        self.replace_presentation(ui, button, button, definition, Owner::EditorButton)
    }

    /// The group's content subtree and disclosure owner are never remounted.
    pub(crate) fn replace_group_header_template(
        &mut self,
        ui: &mut Controls,
        owner: WidgetId,
        definition: &ElementDefinition,
    ) -> Result<(), String> {
        let header = ui
            .group_header_id(owner)
            .ok_or("Unknown template group owner")?;
        self.replace_presentation(ui, owner, header, definition, Owner::Group)
    }
    fn replace_presentation(
        &mut self,
        ui: &mut Controls,
        owner: WidgetId,
        header: WidgetId,
        definition: &ElementDefinition,
        kind: Owner,
    ) -> Result<(), String> {
        rust_desktop_ui_xaml::validate_header_content(definition)?;
        validate_tree(ui, definition)?;
        if !ui.runtime.tree().contains(owner) {
            return Err("Template owner has been removed".into());
        }
        let (context, converters) = self
            .presentation_context
            .clone()
            .ok_or("Template bindings are not initialized")?;
        let inherited = self
            .presentations
            .get(&owner)
            .map(|old| old.inherited_context.as_str())
            .or_else(|| {
                self.presentation_owner_contexts
                    .get(&owner)
                    .map(String::as_str)
            })
            .unwrap_or("")
            .to_owned();
        let width = ui.runtime.bounds(header).map_or(0.0, |bounds| bounds.width);
        let before: HashSet<_> = ui.runtime.tree().live_ids().into_iter().collect();
        let definition = self.isolate_presentation(definition, kind);
        let candidate = (|| {
            let mut view = Box::new(MountedView::default());
            let root =
                view.mount_element(ui, header, &definition, Axis::Horizontal, width, &inherited)?;
            // New template names override the previous instance only inside
            // the candidate resolver. Existing owner bindings are untouched.
            let own_names = view.names.clone();
            view.names = self.names.clone();
            view.names.extend(own_names.clone());
            view.initialize_bindings(ui, context, converters)?;
            view.names = own_names;
            Ok::<_, String>(Presentation {
                root,
                owner: kind,
                inherited_context: inherited,
                view,
            })
        })();
        let candidate = match candidate {
            Ok(candidate) => candidate,
            Err(error) => {
                remove_added(ui, &before)?;
                return Err(error);
            }
        };
        if let Err(error) = attach(ui, owner, candidate.root, kind) {
            remove_added(ui, &before)?;
            return Err(error);
        }
        let old = self.presentations.remove(&owner);
        if let Some(old) = old
            && ui.runtime.tree().contains(old.root)
            && let Err(error) = ui.remove(old.root)
        {
            attach(ui, owner, old.root, old.owner)?;
            remove_added(ui, &before)?;
            self.presentations.insert(owner, old);
            return Err(err(error));
        }
        self.names.retain(|_, id| ui.runtime.tree().contains(*id));
        self.automation_names
            .retain(|id, _| ui.runtime.tree().contains(*id));
        self.roles.retain(|id, _| ui.runtime.tree().contains(*id));
        self.automation_names
            .extend(candidate.view.automation_names.clone());
        self.roles.extend(candidate.view.roles.clone());
        self.presentations.insert(owner, candidate);
        Ok(())
    }
    fn isolate_presentation(
        &mut self,
        definition: &ElementDefinition,
        kind: Owner,
    ) -> ElementDefinition {
        self.presentation_revision = self.presentation_revision.wrapping_add(1);
        let prefix = format!("__live_template_{}_", self.presentation_revision);
        let mut definition = definition.clone();
        // Runtime replacements need the same defaults as compiled templates.
        // Explicit Stretch/Start/End remain meaningful even for fixed-size roots.
        let common = &mut definition.common;
        if !common.vertical_alignment_explicit
            && common.vertical_alignment == rust_desktop_ui_xaml::Alignment::Stretch
        {
            common.vertical_alignment = rust_desktop_ui_xaml::Alignment::Center;
        }
        if matches!(kind, Owner::EditorButton)
            && !common.horizontal_alignment_explicit
            && common.horizontal_alignment == rust_desktop_ui_xaml::Alignment::Stretch
        {
            common.horizontal_alignment = rust_desktop_ui_xaml::Alignment::Center;
        }
        let mut names = HashMap::new();
        let mut pending = vec![&mut definition];
        while let Some(element) = pending.pop() {
            if let Some(name) = &mut element.common.name {
                let isolated = format!("{prefix}{name}");
                names.insert(name.clone(), isolated.clone());
                *name = isolated;
            }
            pending.extend(element.children_mut());
        }
        let mut pending = vec![&mut definition];
        while let Some(element) = pending.pop() {
            for binding in element.common.bindings.values_mut() {
                if let Some(name) = &mut binding.element_name
                    && let Some(isolated) = names.get(name)
                {
                    *name = isolated.clone();
                }
                for input in &mut binding.inputs {
                    if let Some(name) = &mut input.element_name
                        && let Some(isolated) = names.get(name)
                    {
                        *name = isolated.clone();
                    }
                }
            }
            pending.extend(element.children_mut());
        }
        definition
    }
}
fn attach(ui: &mut Controls, owner: WidgetId, root: WidgetId, kind: Owner) -> Result<(), String> {
    match kind {
        Owner::Button => ui.set_button_content(owner, Some(root)),
        Owner::EditorButton => ui.set_editor_button_content(owner, Some(root)),
        Owner::Group => ui.set_group_header_content(owner, Some(root)),
        Owner::Tab => ui.set_tab_header_content(owner, Some(root)),
    }
    .map_err(err)
}
fn remove_added(ui: &mut Controls, before: &HashSet<WidgetId>) -> Result<(), String> {
    let roots: Vec<_> = ui
        .runtime
        .tree()
        .live_ids()
        .into_iter()
        .filter(|id| {
            !before.contains(id)
                && ui
                    .runtime
                    .tree()
                    .parent(*id)
                    .is_none_or(|parent| before.contains(&parent))
        })
        .collect();
    for root in roots {
        ui.remove(root).map_err(err)?;
    }
    Ok(())
}
