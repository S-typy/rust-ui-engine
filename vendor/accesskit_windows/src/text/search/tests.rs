// Copyright 2026 rust-ui-engine contributors. Licensed under Apache-2.0.
use super::*;
use accesskit::{Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};

fn tree(readonly: bool) -> Tree {
    let mut root = Node::new(Role::TextInput);
    if readonly {
        root.set_read_only();
    }
    let mut nodes = vec![];
    let mut children = vec![];
    for (i, (value, weight, italic)) in [
        ("First ", 400.0, false),
        ("Бета ", 700.0, false),
        ("e\u{301} 😀 ", 700.0, true),
        ("БЕТА", 400.0, false),
        (" END", 700.0, false),
    ]
    .into_iter()
    .enumerate()
    {
        let id = NodeId(i as u64 + 2);
        let mut run = Node::new(Role::TextRun);
        run.set_value(value);
        run.set_font_family("Test Font");
        run.set_font_size(16.0);
        run.set_font_weight(weight);
        if italic {
            run.set_italic();
        }
        run.set_character_lengths(if i == 2 {
            vec![3, 1, 4, 1]
        } else {
            value.chars().map(|c| c.len_utf8() as u8).collect()
        });
        children.push(id);
        nodes.push((id, run));
    }
    root.set_children(children);
    nodes.insert(0, (NodeId(1), root));
    Tree::new(
        TreeUpdate {
            nodes,
            tree: Some(TreeInfo::new(NodeId(1))),
            tree_id: TreeId::ROOT,
            focus: NodeId(1),
        },
        false,
    )
}

#[test]
fn unicode_direction_case_cross_run_and_grapheme_endpoints() {
    let tree = tree(false);
    let root = tree.state().root();
    let range = root.document_range();
    let first = text(&range, &BSTR::from("бета"), false, true)
        .unwrap()
        .unwrap();
    assert_eq!(first.text(), "Бета");
    assert_eq!(first.start().to_global_utf16_index(), 6);
    let last = text(&range, &BSTR::from("бета"), true, true)
        .unwrap()
        .unwrap();
    assert_eq!(last.text(), "БЕТА");
    assert!(last.start() > first.end());
    assert!(
        text(&range, &BSTR::from("бета"), false, false)
            .unwrap()
            .is_none()
    );
    for (needle, expected) in [
        ("e", "e\u{301}"),
        ("\u{301}", "e\u{301}"),
        ("😀", "😀"),
        ("Бета e", "Бета e\u{301}"),
    ] {
        assert_eq!(
            text(&range, &BSTR::from(needle), false, false)
                .unwrap()
                .unwrap()
                .text(),
            expected
        );
    }
    let mut suffix = range;
    suffix.set_start(first.end());
    assert_eq!(
        text(&suffix, &BSTR::from("бета"), false, true)
            .unwrap()
            .unwrap()
            .text(),
        "БЕТА"
    );
    let mut prefix = range;
    prefix.set_end(first.end());
    assert!(
        text(&prefix, &BSTR::from("БЕТА"), true, false)
            .unwrap()
            .is_none()
    );
    assert_eq!(range.text(), "First Бета e\u{301} 😀 БЕТА END");
}

#[test]
fn search_empty_invalid_and_ordinal_non_normalization() {
    let tree = tree(false);
    let root = tree.state().root();
    let range = root.document_range();
    assert_eq!(
        text(&range, &BSTR::from_wide(&[0xd800]), false, false)
            .err()
            .unwrap()
            .code(),
        windows::Win32::Foundation::E_INVALIDARG
    );
    assert_eq!(
        text(&range, &BSTR::new(), false, false)
            .err()
            .unwrap()
            .code(),
        windows::Win32::Foundation::E_INVALIDARG
    );
    assert!(
        text(&range, &BSTR::from("é"), false, true)
            .unwrap()
            .is_none()
    );
    assert!(
        text(
            &range.start().to_degenerate_range(),
            &BSTR::from("First"),
            false,
            false
        )
        .unwrap()
        .is_none()
    );
    let too_large = BSTR::from("x".repeat(MAX_SEARCH_UNITS + 1));
    assert_eq!(
        text(&range, &too_large, false, false).err().unwrap().code(),
        windows::Win32::Foundation::E_INVALIDARG
    );
}

#[test]
fn oversized_text_search_is_bounded_without_partial_success() {
    let mut root = Node::new(Role::TextInput);
    root.set_children(vec![NodeId(2)]);
    let mut run = Node::new(Role::TextRun);
    run.set_value("x".repeat(MAX_SEARCH_UNITS + 1));
    run.set_character_lengths(vec![1; MAX_SEARCH_UNITS + 1]);
    run.set_font_weight(700.0);
    let tree = Tree::new(
        TreeUpdate {
            nodes: vec![(NodeId(1), root), (NodeId(2), run)],
            tree: Some(TreeInfo::new(NodeId(1))),
            tree_id: TreeId::ROOT,
            focus: NodeId(1),
        },
        false,
    );
    let root = tree.state().root();
    let range = root.document_range();
    for error in [
        text(&range, &BSTR::from("x"), false, false).err().unwrap(),
        attribute(
            &range,
            UIA_FontWeightAttributeId,
            &VARIANT::from(700i32),
            true,
        )
        .err()
        .unwrap(),
    ] {
        assert_eq!(
            error.code(),
            windows::core::HRESULT(UIA_E_INVALIDOPERATION as _)
        );
    }
}

