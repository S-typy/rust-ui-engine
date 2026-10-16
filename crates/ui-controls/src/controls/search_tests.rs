use super::*;
use rust_desktop_ui_core::Modifiers;

fn frame(ui: &mut Controls) {
    ui.update(Size::new(800.0, 600.0)).unwrap();
}
fn focus(ui: &mut Controls, id: WidgetId) {
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    frame(ui);
}
fn key(ui: &mut Controls, key: Key) {
    ui.dispatch(InputEvent::KeyDown {
        key,
        modifiers: Modifiers::default(),
        repeat: false,
    })
    .unwrap();
}
fn fixture(settings: SearchBoxSettings) -> (Controls, SearchBox) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let search = ui
        .search_box(
            root,
            "Search documents",
            "",
            settings,
            LayoutStyle {
                width: Length::Px(440.0),
                height: Length::Px(36.0),
                flex_shrink: 0.0,
                ..Default::default()
            },
        )
        .unwrap();
    focus(&mut ui, search.editor);
    (ui, search)
}
fn request(ui: &mut Controls, parts: SearchBox, query: &str) -> SearchSuggestionRequest {
    ui.set_search_query(parts.container, query).unwrap();
    ui.drain_search_suggestion_requests().pop().unwrap()
}
fn complete(ui: &mut Controls, request: &SearchSuggestionRequest, values: Vec<SearchSuggestion>) {
    assert_eq!(
        ui.complete_search_suggestions(request, Ok(values)).unwrap(),
        SearchCompletion::Applied
    );
    frame(ui);
}

#[test]
fn search_latest_completion_keeps_editor_focus_and_rejects_late_results() {
    let (mut ui, p) = fixture(Default::default());
    let old = request(&mut ui, p, "a");
    let current = request(&mut ui, p, "al");
    assert!(old.is_cancelled());
    complete(&mut ui, &current, vec![SearchSuggestion::new("a", "Alpha")]);
    assert_eq!(ui.runtime.focused(), Some(p.editor));
    assert!(ui.search_popup_is_open(p.container));
    assert_eq!(
        ui.complete_search_suggestions(&old, Ok(vec![SearchSuggestion::new("b", "Beta")]))
            .unwrap(),
        SearchCompletion::Stale
    );
    assert_eq!(ui.search_suggestions(p.container).unwrap()[0].key, "a");
    assert_eq!(ui.search_query(p.container), Some("al"));
    assert!(ui.drain_search_events().is_empty());
}

#[test]
fn search_arrow_skips_disabled_and_choice_does_not_search_until_explicit_enter() {
    let (mut ui, p) = fixture(Default::default());
    let token = request(&mut ui, p, "a");
    let mut disabled = SearchSuggestion::new("disabled", "Forbidden");
    disabled.enabled = false;
    complete(
        &mut ui,
        &token,
        vec![
            disabled,
            SearchSuggestion::new("a", "Alpha"),
            SearchSuggestion::new("b", "Beta"),
        ],
    );
    key(&mut ui, Key::ArrowDown);
    assert_eq!(ui.selected_keys(p.list.container).unwrap(), ["a"]);
    assert_eq!(ui.runtime.focused(), Some(p.editor));
    assert_eq!(ui.search_query(p.container), Some("a"));
    key(&mut ui, Key::Enter);
    assert_eq!(ui.search_query(p.container), Some("Alpha"));
    assert!(!ui.search_popup_is_open(p.container));
    assert!(
        matches!(&ui.drain_search_events()[..],[SearchEvent {kind:SearchEventKind::SuggestionChosen { key, query },..}] if key=="a" && query=="Alpha")
    );
    assert!(ui.search_history(p.container).unwrap().is_empty());
    key(&mut ui, Key::Enter);
    assert!(
        matches!(&ui.drain_search_events()[..],[SearchEvent {kind:SearchEventKind::SearchRequested { query },..}] if query=="Alpha")
    );
    assert_eq!(ui.search_history(p.container).unwrap(), ["Alpha"]);
}

#[test]
fn search_escape_first_cancels_preedit_then_closes_without_resetting_draft() {
    let (mut ui, p) = fixture(Default::default());
    let token = request(&mut ui, p, "draft");
    complete(&mut ui, &token, vec![SearchSuggestion::new("a", "Alpha")]);
    ui.ime_preedit("\u{3042}", Some((3, 3))).unwrap();
    key(&mut ui, Key::Escape);
    assert!(ui.document(p.editor).unwrap().composition().is_none());
    assert_eq!(ui.search_query(p.container), Some("draft"));
    assert!(ui.search_popup_is_open(p.container));
    key(&mut ui, Key::Escape);
    assert!(!ui.search_popup_is_open(p.container));
    assert_eq!(ui.search_query(p.container), Some("draft"));
    assert_eq!(ui.accepted_value(p.editor), Some(""));
    frame(&mut ui);
    assert!(ui.drain_search_suggestion_requests().is_empty());
    assert!(ui.drain_search_events().is_empty());
}

