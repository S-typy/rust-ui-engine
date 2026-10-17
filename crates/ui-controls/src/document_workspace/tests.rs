use super::*;
use crate::{DockGroupId, DockNode, DockPane};
use rust_desktop_ui_core::Size;
fn id(s: &str) -> DocumentId {
    DocumentId::new(s).unwrap()
}
fn fixture() -> (DocumentWorkspace, DockLayout) {
    let ids = vec![id("A"), id("B"), id("C")];
    let layout = DockLayout::new(
        ids.iter()
            .map(|id| DockPane {
                id: id.clone(),
                title: id.to_string(),
                closable: true,
                min_size: Size::new(100., 80.),
            })
            .collect(),
        DockNode::Tabs {
            id: DockGroupId(1),
            panes: ids,
            active: id("A"),
        },
    )
    .unwrap();
    (DocumentWorkspace::new(&layout).unwrap(), layout)
}
fn request(p: DocumentCloseProgress) -> DocumentRequest {
    let DocumentCloseProgress::Pending(r) = p else {
        panic!("Expected request")
    };
    r
}
#[test]
fn batch_cancel_after_successful_save_preserves_all_documents_and_truthful_saved_revision() {
    let (mut docs, mut layout) = fixture();
    docs.mark_changed(&id("A")).unwrap();
    docs.mark_changed(&id("B")).unwrap();
    let before = layout.snapshot();
    let a = request(
        docs.begin_close(&mut layout, DocumentCloseScope::All)
            .unwrap(),
    );
    let save = request(
        docs.decide(&mut layout, &a, DocumentCloseDecision::Save)
            .unwrap(),
    );
    let b = request(docs.complete_save(&mut layout, &save, Ok(())).unwrap());
    assert_eq!(layout.snapshot(), before);
    assert!(!docs.document(&id("A")).unwrap().dirty());
    assert!(docs.document(&id("B")).unwrap().dirty());
    assert_eq!(
        docs.decide(&mut layout, &b, DocumentCloseDecision::Cancel)
            .unwrap(),
        DocumentCloseProgress::Cancelled
    );
    assert_eq!(layout.snapshot(), before);
    assert!(docs.complete_save(&mut layout, &save, Ok(())).is_err());
}
#[test]
fn failure_and_late_changes_never_partially_close() {
    let (mut docs, mut layout) = fixture();
    docs.mark_changed(&id("A")).unwrap();
    docs.mark_changed(&id("B")).unwrap();
    let before = layout.snapshot();
    let a = request(
        docs.begin_close(&mut layout, DocumentCloseScope::All)
            .unwrap(),
    );
    let b = request(
        docs.decide(&mut layout, &a, DocumentCloseDecision::Discard)
            .unwrap(),
    );
    let save = request(
        docs.decide(&mut layout, &b, DocumentCloseDecision::Save)
            .unwrap(),
    );
    assert_eq!(
        docs.complete_save(&mut layout, &save, Err("Disk full".into()))
            .unwrap(),
        DocumentCloseProgress::Failed("Disk full".into())
    );
    assert_eq!(layout.snapshot(), before);
    let a = request(
        docs.begin_close(&mut layout, DocumentCloseScope::All)
            .unwrap(),
    );
    let save = request(
        docs.decide(&mut layout, &a, DocumentCloseDecision::Save)
            .unwrap(),
    );
    docs.mark_changed(&id("A")).unwrap();
    assert!(!docs.close_pending());
    assert!(docs.complete_save(&mut layout, &save, Ok(())).is_err());
    assert_eq!(layout.snapshot(), before);
}
#[test]
fn foreign_request_wrong_stage_and_layout_change_are_atomic() {
    let (mut docs, mut layout) = fixture();
    let (mut other, mut otherlayout) = fixture();
    docs.mark_changed(&id("A")).unwrap();
    other.mark_changed(&id("A")).unwrap();
    let a = request(
        docs.begin_close(&mut layout, DocumentCloseScope::All)
            .unwrap(),
    );
    let foreign = request(
        other
            .begin_close(&mut otherlayout, DocumentCloseScope::All)
            .unwrap(),
    );
    assert!(
        docs.decide(&mut layout, &foreign, DocumentCloseDecision::Discard)
            .is_err()
    );
    assert!(docs.complete_save(&mut layout, &a, Ok(())).is_err());
    assert_eq!(docs.pending_request(), Some(&a));
    layout.activate(&id("B")).unwrap();
    let changed = layout.snapshot();
    assert!(
        docs.decide(&mut layout, &a, DocumentCloseDecision::Discard)
            .is_err()
    );
    assert_eq!(layout.snapshot(), changed);
    assert_eq!(docs.pending_request(), Some(&a));
    assert!(docs.cancel_close());
}
#[test]
fn all_consent_closes_atomically_and_reopening_retains_identity() {
    let (mut docs, mut layout) = fixture();
    docs.mark_changed(&id("A")).unwrap();
    docs.mark_changed(&id("B")).unwrap();
    let a = request(
        docs.begin_close(&mut layout, DocumentCloseScope::Group(DockGroupId(1)))
            .unwrap(),
    );
    let b = request(
        docs.decide(&mut layout, &a, DocumentCloseDecision::Discard)
            .unwrap(),
    );
    assert!(layout.panes().iter().all(|p| !layout.is_closed(&p.id)));
    let DocumentCloseProgress::Closed {
        documents,
        discarded,
    } = docs
        .decide(&mut layout, &b, DocumentCloseDecision::Discard)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(documents.len(), 3);
    assert_eq!(discarded, vec![id("A"), id("B")]);
    assert!(layout.panes().iter().all(|p| layout.is_closed(&p.id)));
    docs.open(
        &mut layout,
        &id("A"),
        DockPlacement::Root {
            window: DockWindowId::MAIN,
            edge: crate::DockEdge::Right,
        },
    )
    .unwrap();
    assert_eq!(docs.active(), Some(&id("A")));
    assert_eq!(docs.document(&id("A")).unwrap().revision, 1);
    assert!(docs.document(&id("A")).unwrap().dirty());
}
#[test]
fn revision_and_request_overflow_preserve_state() {
    let (mut docs, mut layout) = fixture();
    docs.states.get_mut(&id("A")).unwrap().revision = u64::MAX;
    assert!(docs.mark_changed(&id("A")).is_err());
    assert_eq!(docs.document(&id("A")).unwrap().revision, u64::MAX);
    docs.sequence = u64::MAX;
    let before = layout.snapshot();
    assert!(
        docs.begin_close(&mut layout, DocumentCloseScope::All)
            .is_err()
    );
    assert_eq!(layout.snapshot(), before);
    assert!(!docs.close_pending());
}
#[test]
fn nonclosable_member_rejects_before_prompt_or_close() {
    let (mut docs, layout) = fixture();
    let mut panes = layout.panes().to_vec();
    panes[2].closable = false;
    let mut layout = DockLayout::from_snapshot(panes, layout.snapshot()).unwrap();
    let before = layout.snapshot();
    docs.mark_changed(&id("A")).unwrap();
    assert!(
        docs.begin_close(&mut layout, DocumentCloseScope::All)
            .is_err()
    );
    assert_eq!(layout.snapshot(), before);
    assert!(!docs.close_pending());
}

