use super::*;
use rust_desktop_ui_treegrid::{
    GridHitKind, GridKey, RowKey, SelectionModifiers, TreeDataSource, TreeMoveTarget,
};

pub(super) struct NodeDrag {
    pub(super) row: RowKey,
    pub(super) start: Point,
    pub(super) revision: u64,
    pub(super) active: bool,
    pub(super) target: Option<(TreeMoveTarget, Rect)>,
}
fn drop_target(
    frame: &rust_desktop_ui_treegrid::GridFrame,
    model: &rust_desktop_ui_treegrid::TreeGrid,
    source: &impl TreeDataSource,
    moving: RowKey,
    position: Point,
) -> Option<(TreeMoveTarget, Rect)> {
    let GridHitKind::Cell { row, column } = frame.hit_test(position)? else {
        return None;
    };
    let bounds = frame.cell_bounds(row, column)?;
    let (target, y) = if position.y < bounds.y + bounds.height / 3.0 {
        (TreeMoveTarget::Before(row), bounds.y)
    } else if position.y > bounds.y + bounds.height * 2.0 / 3.0 {
        (TreeMoveTarget::After(row), bounds.y + bounds.height - 2.0)
    } else {
        (
            TreeMoveTarget::Into(Some(row)),
            bounds.y + bounds.height - 2.0,
        )
    };
    model.validate_node_move(source, &[moving], target).ok()?;
    Some((target, Rect::new(bounds.x, y, bounds.width, 2.0)))
}
impl Controls {
    pub(super) fn tree_before_input(&mut self, event: &InputEvent) -> Result<bool, ControlError> {
        if self.popup.is_some() || self.rich.popup.is_some() {
            return Ok(false);
        }
        if matches!(event, InputEvent::WindowFocused(false)) {
            self.cancel_tree_pointers();
            return Ok(false);
        }
        let captured = self.trees.hosts.iter().find_map(|(&id, state)| {
            (state.drag.is_some() || state.model.pointer_captured() || state.scroll.captured())
                .then_some(id)
        });
        let hit = match event {
            InputEvent::PointerDown { position, .. }
            | InputEvent::PointerUp { position, .. }
            | InputEvent::PointerMoved { position }
            | InputEvent::Wheel { position, .. } => self.runtime.hit_test(*position),
            _ => self.runtime.focused(),
        };
        let Some(id) = captured
            .or(hit.filter(|id| self.trees.hosts.contains_key(id)))
            .filter(|id| self.is_enabled(*id))
        else {
            return Ok(false);
        };
        self.trees.hosts[&id]
            .source
            .check_available()
            .map_err(super::tree::tree_error)?;
        if self.trees.hosts[&id].source.revision() != self.trees.hosts[&id].revision {
            self.tree_refresh(id)?;
            if captured.is_some() {
                return Ok(true);
            }
            self.sync_trees()?;
        }
        if matches!(
            event,
            InputEvent::KeyDown {
                key: Key::Escape,
                ..
            }
        ) && captured.is_some()
        {
            let state = self.trees.hosts.get_mut(&id).unwrap();
            state.drag = None;
            state.scroll.cancel();
            state.model.cancel_pointer();
            self.runtime.apply(Mutation::CapturePointer(None))?;
            self.scene_dirty = true;
            return Ok(true);
        }
        let recognized = matches!(
            event,
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                ..
            } | InputEvent::Wheel { .. }
                | InputEvent::KeyDown {
                    key: Key::ArrowLeft
                        | Key::ArrowRight
                        | Key::ArrowUp
                        | Key::ArrowDown
                        | Key::Home
                        | Key::End
                        | Key::PageUp
                        | Key::PageDown
                        | Key::Enter,
                    ..
                }
        ) || (captured.is_some()
            && matches!(
                event,
                InputEvent::PointerMoved { .. }
                    | InputEvent::PointerUp {
                        button: PointerButton::Primary,
                        ..
                    }
            ));
        if !recognized {
            return Ok(false);
        }
        let report = self.runtime.dispatch(event.clone());
        if let Some(error) = report.errors.into_iter().next() {
            return Err(error.into());
        }
        if !self.runtime.tree().contains(id) {
            self.prune_trees();
            return Ok(true);
        }
        if report.default_prevented {
            return Ok(true);
        }
        self.trees.hosts[&id]
            .source
            .check_available()
            .map_err(super::tree::tree_error)?;
        if self.trees.hosts[&id].source.revision() != self.trees.hosts[&id].revision {
            self.tree_refresh(id)?;
            return Ok(true);
        }
        self.synchronize_editor_focus()?;
        let selection_event = matches!(
            event,
            InputEvent::PointerDown { .. } | InputEvent::KeyDown { .. }
        );
        let before = selection_event.then(|| self.tree_selected_keys(id).unwrap());
        let snapshot = (selection_event && self.trees.hosts[&id].selection_validator.is_some())
            .then(|| self.trees.hosts[&id].model.selection_snapshot());
        let result = {
            let state = self.trees.hosts.get_mut(&id).unwrap();
            if state
                .scroll
                .input(&mut state.model, event)
                .map_err(super::tree::tree_error)?
            {
                Ok(())
            } else {
                match event {
                    InputEvent::PointerDown {
                        position,
                        button: PointerButton::Primary,
                        modifiers,
                    } => {
                        let result = state
                            .model
                            .pointer_down(
                                &mut state.source,
                                &state.frame,
                                *position,
                                SelectionModifiers {
                                    shift: modifiers.shift
                                        && state.settings.selection_mode == SelectionMode::Multiple,
                                    control: modifiers.control
                                        && state.settings.selection_mode == SelectionMode::Multiple,
                                },
                            )
                            .map(|_| ());
                        if result.is_ok()
                            && state.settings.allow_node_drag
                            && !state.settings.read_only
                            && !modifiers.shift
                            && !modifiers.control
                            && let Some(GridHitKind::Cell { row, .. }) =
                                state.frame.hit_test(*position)
                        {
                            state.drag = Some(NodeDrag {
                                row,
                                start: *position,
                                revision: state.source.revision(),
                                active: false,
                                target: None,
                            });
                        }
                        result
                    }
                    InputEvent::PointerMoved { position } => {
                        if let Some(drag) = &mut state.drag {
                            if drag.revision != state.source.revision() {
                                state.drag = None;
                                Ok(())
                            } else {
                                drag.active |= (position.x - drag.start.x)
                                    .abs()
                                    .max((position.y - drag.start.y).abs())
                                    >= 5.0;
                                drag.target = None;
                                if drag.active {
                                    drag.target = drop_target(
                                        &state.frame,
                                        &state.model,
                                        &state.source,
                                        drag.row,
                                        *position,
                                    );
                                }
                                Ok(())
                            }
                        } else {
                            state.model.pointer_move(*position).map(|_| ())
                        }
                    }
                    InputEvent::PointerUp {
                        position,
                        button: PointerButton::Primary,
                        ..
                    } => {
                        if let Some(drag) = state.drag.take() {
                            if drag.active
                                && drag.revision == state.source.revision()
                                && let Some((target, _)) = drop_target(
                                    &state.frame,
                                    &state.model,
                                    &state.source,
                                    drag.row,
                                    *position,
                                )
                            {
                                let result =
                                    state
                                        .model
                                        .move_nodes(&mut state.source, &[drag.row], target);
                                if result.is_ok() {
                                    state.revision = state.source.revision();
                                }
                                result
                            } else {
                                Ok(())
                            }
                        } else {
                            state
                                .model
                                .pointer_up(&mut state.source, &state.frame, *position)
                                .map(|_| ())
                        }
                    }
                    InputEvent::Wheel {
                        delta, modifiers, ..
                    } => {
                        state.drag = None;
                        state.scroll.cancel();
                        state.model.cancel_pointer();
                        if modifiers.shift {
                            state.model.scroll_by(-f64::from(delta.x + delta.y), 0.0)
                        } else {
                            state
                                .model
                                .scroll_by(-f64::from(delta.x), -f64::from(delta.y))
                        }
                    }
                    InputEvent::KeyDown { key, modifiers, .. }
                        if !modifiers.alt && !modifiers.super_key =>
                    {
                        if modifiers.control && matches!(key, Key::ArrowLeft | Key::ArrowRight) {
                            state
                                .model
                                .navigate_column(&state.source, *key == Key::ArrowRight)
                        } else if *key == Key::Enter {
                            if let Some(row) = state.model.focused() {
                                state
                                    .model
                                    .set_expanded(
                                        &mut state.source,
                                        row,
                                        !state.model.is_expanded(row),
                                    )
                                    .map(|_| ())
                            } else {
                                Ok(())
                            }
                        } else {
                            let key = match key {
                                Key::ArrowLeft => GridKey::Left,
                                Key::ArrowRight => GridKey::Right,
                                Key::ArrowUp => GridKey::Up,
                                Key::ArrowDown => GridKey::Down,
                                Key::Home => GridKey::Home,
                                Key::End => GridKey::End,
                                Key::PageUp => GridKey::PageUp,
                                _ => GridKey::PageDown,
                            };
                            state.model.navigate(
                                &mut state.source,
                                key,
                                SelectionModifiers {
                                    shift: modifiers.shift
                                        && state.settings.selection_mode == SelectionMode::Multiple,
                                    control: modifiers.control
                                        && state.settings.selection_mode == SelectionMode::Multiple,
                                },
                            )
                        }
                    }
                    _ => Ok(()),
                }
            }
        };
        let state = self.trees.hosts.get_mut(&id).unwrap();
        state.error = result.err().map(|error| error.to_string());
        if let Some(snapshot) = snapshot
            && self.tree_selected_keys(id) != before
        {
            let _ = self.validate_tree_selection(id, snapshot);
        }
        let state = self.trees.hosts.get_mut(&id).unwrap();
        let capture =
            state.drag.is_some() || state.model.pointer_captured() || state.scroll.captured();
        if capture {
            self.runtime.apply(Mutation::CapturePointer(Some(id)))?;
        } else if captured.is_some() {
            self.runtime.apply(Mutation::CapturePointer(None))?;
        }
        if let Some(before) = before {
            self.record_tree_selection(id, &before);
        }
        self.scene_dirty = true;
        Ok(true)
    }
}