#[test]
fn search_focus_leave_disable_hide_and_window_blur_cancel_without_stealing_focus() {
    for reason in 0..4 {
        let (mut ui, p) = fixture(Default::default());
        let root = ui.root();
        let other = ui
            .edit_box(root, "Other", "keep", LayoutStyle::default())
            .unwrap();
        let token = request(&mut ui, p, "draft");
        match reason {
            0 => focus(&mut ui, other),
            1 => {
                ui.set_enabled(p.container, false).unwrap();
                frame(&mut ui);
            }
            2 => {
                ui.edit_props(p.container, |props| props.visible = false)
                    .unwrap();
                frame(&mut ui);
            }
            _ => ui.dispatch(InputEvent::WindowFocused(false)).unwrap(),
        }
        assert!(token.is_cancelled(), "reason {reason}");
        assert_eq!(
            ui.complete_search_suggestions(&token, Ok(vec![SearchSuggestion::new("x", "Late")]))
                .unwrap(),
            SearchCompletion::Stale
        );
        assert!(!ui.search_popup_is_open(p.container));
        assert_eq!(ui.search_query(p.container), Some("draft"));
        if reason == 0 {
            assert_eq!(ui.runtime.focused(), Some(other));
        }
    }
    let (mut ui, p) = fixture(Default::default());
    let root = ui.root();
    let other = ui
        .button(root, "Other", None, LayoutStyle::default())
        .unwrap();
    let token = request(&mut ui, p, "a");
    complete(&mut ui, &token, vec![SearchSuggestion::new("a", "Alpha")]);
    focus(&mut ui, other);
    assert_eq!(ui.runtime.focused(), Some(other));
    assert!(!ui.search_popup_is_open(p.container));
}

#[test]
fn search_removal_cancels_tokens_and_rejects_single_required_part_removal() {
    let (mut ui, p) = fixture(Default::default());
    let token = request(&mut ui, p, "a");
    for id in [
        p.editor,
        p.clear_button,
        p.search_button,
        p.popup.container,
        p.popup.content,
        p.list.container,
        p.list.content,
        p.status_label,
    ] {
        assert!(ui.remove(id).is_err());
    }
    ui.remove(p.container).unwrap();
    assert!(token.is_cancelled());
    assert!(ui.search_parts(p.container).is_none());
    assert!(!ui.runtime.tree().contains(p.popup.container));
    assert_eq!(
        ui.complete_search_suggestions(&token, Ok(Vec::new()))
            .unwrap(),
        SearchCompletion::Stale
    );
}

#[test]
fn search_malformed_response_preserves_previous_rows_query_and_reports_error() {
    let (mut ui, p) = fixture(Default::default());
    let first = request(&mut ui, p, "a");
    complete(&mut ui, &first, vec![SearchSuggestion::new("a", "Alpha")]);
    let rows = ui.rich.lists[&p.list.container].rows.clone();
    let source = ui.selection_items(p.list.container).unwrap().to_vec();
    let token = request(&mut ui, p, "b");
    assert_eq!(
        ui.complete_search_suggestions(
            &token,
            Ok(vec![
                SearchSuggestion::new("x", "X"),
                SearchSuggestion::new("x", "Y")
            ])
        )
        .unwrap(),
        SearchCompletion::Rejected
    );
    assert_eq!(ui.search_query(p.container), Some("b"));
    assert_eq!(ui.search_suggestions(p.container).unwrap()[0].key, "a");
    assert_eq!(ui.selection_items(p.list.container).unwrap(), source);
    assert_eq!(ui.rich.lists[&p.list.container].rows, rows);
    assert!(matches!(
        ui.search_status(p.container),
        Some(SearchStatus::Failed(_))
    ));
    assert!(matches!(
        &ui.drain_search_events()[..],
        [SearchEvent {
            kind: SearchEventKind::SuggestionsFailed { .. },
            ..
        }]
    ));
    assert!(ui.choose_search_suggestion(p.container, "a").is_err());
}

