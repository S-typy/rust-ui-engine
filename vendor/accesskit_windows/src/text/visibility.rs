// Copyright 2026 rust-ui-engine contributors. Apache-2.0.
// Visibility uses the same transformed run geometry as the text consumer.
use super::*;
use accesskit::Rect;

fn clip_box(mut rect: Rect, node: &NodeRef<'_>) -> Option<Rect> {
    let mut ancestor = node.parent();
    while let Some(node) = ancestor {
        if node.data().clips_children()
            && let Some(clip) = node.bounding_box()
        {
            if clip.width() <= 0.0
                || clip.height() <= 0.0
                || rect.x1 < clip.x0
                || rect.x0 > clip.x1
                || rect.y1 <= clip.y0
                || rect.y0 >= clip.y1
                || rect.width() > 0.0 && (rect.x1 <= clip.x0 || rect.x0 >= clip.x1)
            {
                return None;
            }
            rect = Rect::new(
                rect.x0.max(clip.x0),
                rect.y0.max(clip.y0),
                rect.x1.min(clip.x1),
                rect.y1.min(clip.y1),
            );
        }
        ancestor = node.parent();
    }
    Some(rect)
}

pub(super) fn clipped_boxes(range: &Range<'_>) -> Vec<Rect> {
    let boxes = range.bounding_boxes();
    if boxes.is_empty() {
        return boxes;
    }
    let mut index = 0;
    let mut result = Vec::new();
    range.traverse_text(|node, _| {
        if let Some(rect) = boxes.get(index).and_then(|rect| clip_box(*rect, node)) {
            result.push(rect);
        }
        index += 1;
        None::<()>
    });
    // Both APIs walk exactly the same runs. Never return mismatched geometry.
    if index != boxes.len() {
        return Vec::new();
    }
    result
}

