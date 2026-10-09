//! M1 foundation gallery: retained primitives, not finished controls.

use rust_desktop_ui_core::{
    Axis, Border, Color, Edges, EventPhase, FrameStats, InputEvent, Key, LayoutKind, LayoutStyle,
    Length, Mutation, NodeProps, Paint, Point, PointerButton, RuntimeError, Scene, Size, UiRuntime,
    UiTree, WidgetId,
};
use rust_desktop_ui_layout::TaffyLayout;
use std::{cell::RefCell, rc::Rc};

const CONTENT_HEIGHT: f32 = 1264.0;

enum Action {
    Activate,
    Remove,
    Scroll(f32),
    ScrollTo(f32),
}

pub struct RetainedDemo {
    pub runtime: UiRuntime,
    layout: TaffyLayout,
    actions: Rc<RefCell<Vec<Action>>>,
    viewport: Size,
    a: WidgetId,
    b: WidgetId,
    c: WidgetId,
    scroller: WidgetId,
    rows: Vec<WidgetId>,
    pub activated: usize,
    pub removed: usize,
    pub scroll_y: f32,
}

impl RetainedDemo {
    pub fn new() -> Result<Self, RuntimeError> {
        let mut tree = UiTree::new();
        let root = tree.root();
        tree.set_props(
            root,
            NodeProps {
                style: LayoutStyle {
                    kind: LayoutKind::Flex(Axis::Vertical),
                    padding: Edges::all(16.0),
                    gap: 12.0,
                    ..Default::default()
                },
                paint: background(239, 244, 250),
                clip: true,
                ..Default::default()
            },
        )?;
        let header = tree.insert(
            root,
            NodeProps {
                style: LayoutStyle {
                    kind: LayoutKind::Flex(Axis::Horizontal),
                    height: Length::Px(56.0),
                    flex_shrink: 0.0,
                    gap: 12.0,
                    ..Default::default()
                },
                ..Default::default()
            },
        )?;
        let a = tree.insert(
            header,
            tile(Color::rgb(35, 90, 170), Color::rgb(60, 118, 201), false),
        )?;
        let b = tree.insert(
            header,
            tile(Color::rgb(22, 128, 110), Color::rgb(42, 157, 137), false),
        )?;
        let c = tree.insert(
            header,
            tile(Color::rgb(208, 145, 45), Color::rgb(229, 172, 80), true),
        )?;
        let body = tree.insert(
            root,
            NodeProps {
                style: LayoutStyle {
                    kind: LayoutKind::Flex(Axis::Horizontal),
                    height: Length::Px(0.0),
                    flex_grow: 1.0,
                    gap: 16.0,
                    ..Default::default()
                },
                ..Default::default()
            },
        )?;
        let navigation = tree.insert(
            body,
            NodeProps {
                style: LayoutStyle {
                    kind: LayoutKind::Stack(Axis::Vertical),
                    width: Length::Px(200.0),
                    flex_shrink: 0.0,
                    padding: Edges::all(12.0),
                    gap: 10.0,
                    ..Default::default()
                },
                paint: background(255, 255, 255),
                clip: true,
                hit_test: false,
                ..Default::default()
            },
        )?;
        for _ in 0..5 {
            tree.insert(
                navigation,
                NodeProps {
                    style: LayoutStyle {
                        height: Length::Px(48.0),
                        ..Default::default()
                    },
                    paint: background(214, 223, 236),
                    hit_test: false,
                    ..Default::default()
                },
            )?;
        }
        let scroller = tree.insert(
            body,
            NodeProps {
                style: LayoutStyle {
                    kind: LayoutKind::Overlay,
                    width: Length::Px(0.0),
                    flex_grow: 1.0,
                    ..Default::default()
                },
                paint: Paint {
                    focus_border: Some(Border {
                        width: 2.0,
                        color: Color::rgb(208, 145, 45),
                    }),
                    ..background(255, 255, 255)
                },
                clip: true,
                focusable: true,
                ..Default::default()
            },
        )?;
        let content = tree.insert(
            scroller,
            NodeProps {
                style: LayoutStyle {
                    kind: LayoutKind::Stack(Axis::Vertical),
                    width: Length::Percent(1.0),
                    height: Length::Px(CONTENT_HEIGHT),
                    padding: Edges::all(12.0),
                    gap: 8.0,
                    ..Default::default()
                },
                hit_test: false,
                ..Default::default()
            },
        )?;
        let mut rows = Vec::new();
        for row in 0..24 {
            rows.push(tree.insert(
                content,
                NodeProps {
                    style: LayoutStyle {
                        height: Length::Px(44.0),
                        ..Default::default()
                    },
                    paint: Paint {
                        hover_background: Some(Color::rgb(218, 232, 250)),
                        border: Some(Border {
                            width: 1.0,
                            color: Color::rgb(224, 232, 241),
                        }),
                        ..if row % 2 == 0 {
                            background(238, 243, 249)
                        } else {
                            background(250, 252, 255)
                        }
                    },
                    ..Default::default()
                },
            )?);
        }
        let actions = Rc::new(RefCell::new(Vec::new()));
        let mut runtime = UiRuntime::new(tree);
        for id in [a, b, c] {
            let actions = Rc::clone(&actions);
            runtime.on(id, move |event, context| {
                if event.phase != EventPhase::Target {
                    return;
                }
                match &event.input {
                    InputEvent::PointerDown {
                        button: PointerButton::Primary,
                        ..
                    } => {
                        context.enqueue(Mutation::CapturePointer(Some(id)));
                        actions.borrow_mut().push(Action::Activate);
                    }
                    InputEvent::KeyDown {
                        key: Key::Enter | Key::Space,
                        repeat: false,
                        ..
                    } => {
                        actions.borrow_mut().push(Action::Activate);
                    }
                    InputEvent::KeyDown {
                        key: Key::Delete,
                        repeat: false,
                        ..
                    } if id == c => {
                        context.enqueue(Mutation::Remove(id));
                        actions.borrow_mut().push(Action::Remove);
                    }
                    _ => {}
                }
            })?;
        }
        let scroll_actions = Rc::clone(&actions);
        runtime.on(scroller, move |event, _context| {
            if event.phase == EventPhase::Capture {
                return;
            }
            let action = match &event.input {
                InputEvent::Wheel { delta, .. } => Some(Action::Scroll(-delta.y)),
                InputEvent::KeyDown {
                    key: Key::ArrowDown,
                    ..
                } => Some(Action::Scroll(52.0)),
                InputEvent::KeyDown {
                    key: Key::ArrowUp, ..
                } => Some(Action::Scroll(-52.0)),
                InputEvent::KeyDown { key: Key::Home, .. } => Some(Action::ScrollTo(0.0)),
                InputEvent::KeyDown { key: Key::End, .. } => Some(Action::ScrollTo(CONTENT_HEIGHT)),
                _ => None,
            };
            if let Some(action) = action {
                scroll_actions.borrow_mut().push(action);
            }
        })?;
        Ok(Self {
            runtime,
            layout: TaffyLayout::new(),
            actions,
            viewport: Size::ZERO,
            a,
            b,
            c,
            scroller,
            rows,
            activated: 0,
            removed: 0,
            scroll_y: 0.0,
        })
    }