#[test]
fn search_invalid_shaping_and_provider_error_text_are_safe_rejections() {
    let (mut ui, p) = fixture(Default::default());
    let token = request(&mut ui, p, "a");
    let value = format!("a{}", "\u{0301}".repeat(100));
    assert_eq!(
        ui.complete_search_suggestions(&token, Ok(vec![SearchSuggestion::new("a", &value)]))
            .unwrap(),
        SearchCompletion::Rejected
    );
    frame(&mut ui);
    let _ = ui.scene();
    ui.drain_search_events();
    let token = request(&mut ui, p, "ab");
    assert_eq!(
        ui.complete_search_suggestions(&token, Err(value)).unwrap(),
        SearchCompletion::Rejected
    );
    frame(&mut ui);
    let _ = ui.scene();
    assert!(
        matches!(ui.search_status(p.container),Some(SearchStatus::Failed(message)) if message.contains("invalid error"))
    );
}

#[test]
fn search_history_is_explicit_bounded_and_choice_policy_produces_two_ordered_events() {
    let (mut ui, p) = fixture(SearchBoxSettings {
        history_limit: 3,
        max_suggestions: 2,
        choice_policy: SearchChoicePolicy::FillAndSearch,
        ..Default::default()
    });
    ui.set_search_history(
        p.container,
        vec!["Alpha".into(), "Beta".into(), "Gamma".into()],
    )
    .unwrap();
    ui.open_search_history(p.container).unwrap();
    assert_eq!(ui.search_suggestions(p.container).unwrap().len(), 2);
    assert_eq!(ui.search_query(p.container), Some(""));
    key(&mut ui, Key::ArrowDown);
    key(&mut ui, Key::Enter);
    assert!(
        matches!(&ui.drain_search_events()[..],[SearchEvent{kind:SearchEventKind::SuggestionChosen{query:a,..},..},SearchEvent{kind:SearchEventKind::SearchRequested{query:b},..}] if a=="Alpha" && b=="Alpha")
    );
    ui.clear_search_query(p.container).unwrap();
    assert_eq!(ui.search_query(p.container), Some(""));
    assert!(ui.drain_search_events().is_empty());
    ui.set_search_history(p.container, Vec::new()).unwrap();
    assert!(ui.search_history(p.container).unwrap().is_empty());
}

#[test]
fn search_direct_inner_readonly_cancels_pending_and_prevents_actions() {
    let (mut ui, p) = fixture(Default::default());
    let token = request(&mut ui, p, "draft");
    ui.set_read_only(p.editor, true).unwrap();
    frame(&mut ui);
    assert!(token.is_cancelled());
    for button in [p.clear_button, p.search_button] {
        assert!(!ui.is_enabled(button));
        let semantic = ui
            .semantics()
            .into_iter()
            .find(|node| node.id == button)
            .unwrap();
        assert!(semantic.disabled);
        assert!(!semantic.actions.contains(&SemanticAction::Activate));
    }
    ui.clear_search_query(p.container).unwrap();
    ui.submit_search(p.container).unwrap();
    assert_eq!(ui.search_query(p.container), Some("draft"));
    assert!(ui.drain_search_events().is_empty());
    assert_eq!(
        ui.complete_search_suggestions(&token, Ok(Vec::new()))
            .unwrap(),
        SearchCompletion::Stale
    );
    ui.set_read_only(p.editor, false).unwrap();
    frame(&mut ui);
    assert!(ui.is_enabled(p.clear_button) && ui.is_enabled(p.search_button));
}

#[test]
fn search_target_uses_shared_query_engine_for_text_and_null_semantics() {
    use rust_desktop_ui_data::{
        CellValue, DataRow, DataTable, FieldSchema, FieldType, QueryEngine, TableSchema,
        TextComparison, TextMatchOperator,
    };
    let mut field = FieldSchema::new("Name", FieldType::Text);
    field.nullable = true;
    let table = DataTable::new(
        TableSchema::new(vec![field]).unwrap(),
        vec![
            DataRow::new(1, vec![CellValue::Text("Alpha".into())]),
            DataRow::new(2, vec![CellValue::Text("Beta".into())]),
            DataRow::new(3, vec![CellValue::Null]),
        ],
    )
    .unwrap();
    let target = SearchTarget {
        fields: vec!["Name".into()],
        operator: TextMatchOperator::Contains,
        comparison: TextComparison::UnicodeLowercase,
    };
    assert_eq!(
        QueryEngine::execute(&table, &target.query_request("ALP").unwrap())
            .unwrap()
            .row_keys,
        [1]
    );
    assert_eq!(
        QueryEngine::execute(&table, &target.query_request("").unwrap())
            .unwrap()
            .row_keys,
        [1, 2, 3]
    );
    assert!(
        QueryEngine::execute(
            &table,
            &SearchTarget {
                fields: vec!["Missing".into()],
                ..target
            }
            .query_request("x")
            .unwrap()
        )
        .is_err()
    );
}

