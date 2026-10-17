use super::*;
fn model(view: ChartView) -> ChartModel {
    ChartModel::new(
        vec![
            ChartSeries {
                id: 1,
                label: "Доход".into(),
                points: vec![
                    ChartPoint {
                        id: i64::MIN,
                        x: 0.0,
                        y: Some(-5.0),
                    },
                    ChartPoint {
                        id: 2,
                        x: 1.0,
                        y: None,
                    },
                    ChartPoint {
                        id: 3,
                        x: 2.0,
                        y: Some(0.0),
                    },
                    ChartPoint {
                        id: i64::MAX,
                        x: 3.0,
                        y: Some(10.0),
                    },
                ],
            },
            ChartSeries {
                id: 2,
                label: "Расход".into(),
                points: vec![
                    ChartPoint {
                        id: 9,
                        x: 0.0,
                        y: Some(2.0),
                    },
                    ChartPoint {
                        id: 10,
                        x: 3.0,
                        y: Some(5.0),
                    },
                ],
            },
        ],
        ChartSettings {
            view,
            x_title: "Период".into(),
            y_title: "Сумма".into(),
            ..Default::default()
        },
    )
    .unwrap()
}
fn fixture(view: ChartView) -> (Controls, WidgetId) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let id = ui
        .chart(
            root,
            model(view),
            LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.update(Size::new(760.0, 480.0)).unwrap();
    (ui, id)
}
#[test]
fn chart_actual_scene_has_full_labels_legend_and_finite_clipped_geometry() {
    for theme in [Theme::FluentLight, Theme::FluentDark] {
        for view in [ChartView::Line, ChartView::Scatter, ChartView::Bar] {
            let (mut ui, id) = fixture(view);
            ui.set_theme(theme).unwrap();
            for size in [Size::new(760.0, 480.0), Size::new(1100.0, 600.0)] {
                ui.update(size).unwrap();
                assert_eq!(ui.chart_display_status(id), Some(ChartDisplayStatus::Full));
                for label in ["Доход", "Расход", "Период", "Сумма"] {
                    let run = ui
                        .scene()
                        .texts
                        .iter()
                        .find(|r| r.text == label)
                        .unwrap_or_else(|| panic!("Missing {label}"));
                    assert!(
                        run.bounds
                            .intersection(run.clip)
                            .is_some_and(|b| b.width >= run.bounds.width - 0.1
                                && b.height >= run.bounds.height - 0.1),
                        "{} {:?} {:?}",
                        label,
                        run.bounds,
                        run.clip
                    );
                }
                let geometry = ui.chart_geometry(id).unwrap().unwrap();
                for mark in geometry.points {
                    assert!(mark.position.x.is_finite() && mark.position.y.is_finite());
                    assert!(
                        mark.position.x >= geometry.plot.x
                            && mark.position.x <= geometry.plot.x + geometry.plot.width
                    );
                }
                assert!(!ui.scene().stroke_paths.is_empty());
            }
        }
    }
}
#[test]
fn chart_pointer_selection_once_and_capture_loss_cancel() {
    let (mut ui, id) = fixture(ChartView::Scatter);
    let mark = ui.chart_geometry(id).unwrap().unwrap().points[0];
    let down = InputEvent::PointerDown {
        position: mark.position,
        button: PointerButton::Primary,
        modifiers: Default::default(),
    };
    let up = InputEvent::PointerUp {
        position: mark.position,
        button: PointerButton::Primary,
        modifiers: Default::default(),
    };
    ui.dispatch(down.clone()).unwrap();
    ui.cancel_pointer_interaction().unwrap();
    ui.dispatch(up.clone()).unwrap();
    assert!(ui.chart_model(id).unwrap().selected().is_none());
    ui.dispatch(down).unwrap();
    ui.dispatch(up).unwrap();
    assert_eq!(ui.chart_model(id).unwrap().selected(), Some(mark.key));
    assert_eq!(ui.take_chart_selection_changes().len(), 1);
    assert!(ui.take_chart_selection_changes().is_empty());
}
#[test]
fn chart_keyboard_uses_visible_geometry_and_live_replace_preserves_key() {
    let (mut ui, id) = fixture(ChartView::Line);
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::End,
        modifiers: Default::default(),
        repeat: false,
    })
    .unwrap();
    let key = ui.chart_model(id).unwrap().selected().unwrap();
    let mut candidate = ui.chart_model(id).unwrap().clone();
    let mut series = candidate.series().to_vec();
    series[1].points[1].y = Some(6.0);
    candidate
        .replace(series, candidate.settings().clone())
        .unwrap();
    ui.set_chart_model(id, candidate).unwrap();
    assert_eq!(ui.chart_model(id).unwrap().selected(), Some(key));
    assert!(ui.take_chart_selection_changes().is_empty());
    let semantics = ui.semantics().into_iter().find(|n| n.id == id).unwrap();
    assert!(semantics.read_only && semantics.description.unwrap().contains("Расход"));
    assert!(!semantics.actions.contains(&SemanticAction::SetValue));
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Escape,
        modifiers: Default::default(),
        repeat: false,
    })
    .unwrap();
    assert!(ui.chart_model(id).unwrap().selected().is_none());
}
#[test]
fn chart_failed_shape_replacement_is_atomic_and_preserves_capture() {
    let (mut ui, id) = fixture(ChartView::Line);
    let point = ui.chart_geometry(id).unwrap().unwrap().points[0].position;
    ui.dispatch(InputEvent::PointerDown {
        position: point,
        button: PointerButton::Primary,
        modifiers: Default::default(),
    })
    .unwrap();
    let before = ui.chart_model(id).unwrap().clone();
    let mut series = before.series().to_vec();
    series[0].label = format!("A{}", "\u{301}".repeat(40));
    let bad = ChartModel::new(series, before.settings().clone()).unwrap();
    assert!(ui.set_chart_model(id, bad).is_err());
    assert_eq!(ui.chart_model(id).unwrap().series(), before.series());
    assert_eq!(ui.runtime.captured(), Some(id));
    ui.cancel_pointer_interaction().unwrap();
}
#[test]
fn chart_successful_update_cancels_gesture_and_prunes_removed_selection() {
    let (mut ui, id) = fixture(ChartView::Bar);
    let mark = ui.chart_geometry(id).unwrap().unwrap().points[0];
    ui.set_chart_selected(id, Some(mark.key)).unwrap();
    ui.dispatch(InputEvent::PointerDown {
        position: mark.position,
        button: PointerButton::Primary,
        modifiers: Default::default(),
    })
    .unwrap();
    let mut m = ui.chart_model(id).unwrap().clone();
    m.replace(Vec::new(), m.settings().clone()).unwrap();
    ui.set_chart_model(id, m).unwrap();
    assert_eq!(ui.runtime.captured(), None);
    assert!(ui.chart_model(id).unwrap().selected().is_none());
    ui.update(Size::new(760.0, 480.0)).unwrap();
    assert!(ui.chart_geometry(id).unwrap().unwrap().points.is_empty());
}
#[test]
fn chart_tiny_viewport_compact_sparkline_and_idle_repaint() {
    let (mut ui, id) = fixture(ChartView::Line);
    ui.update(Size::new(260.0, 100.0)).unwrap();
    assert_eq!(
        ui.chart_display_status(id),
        Some(ChartDisplayStatus::Compact)
    );
    assert!(
        ui.scene()
            .texts
            .iter()
            .any(|t| t.text.starts_with("Компактная"))
    );
    let mut m = model(ChartView::Sparkline);
    ui.set_chart_model(id, m.clone()).unwrap();
    ui.update(Size::new(260.0, 100.0)).unwrap();
    assert_eq!(ui.chart_display_status(id), Some(ChartDisplayStatus::Full));
    assert!(ui.scene().texts.is_empty());
    m.select(Some(ChartPointKey {
        series: 1,
        point: 3,
    }))
    .unwrap();
    ui.set_chart_model(id, m).unwrap();
    ui.update(Size::new(260.0, 100.0)).unwrap();
    let count = ui.scene().rounded_rectangles.len();
    ui.update(Size::new(260.0, 100.0)).unwrap();
    assert_eq!(ui.scene().rounded_rectangles.len(), count);
}
#[test]
fn chart_disable_ancestor_cancels_pointer_without_selection() {
    let (mut ui, id) = fixture(ChartView::Line);
    let mark = ui.chart_geometry(id).unwrap().unwrap().points[0];
    ui.dispatch(InputEvent::PointerDown {
        position: mark.position,
        button: PointerButton::Primary,
        modifiers: Default::default(),
    })
    .unwrap();
    ui.edit_props(ui.root(), |p| p.enabled = false).unwrap();
    ui.update(Size::new(760.0, 480.0)).unwrap();
    assert!(ui.runtime.captured().is_none());
    assert!(ui.chart_model(id).unwrap().selected().is_none());
    ui.remove(id).unwrap();
    assert!(ui.chart_model(id).is_none());
}