    pub fn prepare(&mut self, viewport: Size) -> Result<FrameStats, RuntimeError> {
        let mut stats = self.runtime.update(&mut self.layout, viewport)?;
        self.viewport = viewport;
        self.scroll_to(self.scroll_y)?;
        if self.runtime.needs_update() {
            let next = self.runtime.update(&mut self.layout, viewport)?;
            stats.layout_passes += next.layout_passes;
            stats.measured_nodes += next.measured_nodes;
            stats.arranged_nodes += next.arranged_nodes;
            stats.paint_passes += next.paint_passes;
            stats.painted_nodes += next.painted_nodes;
            stats.semantics_nodes += next.semantics_nodes;
        }
        Ok(stats)
    }

    pub fn input(&mut self, input: InputEvent) -> Result<(), RuntimeError> {
        // Events arriving before the next RedrawRequested must see current geometry.
        self.prepare(self.viewport)?;
        let report = self.runtime.dispatch(input);
        if let Some(error) = report.errors.into_iter().next() {
            return Err(error);
        }
        let actions = std::mem::take(&mut *self.actions.borrow_mut());
        for action in actions {
            match action {
                Action::Activate => self.activated += 1,
                Action::Remove => self.removed += 1,
                Action::Scroll(delta) => self.scroll_to(self.scroll_y + delta)?,
                Action::ScrollTo(y) => self.scroll_to(y)?,
            }
        }
        Ok(())
    }

    fn scroll_to(&mut self, desired: f32) -> Result<(), RuntimeError> {
        let height = self
            .runtime
            .bounds(self.scroller)
            .map_or(0.0, |bounds| bounds.height);
        let next = desired.clamp(0.0, (CONTENT_HEIGHT - height).max(0.0));
        if next != self.scroll_y {
            if let Some(node) = self.runtime.tree().node(self.scroller) {
                let mut props = node.props.clone();
                props.scroll = Point::new(0.0, next);
                self.runtime.apply(Mutation::SetProps {
                    node: self.scroller,
                    props,
                })?;
            }
            self.scroll_y = next;
        }
        Ok(())
    }