#[test]
fn search_clear_and_search_buttons_keyboard_activate_exactly_once_and_return_editor_focus() {
    let (mut ui, p) = fixture(Default::default());
    ui.set_search_query(p.container, "Draft").unwrap();
    focus(&mut ui, p.clear_button);
    key(&mut ui, Key::Enter);
    ui.dispatch(InputEvent::KeyUp {
        key: Key::Enter,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert_eq!(ui.search_query(p.container), Some(""));
    assert_eq!(ui.runtime.focused(), Some(p.editor));
    assert!(ui.drain_search_events().is_empty());
    ui.set_search_query(p.container, "Query").unwrap();
    focus(&mut ui, p.search_button);
    key(&mut ui, Key::Enter);
    ui.dispatch(InputEvent::KeyUp {
        key: Key::Enter,
        modifiers: Modifiers::default(),
    })
    .unwrap();
    assert_eq!(ui.runtime.focused(), Some(p.editor));
    assert!(
        matches!(&ui.drain_search_events()[..],[SearchEvent {kind:SearchEventKind::SearchRequested{query},..}] if query=="Query")
    );
}

#[test]
fn search_byte_limit_and_inner_settings_rejection_preserve_text_and_history() {
    let (mut ui, p) = fixture(SearchBoxSettings {
        max_query_length: 4096,
        ..Default::default()
    });
    ui.set_search_query(p.container, "keep").unwrap();
    let settings = ui.editor_settings(p.editor).unwrap().clone();
    assert!(ui.set_max_length(p.editor, None).is_err());
    assert!(
        ui.set_editor_settings(
            p.editor,
            EditorSettings {
                commit_on_focus_lost: true,
                ..settings.clone()
            }
        )
        .is_err()
    );
    assert_eq!(ui.editor_settings(p.editor), Some(&settings));
    let cluster = format!("a{}", "\u{301}".repeat(31));
    let large = cluster.repeat(1100);
    assert!(large.len() > MAX_SEARCH_QUERY_BYTES);
    assert!(ui.set_search_query(p.container, &large).is_err());
    assert!(ui.set_text(p.editor, &large).is_err());
    assert!(ui.set_search_history(p.container, vec![large]).is_err());
    assert_eq!(ui.search_query(p.container), Some("keep"));
    assert!(ui.search_history(p.container).unwrap().is_empty());
    ui.ime_preedit(&cluster.repeat(1100), None).unwrap();
    assert!(ui.document(p.editor).unwrap().composition().is_none());
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Character("z".into()),
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
        repeat: false,
    })
    .unwrap();
    assert_eq!(ui.search_query(p.container), Some(""));
}

#[test]
fn search_queue_backpressure_rejects_choice_before_filling_query() {
    let (mut ui, p) = fixture(SearchBoxSettings {
        choice_policy: SearchChoicePolicy::FillAndSearch,
        ..Default::default()
    });
    ui.set_search_query(p.container, "keep").unwrap();
    for _ in 0..255 {
        ui.submit_search(p.container).unwrap();
    }
    let token = request(&mut ui, p, "a");
    complete(&mut ui, &token, vec![SearchSuggestion::new("a", "Alpha")]);
    assert!(ui.choose_search_suggestion(p.container, "a").is_err());
    assert_eq!(ui.search_query(p.container), Some("a"));
    assert_eq!(ui.search_history(p.container).unwrap(), ["keep"]);
    assert_eq!(ui.drain_search_events().len(), 255);
    ui.choose_search_suggestion(p.container, "a").unwrap();
    assert_eq!(ui.drain_search_events().len(), 2);
}

#[test]
fn search_history_rejection_and_density_change_keep_live_editor_and_rows() {
    let mut ui = Controls::new(Theme::Compact).unwrap();
    let root = ui.root();
    let p = ui
        .search_box(
            root,
            "Find",
            "",
            Default::default(),
            LayoutStyle {
                width: Length::Px(400.0),
                ..Default::default()
            },
        )
        .unwrap();
    focus(&mut ui, p.editor);
    ui.set_search_history(p.container, vec!["Alpha".into()])
        .unwrap();
    let bad = format!("a{}", "\u{301}".repeat(100));
    assert!(ui.set_search_history(p.container, vec![bad]).is_err());
    assert_eq!(ui.search_history(p.container).unwrap(), ["Alpha"]);
    ui.open_search_history(p.container).unwrap();
    let rows = ui.rich.lists[&p.list.container].rows.clone();
    ui.set_theme(Theme::FluentDark).unwrap();
    frame(&mut ui);
    assert_eq!(ui.runtime.focused(), Some(p.editor));
    assert_eq!(
        ui.runtime.bounds(p.container).unwrap().height,
        Theme::FluentDark.tokens().control_height
    );
    assert_eq!(ui.rich.lists[&p.list.container].rows, rows);
    assert!(ui.search_popup_is_open(p.container));
}