#[test]
fn chart_overlay_button_wins_visual_hit_order() {
    let (mut ui, id) = fixture(ChartView::Line);
    let root = ui.root();
    ui.edit_props(root, |p| p.style.kind = LayoutKind::Overlay)
        .unwrap();
    ui.update(Size::new(760.0, 480.0)).unwrap();
    let point = ui.chart_geometry(id).unwrap().unwrap().points[0].position;
    let hits = std::rc::Rc::new(std::cell::Cell::new(0));
    let copy = hits.clone();
    ui.commands_mut().register("overlay", "Overlay");
    ui.commands_mut()
        .on_execute(&"overlay".into(), move || copy.set(copy.get() + 1));
    let button = ui
        .button(
            root,
            "Overlay",
            Some("overlay".into()),
            LayoutStyle {
                width: Length::Px(130.0),
                height: Length::Px(36.0),
                offset: Point::new(point.x - 30.0, point.y - 20.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.edit_props(button, |p| p.z_index = 100).unwrap();
    ui.update(Size::new(760.0, 480.0)).unwrap();
    let b = ui.runtime.bounds(button).unwrap();
    let point = Point::new(b.x + 40.0, b.y + 18.0);
    assert_eq!(ui.runtime.hit_test(point), Some(button));
    for input in [
        InputEvent::PointerDown {
            position: point,
            button: PointerButton::Primary,
            modifiers: Default::default(),
        },
        InputEvent::PointerUp {
            position: point,
            button: PointerButton::Primary,
            modifiers: Default::default(),
        },
    ] {
        ui.dispatch(input).unwrap();
    }
    assert_eq!(hits.get(), 1);
    assert!(ui.chart_model(id).unwrap().selected().is_none());
    assert!(ui.chart_press.is_none());
}
#[test]
fn chart_external_selection_cancels_old_pointer_and_invalid_choice_preserves_it() {
    let (mut ui, id) = fixture(ChartView::Scatter);
    let geometry = ui.chart_geometry(id).unwrap().unwrap();
    let a = geometry.points[0];
    let b = geometry.points[1];
    ui.dispatch(InputEvent::PointerDown {
        position: a.position,
        button: PointerButton::Primary,
        modifiers: Default::default(),
    })
    .unwrap();
    assert!(
        ui.set_chart_selected(
            id,
            Some(ChartPointKey {
                series: 999,
                point: 0
            })
        )
        .is_err()
    );
    assert_eq!(ui.runtime.captured(), Some(id));
    ui.set_chart_selected(id, Some(b.key)).unwrap();
    assert!(ui.runtime.captured().is_none());
    ui.dispatch(InputEvent::PointerUp {
        position: a.position,
        button: PointerButton::Primary,
        modifiers: Default::default(),
    })
    .unwrap();
    assert_eq!(ui.chart_model(id).unwrap().selected(), Some(b.key));
    assert!(ui.take_chart_selection_changes().is_empty());
}
#[test]
fn chart_boundary_caption_join_is_rejected_before_mount_or_replace() {
    let (mut ui, id) = fixture(ChartView::Line);
    let before = ui.chart_model(id).unwrap().clone();
    let mut series = before.series().to_vec();
    series[0].label = "\u{301}".repeat(32);
    let candidate = ChartModel::new(series, before.settings().clone()).unwrap();
    assert!(ui.set_chart_model(id, candidate.clone()).is_err());
    let nodes = ui.runtime.tree().preorder();
    let root = ui.root();
    assert!(ui.chart(root, candidate, LayoutStyle::default()).is_err());
    assert_eq!(ui.runtime.tree().preorder(), nodes);
    assert_eq!(ui.chart_model(id).unwrap().series(), before.series());
}

#[test]
fn chart_outside_pointer_closes_flyout_and_menu_before_own_capture() {
    let (mut ui, id) = fixture(ChartView::Line);
    ui.edit_props(ui.root(), |p| p.style.kind = LayoutKind::Overlay)
        .unwrap();
    let root = ui.root();
    let anchor = ui
        .button(
            root,
            "Anchor",
            None,
            LayoutStyle {
                width: Length::Px(80.0),
                height: Length::Px(36.0),
                offset: Point::new(10.0, 10.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.update(Size::new(760.0, 480.0)).unwrap();
    let flyout = ui.flyout(anchor, Size::new(150.0, 80.0)).unwrap();
    ui.button(flyout.content, "Popup", None, LayoutStyle::default())
        .unwrap();
    ui.open_flyout(flyout).unwrap();
    ui.update(Size::new(760.0, 480.0)).unwrap();
    let point = ui
        .chart_geometry(id)
        .unwrap()
        .unwrap()
        .points
        .iter()
        .find(|p| ui.runtime.hit_test(p.position) == Some(id))
        .unwrap()
        .position;
    ui.dispatch(InputEvent::PointerDown {
        position: point,
        button: PointerButton::Primary,
        modifiers: Default::default(),
    })
    .unwrap();
    assert!(!ui.flyout_is_open(flyout));
    ui.cancel_pointer_interaction().unwrap();
    ui.commands_mut().register("menu", "Menu");
    ui.popup_menu(anchor, &[MenuItem::new("Menu", "menu")])
        .unwrap();
    ui.update(Size::new(760.0, 480.0)).unwrap();
    let point = ui
        .chart_geometry(id)
        .unwrap()
        .unwrap()
        .points
        .iter()
        .find(|p| ui.runtime.hit_test(p.position) == Some(id))
        .unwrap()
        .position;
    ui.dispatch(InputEvent::PointerDown {
        position: point,
        button: PointerButton::Primary,
        modifiers: Default::default(),
    })
    .unwrap();
    assert!(!ui.popup_open());
    assert_eq!(ui.runtime.captured(), Some(id));
    ui.cancel_pointer_interaction().unwrap();
}
#[test]
fn chart_background_window_and_super_arrow_do_not_capture_or_select() {
    let (mut ui, id) = fixture(ChartView::Line);
    ui.runtime.apply(Mutation::Focus(Some(id))).unwrap();
    ui.dispatch(InputEvent::KeyDown {
        key: Key::Home,
        modifiers: rust_desktop_ui_core::Modifiers {
            super_key: true,
            ..Default::default()
        },
        repeat: false,
    })
    .unwrap();
    assert!(ui.chart_model(id).unwrap().selected().is_none());
    ui.dispatch(InputEvent::WindowFocused(false)).unwrap();
    let point = ui.chart_geometry(id).unwrap().unwrap().points[0].position;
    assert!(
        !ui.chart_before_input(&InputEvent::PointerDown {
            position: point,
            button: PointerButton::Primary,
            modifiers: Default::default()
        })
        .unwrap()
    );
    assert!(ui.runtime.captured().is_none());
}

#[test]
fn chart_overlap_uses_z_order_and_overlay_appearing_during_capture_cancels_commit() {
    let (mut ui, id) = fixture(ChartView::Scatter);
    let root = ui.root();
    ui.edit_props(root, |p| p.style.kind = LayoutKind::Overlay)
        .unwrap();
    let second = ui
        .chart(
            root,
            model(ChartView::Scatter),
            LayoutStyle {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.edit_props(id, |p| p.z_index = 50).unwrap();
    ui.update(Size::new(760.0, 480.0)).unwrap();
    let point = ui.chart_geometry(id).unwrap().unwrap().points[1];
    assert_eq!(ui.runtime.hit_test(point.position), Some(id));
    ui.dispatch(InputEvent::PointerDown {
        position: point.position,
        button: PointerButton::Primary,
        modifiers: Default::default(),
    })
    .unwrap();
    let button = ui
        .button(
            root,
            "Overlay",
            None,
            LayoutStyle {
                width: Length::Px(60.0),
                height: Length::Px(36.0),
                offset: Point::new(point.position.x - 25.0, point.position.y - 15.0),
                ..Default::default()
            },
        )
        .unwrap();
    ui.edit_props(button, |p| p.z_index = 100).unwrap();
    ui.update(Size::new(760.0, 480.0)).unwrap();
    assert_eq!(ui.runtime.hit_test(point.position), Some(button));
    ui.dispatch(InputEvent::PointerUp {
        position: point.position,
        button: PointerButton::Primary,
        modifiers: Default::default(),
    })
    .unwrap();
    assert!(ui.chart_model(id).unwrap().selected().is_none());
    assert!(ui.chart_model(second).unwrap().selected().is_none());
    assert!(ui.runtime.captured().is_none());
}
