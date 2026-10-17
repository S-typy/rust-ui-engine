use super::*;
use rust_desktop_ui_controls::{SelectionMode, Theme, TileItem, TileModel, TileSettings};
use rust_desktop_ui_core::{LayoutStyle, Length, Size};
#[test]
fn tiles_project_separate_selection_and_invoke_with_stable_set_position() {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    ui.commands_mut().register("open", "Open");
    let mut item = TileItem::new("a", "Alpha");
    item.command = Some("open".into());
    let mut disabled = TileItem::new("b", "Disabled");
    disabled.item.enabled = false;
    let owner = ui
        .tile_collection(
            ui.root(),
            "Tiles",
            TileModel::new(Vec::new(), vec![item, disabled]).unwrap(),
            TileSettings {
                selection: SelectionMode::Multiple,
                ..Default::default()
            },
            LayoutStyle {
                width: Length::Px(500.0),
                height: Length::Px(220.0),
                ..Default::default()
            },
        )
        .unwrap()
        .container;
    ui.update(Size::new(600.0, 400.0)).unwrap();
    let row = ui.tile_item_widget(owner, "a").unwrap();
    let button = ui.tile_invoke_widget(owner, "a").unwrap();
    let view = MountedView::default();
    let mut bridge = Accessibility::default();
    let tree = bridge.tree(&ui, &view, "Tiles", 1.0);
    let node = |id| {
        &tree
            .nodes
            .iter()
            .find(|(n, _)| *n == bridge.id(id).unwrap())
            .unwrap()
            .1
    };
    assert!(node(owner).is_multiselectable());
    assert_eq!(node(row).role(), Role::ListBoxOption);
    assert_eq!(node(row).is_selected(), Some(false));
    assert_eq!(node(row).position_in_set(), Some(0));
    assert_eq!(node(row).size_of_set(), Some(2));
    assert_eq!(node(button).role(), Role::Button);
    assert_eq!(node(button).is_selected(), None);
    assert!(node(button).supports_action(Action::Click));
    assert!(!node(ui.tile_invoke_widget(owner, "b").unwrap()).supports_action(Action::Click));
    for (widget, count) in [(row, 0), (button, 1)] {
        bridge
            .action(
                &mut ui,
                accesskit::ActionRequest {
                    action: Action::Click,
                    target_tree: accesskit::TreeId::ROOT,
                    target_node: bridge.id(widget).unwrap(),
                    data: None,
                },
            )
            .unwrap();
        assert_eq!(ui.commands_mut().drain_invocations().len(), count);
        assert_eq!(ui.tile_selected_keys(owner).unwrap(), ["a"]);
    }
}
