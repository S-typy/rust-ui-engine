use super::*;
use rust_desktop_ui_controls::{SearchBox, SearchSuggestion, Theme};
use rust_desktop_ui_core::{InputEvent, Key, LayoutStyle, Length, Modifiers, Mutation, Size};

const SIZE: Size = Size::new(800.0, 600.0);

fn fixture() -> (Controls, SearchBox, MountedView, Accessibility) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let parts = ui
        .search_box(
            ui.root(),
            "Поиск документов",
            "",
            Default::default(),
            LayoutStyle {
                width: Length::Px(440.0),
                height: Length::Px(36.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.update(SIZE).unwrap();
    ui.runtime
        .apply(Mutation::Focus(Some(parts.editor)))
        .unwrap();
    (ui, parts, MountedView::default(), Accessibility::default())
}

fn suggestions(ui: &mut Controls, parts: SearchBox) {
    ui.set_search_query(parts.container, "план").unwrap();
    let request = ui.drain_search_suggestion_requests().pop().unwrap();
    let mut disabled = SearchSuggestion::new("disabled", "Недоступная подсказка");
    disabled.enabled = false;
    ui.complete_search_suggestions(
        &request,
        Ok(vec![
            disabled,
            SearchSuggestion::new("one", "План разработки"),
            SearchSuggestion::new("two", "План поездки"),
        ]),
    )
    .unwrap();
    ui.update(SIZE).unwrap();
}

fn key(ui: &mut Controls, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
    ui.update(SIZE).unwrap();
}

fn node(update: &accesskit::TreeUpdate, id: NodeId) -> &Node {
    &update
        .nodes
        .iter()
        .find(|(candidate, _)| *candidate == id)
        .unwrap()
        .1
}

#[test]
fn search_active_descendant_tracks_suggestions_without_moving_edit_focus() {
    let (mut ui, parts, view, mut a11y) = fixture();
    suggestions(&mut ui, parts);
    let update = a11y.tree(&ui, &view, "Search", 1.5);
    let editor = a11y.id(parts.editor).unwrap();
    assert_eq!(update.focus, editor);
    assert_eq!(node(&update, editor).role(), Role::SearchInput);
    assert_eq!(node(&update, editor).active_descendant(), None);
    assert_eq!(node(&update, editor).is_expanded(), Some(true));
    assert_eq!(
        node(&update, editor).controls(),
        &[a11y.id(parts.list.container).unwrap()]
    );
    for (expected_key, expected_label) in [("one", "План разработки"), ("two", "План поездки")]
    {
        key(&mut ui, Key::ArrowDown);
        let update = a11y.tree(&ui, &view, "Search", 1.5);
        let row = ui
            .list_row_widget(parts.list.container, expected_key)
            .unwrap();
        let active = a11y.id(row).unwrap();
        assert_eq!(update.focus, editor);
        assert_eq!(ui.runtime.focused(), Some(parts.editor));
        assert_eq!(ui.search_query(parts.container), Some("план"));
        assert_eq!(node(&update, editor).active_descendant(), Some(active));
        assert_eq!(
            node(&update, active).label().map(str::trim),
            Some(expected_label)
        );
        assert_eq!(node(&update, active).is_selected(), Some(true));
    }
    key(&mut ui, Key::Escape);
    let update = a11y.tree(&ui, &view, "Search", 1.5);
    assert_eq!(node(&update, editor).active_descendant(), None);
    assert_eq!(node(&update, editor).is_expanded(), Some(false));
    assert!(node(&update, editor).controls().is_empty());
    assert_eq!(update.focus, editor);
    assert_eq!(ui.search_query(parts.container), Some("план"));
}

#[test]
fn search_active_descendant_is_cleared_on_query_reset_blur_and_owner_removal() {
    let (mut ui, parts, view, mut a11y) = fixture();
    suggestions(&mut ui, parts);
    key(&mut ui, Key::ArrowDown);
    let update = a11y.tree(&ui, &view, "Search", 1.0);
    let editor = a11y.id(parts.editor).unwrap();
    assert!(node(&update, editor).active_descendant().is_some());
    ui.set_search_query(parts.container, "").unwrap();
    ui.update(SIZE).unwrap();
    let update = a11y.tree(&ui, &view, "Search", 1.0);
    assert_eq!(node(&update, editor).active_descendant(), None);
    suggestions(&mut ui, parts);
    key(&mut ui, Key::ArrowDown);
    ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
    ui.update(SIZE).unwrap();
    let update = a11y.tree(&ui, &view, "Search", 1.0);
    assert_eq!(node(&update, editor).active_descendant(), None);
    assert_ne!(update.focus, editor);
    ui.remove(parts.container).unwrap();
    ui.update(SIZE).unwrap();
    let update = a11y.tree(&ui, &view, "Search", 1.0);
    assert!(
        update
            .nodes
            .iter()
            .all(|(_, node)| node.active_descendant().is_none())
    );
    assert!(a11y.id(parts.editor).is_none());
}
