//! Hierarchy definitions resolve only explicitly registered Rust sources.
use super::*;
use rust_desktop_ui_controls::{SelectionMode, TreeControlSettings};
use rust_desktop_ui_treegrid::{ColumnId, GridColumn, RowKey, TreeGrid};
use rust_desktop_ui_xaml::TreeDefinition;

fn settings(definition: &TreeDefinition, header: bool) -> TreeControlSettings {
    TreeControlSettings {
        columns: if header {
            definition
                .columns
                .iter()
                .map(|column| GridColumn {
                    id: ColumnId(u64::from(column.id)),
                    title: column.header.clone(),
                    width: column.width,
                    min_width: column.min_width,
                    max_width: column.max_width,
                    pinned: column.is_pinned,
                    editable: false,
                })
                .collect()
        } else {
            vec![GridColumn::new(
                ColumnId(u64::from(definition.display_column)),
                "",
                240.0,
            )]
        },
        show_header: header,
        selection_mode: match definition.selection_mode {
            rust_desktop_ui_xaml::SelectionMode::Single => SelectionMode::Single,
            rust_desktop_ui_xaml::SelectionMode::Multiple => SelectionMode::Multiple,
        },
        allow_node_drag: definition.allow_node_drag,
        read_only: definition.is_read_only,
    }
}

pub(super) fn validate_source(
    ui: &Controls,
    definition: &TreeDefinition,
    header: bool,
    common: &rust_desktop_ui_xaml::CommonProperties,
) -> Result<(), String> {
    rust_desktop_ui_xaml::validate_tree(definition, header, common)?;
    let mut source = ui
        .tree_source(&definition.source)
        .ok_or_else(|| format!("Unknown tree source '{}'", definition.source))?;
    source.check_available()?;
    let mut preview = TreeGrid::new(settings(definition, header).columns).map_err(err)?;
    preview.refresh(&mut source).map_err(err)?;
    if let Some(key) = &definition.selected_key {
        preview
            .assign_selected_key(&source, Some(RowKey(key.parse().map_err(err)?)))
            .map_err(err)?;
    }
    Ok(())
}
impl MountedView {
    pub(super) fn mount_tree(
        &mut self,
        ui: &mut Controls,
        parent: WidgetId,
        definition: &TreeDefinition,
        header: bool,
        mut style: LayoutStyle,
        height_bound: bool,
    ) -> Result<WidgetId, String> {
        let source = ui
            .tree_source(&definition.source)
            .ok_or_else(|| format!("Unknown tree source '{}'", definition.source))?;
        if style.height == Length::Auto
            && style.vertical_alignment == Some(rust_desktop_ui_core::Align::Stretch)
            && !height_bound
            && ui
                .runtime
                .tree()
                .node(parent)
                .is_some_and(|node| node.props.style.kind == LayoutKind::Grid)
        {
            style.height = Length::Percent(1.0);
        }
        let id = ui
            .tree_grid(parent, source, settings(definition, header), style)
            .map_err(err)?;
        if let Some(key) = &definition.selected_key {
            ui.set_tree_selected_key(id, Some(RowKey(key.parse().map_err(err)?)))
                .map_err(err)?;
        }
        Ok(id)
    }
}

