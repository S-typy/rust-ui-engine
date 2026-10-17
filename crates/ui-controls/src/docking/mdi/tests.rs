use super::*;
use crate::{DockNode, DockPane};
fn id(s: &str) -> DockPaneId {
    DockPaneId::new(s).unwrap()
}
pub(super) fn layout() -> DockLayout {
    let ids = vec![id("A"), id("B"), id("C")];
    DockLayout::new(
        ids.iter()
            .map(|id| DockPane {
                id: id.clone(),
                title: id.to_string(),
                closable: true,
                min_size: Size::new(120., 80.),
            })
            .collect(),
        DockNode::Tabs {
            id: DockGroupId(1),
            panes: ids,
            active: id("A"),
        },
    )
    .unwrap()
}
#[test]
fn mdi_rectangles_identity_z_order_clipping_and_invalid_update() {
    let l = layout();
    let mut m = DockMdiLayout::new();
    m.synchronize(&l, DockWindowId::MAIN).unwrap();
    let first = m.stored_bounds(&id("A")).unwrap();
    m.activate(&id("A")).unwrap();
    assert_eq!(m.order().last(), Some(&id("A")));
    assert_eq!(m.stored_bounds(&id("A")), Some(first));
    let g = m
        .arrange(DockWindowId::MAIN, Rect::new(12., 18., 1000., 700.))
        .unwrap();
    let p = Point::new(100., 100.);
    assert_eq!(g.frame_at(p).unwrap().pane, id("A"));
    assert!(
        g.visible_content(&id("B"))
            .iter()
            .all(|r| !r.contains(p.x, p.y))
    );
    let revision = m.revision();
    assert!(
        m.set_bounds(&l, &id("A"), Rect::new(f32::NAN, 0., 100., 100.))
            .is_err()
    );
    assert_eq!(m.revision(), revision);
    assert_eq!(m.stored_bounds(&id("A")), Some(first));
}
#[test]
fn mdi_resize_clamp_does_not_destroy_saved_rects_and_reopen_restores_them() {
    let mut l = layout();
    let mut m = DockMdiLayout::new();
    m.synchronize(&l, DockWindowId::MAIN).unwrap();
    let saved = m.stored_bounds(&id("C")).unwrap();
    let g = m
        .arrange(DockWindowId::MAIN, Rect::new(0., 0., 90., 60.))
        .unwrap();
    assert!(
        g.frames
            .iter()
            .all(|f| f.bounds.width <= 90. && f.bounds.height <= 60.)
    );
    assert_eq!(m.stored_bounds(&id("C")), Some(saved));
    l.close(&id("C")).unwrap();
    m.synchronize(&l, DockWindowId::MAIN).unwrap();
    assert!(!m.order().contains(&id("C")));
    l.restore(
        &id("C"),
        DockPlacement::Tab {
            group: DockGroupId(1),
            index: 2,
        },
    )
    .unwrap();
    m.synchronize(&l, DockWindowId::MAIN).unwrap();
    assert_eq!(m.stored_bounds(&id("C")), Some(saved));
}
#[test]
fn tile_and_maximize_are_bounded_atomic() {
    let l = layout();
    let mut m = DockMdiLayout::new();
    m.synchronize(&l, DockWindowId::MAIN).unwrap();
    m.tile(&l, Size::new(900., 600.)).unwrap();
    let g = m
        .arrange(DockWindowId::MAIN, Rect::new(0., 0., 900., 600.))
        .unwrap();
    assert_eq!(g.frames[0].bounds.width, 450.);
    let before = m.rects.clone();
    assert!(m.tile(&l, Size::new(20., 20.)).is_err());
    assert_eq!(m.rects, before);
    m.toggle_maximize(&id("B")).unwrap();
    let g = m
        .arrange(DockWindowId::MAIN, Rect::new(0., 0., 900., 600.))
        .unwrap();
    assert_eq!(g.frames.len(), 1);
    assert_eq!(g.frames[0].bounds, Rect::new(0., 0., 900., 600.));
}

#[test]
fn maximized_activation_and_extreme_viewport_keep_geometry_coherent() {
    let l = layout();
    let mut m = DockMdiLayout::new();
    m.synchronize(&l, DockWindowId::MAIN).unwrap();
    m.toggle_maximize(&id("B")).unwrap();
    assert_eq!(m.order().last(), Some(&id("B")));
    assert!(m.activate(&id("C")).unwrap());
    assert_eq!(m.maximized(), Some(&id("C")));
    assert!(
        m.arrange(DockWindowId::MAIN, Rect::new(1e30, 0., 600., 400.))
            .is_err()
    );
    assert!(
        m.arrange(DockWindowId::MAIN, Rect::new(0., 1e30, 600., 400.))
            .is_err()
    );
    assert_eq!(m.maximized(), Some(&id("C")));
}