#[test]
fn search_history_from_external_command_keeps_popup_after_direct_focus_and_update() {
    let (mut ui, p) = fixture(Default::default());
    ui.set_search_history(p.container, vec!["Alpha".into(), "Beta".into()])
        .unwrap();
    let external = ui
        .button(ui.root(), "History", None, LayoutStyle::default())
        .unwrap();
    focus(&mut ui, external);
    ui.set_text(p.editor, "unframed draft").unwrap();
    ui.runtime.apply(Mutation::Focus(Some(p.editor))).unwrap();
    ui.open_search_history(p.container).unwrap();
    frame(&mut ui);
    assert!(ui.search_popup_is_open(p.container));
    assert_eq!(ui.search_status(p.container), Some(&SearchStatus::Ready));
    assert_eq!(ui.search_query(p.container), Some("unframed draft"));
    assert_eq!(
        ui.search_suggestions(p.container).unwrap()[0].key,
        "history-0"
    );
    assert!(ui.drain_search_suggestion_requests().is_empty());
    assert_eq!(ui.runtime.focused(), Some(p.editor));
    ui.text_input("x").unwrap();
    frame(&mut ui);
    assert!(!ui.search_popup_is_open(p.container));
    assert_eq!(ui.search_status(p.container), Some(&SearchStatus::Pending));
    assert_eq!(ui.drain_search_suggestion_requests().len(), 1);
}

#[test]
fn search_action_button_has_measured_clickable_width_across_density_and_label_changes() {
    use crate::{ThemeConfig, ThemeDensity, ThemeMode};
    let mut ui = Controls::new(Theme::Compact).unwrap();
    let root = ui.root();
    let p = ui
        .search_box(
            root,
            "Find",
            "alpha",
            Default::default(),
            LayoutStyle {
                width: Length::Px(440.0),
                ..Default::default()
            },
        )
        .unwrap();
    let ids = ui.runtime.tree().live_ids();
    for density in [
        ThemeDensity::Compact,
        ThemeDensity::Comfortable,
        ThemeDensity::Touch,
    ] {
        ui.set_theme(Theme::Configured(
            ThemeConfig::new(ThemeMode::Light).with_density(density),
        ))
        .unwrap();
        frame(&mut ui);
        let measured = ui
            .measure_button_content(
                "Search",
                &ButtonStyle::default(),
                None,
                ButtonContentLayout::Leading,
                4096.0,
            )
            .unwrap();
        let button = ui.runtime.bounds(p.search_button).unwrap();
        let editor = ui.runtime.bounds(p.editor).unwrap();
        let clear = ui.runtime.bounds(p.clear_button).unwrap();
        let owner = ui.runtime.bounds(p.container).unwrap();
        assert!(
            button.width >= measured.width && button.width >= ui.theme().tokens().control_height
        );
        assert!(button.x >= clear.x + clear.width && clear.x >= editor.x + editor.width);
        assert!(button.x + button.width <= owner.x + owner.width + 0.01);
        assert_eq!(button.height, ui.theme().tokens().control_height);
        let position = Point::new(
            button.x + button.width / 2.0,
            button.y + button.height / 2.0,
        );
        assert_eq!(ui.runtime.hit_test(position), Some(p.search_button));
        ui.dispatch(InputEvent::PointerDown {
            position,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        })
        .unwrap();
        ui.dispatch(InputEvent::PointerUp {
            position,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        })
        .unwrap();
        let events = ui.drain_search_events();
        assert_eq!(
            events
                .iter()
                .filter(
                    |e| matches!(&e.kind,SearchEventKind::SearchRequested{query} if query=="alpha")
                )
                .count(),
            1
        );
        assert_eq!(ui.runtime.tree().live_ids(), ids);
    }
    let old = ui.runtime.bounds(p.search_button).unwrap().width;
    ui.set_label(p.search_button, "Search the document")
        .unwrap();
    frame(&mut ui);
    assert!(ui.runtime.bounds(p.search_button).unwrap().width > old);
    assert_eq!(ui.document(p.editor).unwrap().text(), "alpha");
}