    pub fn scene(&self) -> &Scene {
        self.runtime.scene()
    }

    pub fn name(&self, id: Option<WidgetId>) -> &'static str {
        match id {
            None => "none",
            Some(id) if id == self.a => "a",
            Some(id) if id == self.b => "b",
            Some(id) if id == self.c => "c",
            Some(id) if id == self.scroller => "scroll",
            Some(id) if self.rows.contains(&id) => "row",
            _ => "other",
        }
    }
}

fn background(r: u8, g: u8, b: u8) -> Paint {
    Paint {
        background: Some(Color::rgb(r, g, b)),
        ..Default::default()
    }
}

fn tile(base: Color, hover: Color, grow: bool) -> NodeProps {
    NodeProps {
        style: LayoutStyle {
            width: if grow {
                Length::Auto
            } else {
                Length::Px(160.0)
            },
            height: Length::Percent(1.0),
            min_size: Size::new(64.0, 0.0),
            flex_grow: if grow { 1.0 } else { 0.0 },
            ..Default::default()
        },
        paint: Paint {
            background: Some(base),
            hover_background: Some(hover),
            focus_border: Some(Border {
                width: 3.0,
                color: Color::rgb(245, 195, 62),
            }),
            ..Default::default()
        },
        focusable: true,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_desktop_ui_core::Modifiers;

    fn key(demo: &mut RetainedDemo, key: Key, shift: bool) {
        demo.input(InputEvent::KeyDown {
            key,
            modifiers: Modifiers {
                shift,
                ..Default::default()
            },
            repeat: false,
        })
        .unwrap();
    }

    #[test]
    fn retained_gallery_exercises_layout_focus_capture_and_removal() {
        let mut demo = RetainedDemo::new().unwrap();
        let viewport = Size::new(960.0, 640.0);
        demo.prepare(viewport).unwrap();
        let count = demo.runtime.tree().len();
        let baseline = demo.runtime.stats();
        demo.input(InputEvent::PointerMoved {
            position: Point::new(50.0, 40.0),
        })
        .unwrap();
        demo.prepare(viewport).unwrap();
        assert_eq!(demo.runtime.stats().layout_passes, baseline.layout_passes);
        assert_eq!(demo.runtime.hovered(), Some(demo.a));
        demo.input(InputEvent::PointerDown {
            position: Point::new(50.0, 40.0),
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        })
        .unwrap();
        assert_eq!(demo.runtime.captured(), Some(demo.a));
        demo.input(InputEvent::PointerMoved {
            position: Point::new(600.0, 300.0),
        })
        .unwrap();
        demo.input(InputEvent::PointerUp {
            position: Point::new(600.0, 300.0),
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        })
        .unwrap();
        assert_eq!(demo.runtime.captured(), None);
        key(&mut demo, Key::Tab, false);
        assert_eq!(demo.runtime.focused(), Some(demo.b));
        key(&mut demo, Key::Enter, false);
        assert_eq!(demo.activated, 2);
        key(&mut demo, Key::Tab, true);
        assert_eq!(demo.runtime.focused(), Some(demo.a));
        key(&mut demo, Key::Tab, false);
        key(&mut demo, Key::Tab, false);
        assert_eq!(demo.runtime.focused(), Some(demo.c));
        key(&mut demo, Key::Delete, false);
        assert_eq!(demo.removed, 1);
        assert_eq!(demo.runtime.tree().len(), count - 1);
        assert_eq!(demo.runtime.focused(), None);
        demo.prepare(viewport).unwrap();
        assert_eq!(
            demo.runtime.stats().layout_passes,
            baseline.layout_passes + 1
        );
        assert_eq!(demo.prepare(viewport).unwrap(), FrameStats::default());
    }

    #[test]
    fn scroll_reuses_layout_and_resize_clamps_it() {
        let mut demo = RetainedDemo::new().unwrap();
        let viewport = Size::new(960.0, 640.0);
        demo.prepare(viewport).unwrap();
        let baseline = demo.runtime.stats().layout_passes;
        demo.input(InputEvent::Wheel {
            position: Point::new(400.0, 200.0),
            delta: Point::new(0.0, -84.0),
            modifiers: Modifiers::default(),
        })
        .unwrap();
        demo.prepare(viewport).unwrap();
        assert_eq!(demo.scroll_y, 84.0);
        assert_eq!(demo.runtime.stats().layout_passes, baseline);
        demo.prepare(Size::new(960.0, 1500.0)).unwrap();
        assert_eq!(demo.scroll_y, 0.0);
        assert!(demo.scene().rectangles.iter().all(|r| r.bounds.is_valid()));
    }
}