#[test]
fn attributes_merge_adjacent_matches_and_respect_direction_and_range() {
    let tree = tree(false);
    let root = tree.state().root();
    let range = root.document_range();
    let bold: VARIANT = 700i32.into();
    let first = attribute(&range, UIA_FontWeightAttributeId, &bold, false)
        .unwrap()
        .unwrap();
    assert_eq!(first.text(), "Бета e\u{301} 😀 ");
    assert_eq!(
        attribute(&range, UIA_FontWeightAttributeId, &bold, true)
            .unwrap()
            .unwrap()
            .text(),
        " END"
    );
    let mut limited = range;
    limited.set_start(first.start().forward_to_character_end());
    limited.set_end(first.end());
    assert_eq!(
        attribute(&limited, UIA_FontWeightAttributeId, &bold, true)
            .unwrap()
            .unwrap()
            .text(),
        "ета e\u{301} 😀 "
    );
    assert_eq!(
        attribute(
            &range,
            UIA_FontNameAttributeId,
            &VARIANT::from("Test Font"),
            false
        )
        .unwrap()
        .unwrap()
        .text(),
        range.text()
    );
    assert_eq!(
        attribute(&range, UIA_IsItalicAttributeId, &VARIANT::from(true), false)
            .unwrap()
            .unwrap()
            .text(),
        "e\u{301} 😀 "
    );
    assert!(
        attribute(
            &range,
            UIA_FontSizeAttributeId,
            &VARIANT::from(17.0f64),
            false
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(
        attribute(
            &range,
            UIA_FontSizeAttributeId,
            &VARIANT::from(16.0f64),
            false
        )
        .unwrap()
        .unwrap()
        .text(),
        range.text()
    );
}

#[test]
fn attributes_are_typed_and_readonly_empty_missing_are_explicit() {
    for readonly in [false, true] {
        let tree = tree(readonly);
        let root = tree.state().root();
        let range = root.document_range();
        assert_eq!(
            attribute(
                &range,
                UIA_IsReadOnlyAttributeId,
                &VARIANT::from(readonly),
                false
            )
            .unwrap()
            .unwrap()
            .text(),
            range.text()
        );
        assert!(
            attribute(
                &range,
                UIA_IsReadOnlyAttributeId,
                &VARIANT::from(!readonly),
                false
            )
            .unwrap()
            .is_none()
        );
        assert_eq!(
            attribute(
                &range,
                UIA_FontWeightAttributeId,
                &VARIANT::from("700"),
                false
            )
            .err()
            .unwrap()
            .code(),
            windows::Win32::Foundation::E_INVALIDARG
        );
        assert!(
            attribute(
                &range,
                UIA_TEXTATTRIBUTE_ID(-1),
                &VARIANT::from(700i32),
                false
            )
            .unwrap()
            .is_none()
        );
        let empty = range.start().to_degenerate_range();
        assert!(
            attribute(
                &empty,
                UIA_IsReadOnlyAttributeId,
                &VARIANT::from(readonly),
                true
            )
            .unwrap()
            .unwrap()
            .is_degenerate()
        );
    }
}

#[test]
fn direct_com_search_is_passive_returns_null_and_rejects_lost_owner() {
    use crate::{context::ActionHandlerNoMut, window_handle::WindowHandle};
    use windows::Win32::Foundation::HWND;
    struct NoActions;
    impl ActionHandlerNoMut for NoActions {
        fn do_action(&self, _: ActionRequest) {
            panic!("Search must be passive");
        }
    }
    let context = Context::new(
        WindowHandle(HWND::default()),
        tree(false),
        Arc::new(NoActions),
        false,
    );
    let root = context.read_tree().state().root_id();
    let provider: IRawElementProviderSimple =
        context.get_or_create_platform_node(root).into_interface();
    let range = unsafe {
        let provider: ITextProvider = provider
            .GetPatternProvider(UIA_TextPatternId)
            .unwrap()
            .cast()
            .unwrap();
        let range = provider.DocumentRange().unwrap();
        let found = range.FindText(&BSTR::from("бета"), false, true).unwrap();
        assert_eq!(found.GetText(-1).unwrap(), BSTR::from("Бета"));
        assert_eq!(
            range
                .FindText(&BSTR::from("absent"), false, false)
                .err()
                .unwrap(),
            Error::empty()
        );
        let bold = range
            .FindAttribute(UIA_FontWeightAttributeId, &VARIANT::from(700i32), true)
            .unwrap();
        assert_eq!(bold.GetText(-1).unwrap(), BSTR::from(" END"));
        assert_eq!(
            range
                .FindAttribute(UIA_IsReadOnlyAttributeId, &VARIANT::from(true), false)
                .err()
                .unwrap(),
            Error::empty()
        );
        range
    };
    drop(context);
    unsafe {
        assert_eq!(
            range
                .FindText(&BSTR::from("First"), false, false)
                .err()
                .unwrap()
                .code(),
            windows::core::HRESULT(UIA_E_ELEMENTNOTAVAILABLE as _)
        );
        assert_eq!(
            range
                .FindAttribute(UIA_IsReadOnlyAttributeId, &VARIANT::from(false), false)
                .err()
                .unwrap()
                .code(),
            windows::core::HRESULT(UIA_E_ELEMENTNOTAVAILABLE as _)
        );
    }
}