pub(crate) fn visible_ranges<'a>(node: &NodeRef<'a>) -> Result<Vec<Range<'a>>> {
    let mut position = node.document_start();
    let mut pending: Option<Range<'a>> = None;
    let mut result = Vec::new();
    // Bound synchronous provider work independently of the client request.
    for _ in 0..=1_048_576 {
        position = position.biased_to_start();
        let mut character = position.to_degenerate_range();
        let next = if position.is_document_end() {
            position
        } else {
            position.forward_to_character_end()
        };
        character.set_end(next);
        if !clipped_boxes(&character).is_empty() {
            if let Some(range) = &mut pending {
                range.set_end(next);
            } else {
                pending = Some(character);
            }
        } else if let Some(range) = pending.take() {
            result.push(range);
        }
        if next.is_document_end() {
            if let Some(range) = pending {
                result.push(range);
            }
            return Ok(result);
        }
        if next == position {
            return Err(invalid_operation());
        }
        position = next;
    }
    Err(invalid_operation())
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::{Affine, Node, NodeId, Role, TextDirection, TreeId, TreeInfo, TreeUpdate};
    fn make_tree(value: &str, clip: Rect, rtl: bool) -> Tree {
        let mut root = Node::new(Role::TextInput);
        root.set_transform(Affine::scale(1.5));
        root.set_children(vec![NodeId(2)]);
        let mut viewport = Node::new(Role::GenericContainer);
        viewport.set_bounds(clip);
        viewport.set_clips_children();
        viewport.set_children(vec![NodeId(3)]);
        let mut run = Node::new(Role::TextRun);
        run.set_value(value);
        run.set_bounds(Rect::new(0.0, 0.0, value.len() as f64 * 10.0, 20.0));
        run.set_text_direction(if rtl {
            TextDirection::RightToLeft
        } else {
            TextDirection::LeftToRight
        });
        run.set_character_lengths(vec![1; value.len()]);
        run.set_character_positions(
            (0..value.len())
                .map(|i| i as f32 * 10.0)
                .collect::<Vec<_>>(),
        );
        run.set_character_widths(vec![10.0; value.len()]);
        Tree::new(
            TreeUpdate {
                nodes: vec![(NodeId(1), root), (NodeId(2), viewport), (NodeId(3), run)],
                tree: Some(TreeInfo::new(NodeId(1))),
                tree_id: TreeId::ROOT,
                focus: NodeId(1),
            },
            false,
        )
    }
    #[test]
    fn clipping_and_visible_ranges_share_transformed_character_geometry() {
        for rtl in [false, true] {
            let tree = make_tree("ABCDEF", Rect::new(12.0, 4.0, 38.0, 16.0), rtl);
            let root = tree.state().root();
            assert_eq!(
                clipped_boxes(&root.document_range()),
                vec![Rect::new(18.0, 6.0, 57.0, 24.0)]
            );
            let ranges = visible_ranges(&root).unwrap();
            assert_eq!(ranges.len(), 1);
            assert_eq!(ranges[0].text(), if rtl { "CDE" } else { "BCD" });
        }
        let tree = make_tree("ABC", Rect::new(100.0, 0.0, 150.0, 20.0), false);
        let root = tree.state().root();
        assert!(clipped_boxes(&root.document_range()).is_empty());
        assert!(visible_ranges(&root).unwrap().is_empty());
        let edge = make_tree("ABCD", Rect::new(10.0, 0.0, 30.0, 20.0), false);
        assert_eq!(
            visible_ranges(&edge.state().root()).unwrap()[0].text(),
            "BC"
        );
    }
    #[test]
    fn empty_document_keeps_a_visible_caret_and_empty_clip_hides_it() {
        let tree = make_tree("", Rect::new(0.0, 0.0, 100.0, 20.0), false);
        let root = tree.state().root();
        let ranges = visible_ranges(&root).unwrap();
        assert_eq!(ranges.len(), 1);
        assert!(ranges[0].is_degenerate());
        assert_eq!(
            clipped_boxes(&ranges[0]),
            vec![Rect::new(0.0, 0.0, 0.0, 30.0)]
        );
        let tree = make_tree("", Rect::new(0.0, 0.0, 0.0, 20.0), false);
        assert!(visible_ranges(&tree.state().root()).unwrap().is_empty());
    }
    #[test]
    fn com_scroll_preserves_exact_endpoint_and_visible_queries_are_passive() {
        use crate::{context::ActionHandlerNoMut, window_handle::WindowHandle};
        use windows::Win32::{
            Foundation::HWND,
            System::Ole::{SafeArrayDestroy, SafeArrayGetUBound},
        };
        #[derive(Default)]
        struct Recorder(std::sync::Mutex<Vec<ActionRequest>>);
        impl ActionHandlerNoMut for Recorder {
            fn do_action(&self, request: ActionRequest) {
                self.0.lock().unwrap().push(request);
            }
        }
        let handler = Arc::new(Recorder::default());
        let context = Context::new(
            WindowHandle(HWND::default()),
            make_tree("ABCDEF", Rect::new(12.0, 0.0, 38.0, 20.0), false),
            handler.clone(),
            false,
        );
        let root = context.read_tree().state().root_id();
        let provider: IRawElementProviderSimple =
            context.get_or_create_platform_node(root).into_interface();
        unsafe {
            let text: ITextProvider = provider
                .GetPatternProvider(UIA_TextPatternId)
                .unwrap()
                .cast()
                .unwrap();
            let ranges = text.GetVisibleRanges().unwrap();
            assert_eq!(SafeArrayGetUBound(ranges, 1).unwrap(), 0);
            SafeArrayDestroy(ranges).unwrap();
            assert!(handler.0.lock().unwrap().is_empty());
            let range = text.DocumentRange().unwrap();
            assert_eq!(
                range
                    .MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, 3)
                    .unwrap(),
                3
            );
            range.ScrollIntoView(true).unwrap();
            range.ScrollIntoView(false).unwrap();
        }
        let requests = handler.0.lock().unwrap();
        assert_eq!(requests.len(), 2);
        for (request, index, top) in [(&requests[0], 3, true), (&requests[1], 6, false)] {
            assert_eq!(request.action, Action::ScrollIntoView);
            assert_eq!(request.target_node, NodeId(1));
            assert_eq!(
                request.data,
                Some(ActionData::ScrollTextIntoView {
                    position: accesskit::TextPosition {
                        node: NodeId(3),
                        character_index: index
                    },
                    align_to_top: top,
                })
            );
        }
    }
}