#[test]
fn register_new_document_is_closed_bounded_and_duplicate_atomic() {
    let (mut w, mut l) = fixture();
    let pane = DockPane {
        id: id("New"),
        title: "New document".into(),
        closable: true,
        min_size: Size::new(100., 80.),
    };
    w.register(&mut l, pane.clone()).unwrap();
    assert!(l.is_closed(&id("New")));
    assert!(!w.document(&id("New")).unwrap().dirty());
    let before = l.snapshot();
    assert!(w.register(&mut l, pane).is_err());
    assert_eq!(l.snapshot(), before);
    assert_eq!(w.documents().count(), 4);
    w.open(
        &mut l,
        &id("New"),
        DockPlacement::Tab {
            group: DockGroupId(1),
            index: 3,
        },
    )
    .unwrap();
    assert_eq!(w.active(), Some(&id("New")));
    assert!(!l.is_closed(&id("New")));
}

#[test]
fn catalog_capacity_and_unknown_close_scope_reject_without_side_effects() {
    let (mut w, mut l) = fixture();
    let original = l.snapshot();
    assert!(
        w.begin_close(&mut l, DocumentCloseScope::Group(DockGroupId(u64::MAX)))
            .is_err()
    );
    assert!(
        w.begin_close(&mut l, DocumentCloseScope::Window(DockWindowId(u64::MAX)))
            .is_err()
    );
    assert_eq!(l.snapshot(), original);
    assert!(!w.close_pending());
    for n in 3..64 {
        w.register(
            &mut l,
            DockPane {
                id: id(&format!("Doc{n}")),
                title: "Document".into(),
                closable: true,
                min_size: Size::new(100., 80.),
            },
        )
        .unwrap();
    }
    let full = l.snapshot();
    assert!(
        w.register(
            &mut l,
            DockPane {
                id: id("Overflow"),
                title: "Document".into(),
                closable: true,
                min_size: Size::new(100., 80.)
            }
        )
        .is_err()
    );
    assert_eq!(l.snapshot(), full);
    assert_eq!(w.documents().count(), 64);
}

#[test]
fn initial_and_post_close_active_key_follow_visible_tab_selection() {
    let (_, mut layout) = fixture();
    layout.activate(&id("B")).unwrap();
    let mut workspace = DocumentWorkspace::new(&layout).unwrap();
    assert_eq!(workspace.active(), Some(&id("B")));
    workspace
        .begin_close(&mut layout, DocumentCloseScope::Document(id("B")))
        .unwrap();
    assert!(
        workspace
            .active()
            .is_some_and(|key| layout.pane_is_visible(key))
    );
}