#[cfg(test)]
use crate::tree_model as demo;

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_controls::{BindingContext, DataValue, Theme, TreeSourceHandle};
    use rust_desktop_ui_core::Size;
    use rust_desktop_ui_treegrid::{IndexedTreeSource, TreeNode};
    use std::{cell::RefCell, rc::Rc};
    fn definition(body: &str) -> ElementDefinition {
        rust_desktop_ui_xaml::parse(&format!("<Window xmlns='urn:rust-ui-engine:ui' xmlns:x='http://schemas.microsoft.com/winfx/2006/xaml'>{body}</Window>")).unwrap().content
    }
    #[test]
    fn demo_controls_lazy_completion_cancel_exact_selection_and_shared_source() {
        let definition = rust_desktop_ui_xaml::parse(include_str!("../TreeWindow.xaml")).unwrap();
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        ui.commands_mut().register("Light", "Light");
        ui.commands_mut().register("Dark", "Dark");
        let source = demo::register(&mut ui).unwrap();
        let context = demo::model().unwrap();
        let root = ui.root();
        let mut view = MountedView::mount_with_context(
            &mut ui,
            root,
            &definition.content,
            Rc::clone(&context),
        )
        .unwrap();
        let mut state = demo::State::install(&mut ui, &view, Rc::clone(&source))
            .unwrap()
            .unwrap();
        ui.update(Size::new(1200.0, 760.0)).unwrap();
        let initial = source.borrow().nodes().len();
        assert!(state.command(&mut ui, "TreeBeginLoad").unwrap());
        state.command(&mut ui, "TreeCancelLoad").unwrap();
        state.command(&mut ui, "TreeCompleteLoad").unwrap();
        assert_eq!(source.borrow().nodes().len(), initial);
        state.command(&mut ui, "TreeBeginLoad").unwrap();
        state.command(&mut ui, "TreeCompleteLoad").unwrap();
        assert_eq!(source.borrow().nodes().len(), initial + 2);
        context
            .borrow_mut()
            .set("SelectedNode", u64::MAX.to_string().into())
            .unwrap();
        view.bindings.as_mut().unwrap().sync(&mut ui);
        assert_eq!(
            ui.tree_selected_keys(view.find_name("Hierarchy").unwrap())
                .unwrap(),
            [RowKey(u64::MAX)]
        );
        state.command(&mut ui, "TreeRename").unwrap();
        state.command(&mut ui, "TreeReorderColumns").unwrap();
        state.command(&mut ui, "TreePinColumn").unwrap();
        state.command(&mut ui, "TreeResizeColumn").unwrap();
        ui.update(Size::new(1200.0, 760.0)).unwrap();
        state.refresh(&mut ui).unwrap();
        assert!(
            ui.label_text(view.find_name("TreeSummary").unwrap())
                .unwrap()
                .contains("принятых ответов: 1")
        );
        state.command(&mut ui, "TreeReset").unwrap();
        assert_eq!(source.borrow().nodes().len(), initial);
    }
    #[test]
    fn compiled_tree_view_and_grid_mount_shared_source_and_selected_key_binding() {
        for header in [false, true] {
            let mut ui = Controls::new(Theme::FluentLight).unwrap();
            let source = Rc::new(RefCell::new(
                IndexedTreeSource::new(vec![
                    TreeNode::new(RowKey(1), None, "Parent"),
                    TreeNode::new(RowKey(u64::MAX), Some(RowKey(1)), "Child"),
                ])
                .unwrap(),
            ));
            ui.register_tree_source("Nodes", TreeSourceHandle::from_shared(source))
                .unwrap();
            let body = if header {
                "<TreeGrid x:Name='NodesView' Source='Nodes' SelectedKey='{Binding Selected}' Width='400' Height='240'><TreeColumn Id='0' Header='Name'/><TreeColumn Id='1' Header='Value' Width='120'/></TreeGrid>"
            } else {
                "<TreeView x:Name='NodesView' Source='Nodes' SelectedKey='{Binding Selected}' Width='400' Height='240'/>"
            };
            let definition = definition(body);
            let context = Rc::new(RefCell::new(
                BindingContext::new(DataValue::object([(
                    "Selected",
                    u64::MAX.to_string().into(),
                )]))
                .unwrap(),
            ));
            let root = ui.root();
            let mut view =
                MountedView::mount_with_context(&mut ui, root, &definition, Rc::clone(&context))
                    .unwrap();
            let id = view.find_name("NodesView").unwrap();
            ui.update(Size::new(640.0, 480.0)).unwrap();
            assert_eq!(ui.tree_selected_keys(id).unwrap(), [RowKey(u64::MAX)]);
            ui.tree_set_expanded(id, RowKey(1), true).unwrap();
            ui.update(Size::new(640.0, 480.0)).unwrap();
            assert_eq!(ui.tree_frame(id).unwrap().semantics.rows.len(), 2);
            assert_eq!(ui.tree_settings(id).unwrap().show_header, header);
            ui.set_tree_selected_key(id, Some(RowKey(1))).unwrap();
            view.bindings.as_mut().unwrap().sync(&mut ui);
            assert_eq!(
                context.borrow().get("Selected").unwrap(),
                &DataValue::from("1")
            );
        }
    }
    #[test]
    fn unknown_source_key_and_invalid_manual_metadata_create_no_retained_hosts() {
        let mut ui = Controls::new(Theme::FluentLight).unwrap();
        let root = ui.root();
        let before = ui.runtime.tree().live_ids();
        let missing = definition("<TreeView Source='Missing'/>");
        assert!(MountedView::mount(&mut ui, root, &missing).is_err());
        ui.register_tree_source(
            "Nodes",
            TreeSourceHandle::new(
                IndexedTreeSource::new(vec![TreeNode::new(RowKey(1), None, "One")]).unwrap(),
            ),
        )
        .unwrap();
        let unknown = definition("<TreeView Source='Nodes' SelectedKey='2'/>");
        assert!(MountedView::mount(&mut ui, root, &unknown).is_err());
        let mut invalid = definition("<TreeGrid Source='Nodes'><TreeColumn Id='0'/></TreeGrid>");
        let ElementKind::TreeGrid(tree) = &mut invalid.kind else {
            panic!()
        };
        tree.columns[0].width = f32::NAN;
        assert!(MountedView::mount(&mut ui, root, &invalid).is_err());
        assert_eq!(ui.runtime.tree().live_ids(), before);
    }
}
