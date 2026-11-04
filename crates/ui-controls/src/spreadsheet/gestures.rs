//! Captured range/fill/resize gestures; previews never write workbook cells.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum DragKind {
    Range {
        anchor: CellAddress,
    },
    Fill {
        source: CellRange,
        target: CellRange,
    },
    Resize {
        row: bool,
        index: u32,
        start: f32,
        original: f64,
        extent: f64,
    },
}
#[derive(Clone, Copy)]
pub(super) struct Drag {
    pub kind: DragKind,
    bounds: Rect,
    generation: u64,
    selection: CellRange,
    active: CellAddress,
    anchor: CellAddress,
    first: (u32, u32),
    pointer: Point,
    origin: Point,
}

impl Spreadsheet {
    /// Cancel an owned gesture after capture loss, replacement or host teardown.
    pub fn cancel_interaction(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        if let Some(drag) = self.drag.take() {
            if drag.generation == self.session.generation() {
                self.session
                    .select_range(drag.selection, drag.active)
                    .map_err(err)?;
                self.anchor = drag.anchor;
                (self.first_row, self.first_column) = drag.first;
                self.load_entry(ui)?;
            }
            if ui.runtime.captured() == Some(self.parts.canvas) {
                ui.runtime.apply(Mutation::CapturePointer(None))?;
            }
        }
        Ok(())
    }
    pub(super) fn validate_interaction(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        if let Some(drag) = self.drag
            && (ui.runtime.captured() != Some(self.parts.canvas)
                || !ui.is_enabled(self.parts.canvas)
                || ui.runtime.bounds(self.parts.canvas) != Some(drag.bounds)
                || self.session.generation() != drag.generation)
        {
            self.cancel_interaction(ui)?;
        }
        Ok(())
    }
    /// Hosts schedule bounded autoscroll while the captured pointer is outside.
    pub fn needs_interaction_tick(&self) -> bool {
        self.drag.is_some_and(|d| {
            !matches!(d.kind, DragKind::Resize { .. })
                && !Rect::new(
                    d.bounds.x + ROW_HEADER,
                    d.bounds.y + HEADER,
                    (d.bounds.width - ROW_HEADER).max(0.0),
                    (d.bounds.height - HEADER).max(0.0),
                )
                .contains(d.pointer.x, d.pointer.y)
        })
    }
    pub fn interaction_tick(&mut self, ui: &mut Controls) -> Result<(), ControlError> {
        if self.needs_interaction_tick()
            && let Some(drag) = self.drag
        {
            self.gesture_input(
                ui,
                &InputEvent::PointerMoved {
                    position: drag.pointer,
                },
            )?;
        }
        Ok(())
    }
    fn move_drag(&mut self, ui: &mut Controls, position: Point) -> Result<(), ControlError> {
        let Some(mut drag) = self.drag else {
            return Ok(());
        };
        drag.pointer = position;
        match &mut drag.kind {
            DragKind::Resize {
                row,
                index,
                start,
                original,
                extent,
            } => {
                *extent = (*original
                    + f64::from(if *row { position.y } else { position.x } - *start))
                .clamp(14.0, 4096.0);
                SheetGeometry::with_override(
                    drag.bounds,
                    self.first_row,
                    self.first_column,
                    self.session.workbook().sheets()[self.session.sheet()].dimensions(),
                    Some((*row, *index, *extent)),
                )?;
            }
            kind => {
                let geometry = self.geometry(drag.bounds)?;
                if geometry.body.width <= 0.0 || geometry.body.height <= 0.0 {
                    return Ok(());
                }
                let step = |position: f32, start: f32, extent: f32| {
                    if position < start {
                        -1
                    } else if position >= start + extent {
                        1
                    } else {
                        0
                    }
                };
                self.first_row = self
                    .first_row
                    .saturating_add_signed(step(position.y, geometry.body.y, geometry.body.height))
                    .min(MAX_ROWS - 1);
                self.first_column = self
                    .first_column
                    .saturating_add_signed(step(position.x, geometry.body.x, geometry.body.width))
                    .min(MAX_COLUMNS - 1);
                let geometry = self.geometry(drag.bounds)?;
                let point = Point::new(
                    position.x.clamp(
                        geometry.body.x,
                        geometry.body.x + (geometry.body.width - 0.01).max(0.0),
                    ),
                    position.y.clamp(
                        geometry.body.y,
                        geometry.body.y + (geometry.body.height - 0.01).max(0.0),
                    ),
                );
                if let Some(focus) = geometry.hit(point) {
                    match kind {
                        DragKind::Range { anchor } => {
                            self.session.select(*anchor, focus).map_err(err)?;
                            drag.generation = self.session.generation();
                        }
                        DragKind::Fill { source, target } => {
                            *target = CellRange::new(
                                CellAddress::new(
                                    source.start().row().min(focus.row()),
                                    source.start().column().min(focus.column()),
                                )
                                .map_err(err)?,
                                CellAddress::new(
                                    source.end().row().max(focus.row()),
                                    source.end().column().max(focus.column()),
                                )
                                .map_err(err)?,
                            )
                            .map_err(err)?;
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
        self.drag = Some(drag);
        let _ = ui;
        Ok(())
    }
    pub(super) fn gesture_input(
        &mut self,
        ui: &mut Controls,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        self.validate_interaction(ui)?;
        if self.drag.is_some() {
            match event {
                InputEvent::WindowFocused(false) => {
                    self.cancel_interaction(ui)?;
                    return Ok(false);
                }
                InputEvent::KeyDown {
                    key: Key::Escape, ..
                } => {
                    self.cancel_interaction(ui)?;
                    return Ok(true);
                }
                InputEvent::PointerMoved { position } => {
                    if let Err(error) = self.move_drag(ui, *position) {
                        self.cancel_interaction(ui)?;
                        return Err(error);
                    }
                    return Ok(true);
                }
                InputEvent::PointerUp {
                    position,
                    button: PointerButton::Primary,
                    ..
                } => {
                    if let Err(error) = self.move_drag(ui, *position) {
                        self.cancel_interaction(ui)?;
                        return Err(error);
                    }
                    let drag = self.drag.take().expect("active drag");
                    ui.runtime.apply(Mutation::CapturePointer(None))?;
                    match drag.kind {
                        DragKind::Range { anchor } => self.anchor = anchor,
                        DragKind::Fill { target, .. } => {
                            self.session.fill_to(target).map_err(err)?
                        }
                        DragKind::Resize {
                            row,
                            index,
                            extent,
                            original,
                            ..
                        } if extent != original => {
                            self.session
                                .apply(&[if row {
                                    WorkbookEdit::RowHeight {
                                        sheet: self.session.sheet(),
                                        row: index,
                                        height: Some(extent),
                                    }
                                } else {
                                    WorkbookEdit::ColumnWidth {
                                        sheet: self.session.sheet(),
                                        column: index,
                                        width: Some(extent),
                                    }
                                }])
                                .map_err(err)?;
                        }
                        _ => {}
                    }
                    self.load_entry(ui)?;
                    let click = matches!(drag.kind, DragKind::Range { .. })
                        && self.session.selection().len() == 1
                        && (drag.origin.x - position.x).abs() < 4.0
                        && (drag.origin.y - position.y).abs() < 4.0;
                    if click {
                        let now = std::time::Instant::now();
                        let address = self.session.active();
                        let double = self.last_click.is_some_and(|(last, time)| {
                            last == address
                                && now.duration_since(time) < std::time::Duration::from_millis(500)
                        });
                        self.last_click = if double { None } else { Some((address, now)) };
                        if double {
                            self.begin_cell_edit(ui)?;
                        }
                    } else {
                        self.last_click = None;
                    }
                    return Ok(true);
                }
                InputEvent::KeyDown { .. }
                | InputEvent::Wheel { .. }
                | InputEvent::PointerDown { .. } => return Ok(true),
                _ => return Ok(false),
            }
        }
        let InputEvent::PointerDown {
            position,
            button: PointerButton::Primary,
            modifiers,
        } = event
        else {
            return Ok(false);
        };
        if !ui.is_enabled(self.parts.canvas)
            || ui.runtime.hit_test(*position) != Some(self.parts.canvas)
            || ui.runtime.captured().is_some()
        {
            return Ok(false);
        }
        let Some(bounds) = ui.runtime.bounds(self.parts.canvas) else {
            return Ok(false);
        };
        let geometry = self.geometry(bounds)?;
        let kind = if let Some((row, index)) = geometry.edge(*position) {
            let dimensions = self.session.workbook().sheets()[self.session.sheet()].dimensions();
            let original = if row {
                dimensions.row_height(index)
            } else {
                dimensions.column_width(index)
            }
            .expect("visible axis");
            DragKind::Resize {
                row,
                index,
                start: if row { position.y } else { position.x },
                original,
                extent: original,
            }
        } else if geometry
            .fill_handle(self.session.selection())
            .is_some_and(|r| r.contains(position.x, position.y))
        {
            DragKind::Fill {
                source: self.session.selection(),
                target: self.session.selection(),
            }
        } else if let Some(address) = geometry.hit(*position) {
            DragKind::Range {
                anchor: if modifiers.shift {
                    self.anchor
                } else {
                    address
                },
            }
        } else {
            return Ok(false);
        };
        self.commit(ui)?;
        let mut drag = Drag {
            kind,
            bounds,
            generation: self.session.generation(),
            selection: self.session.selection(),
            active: self.session.active(),
            anchor: self.anchor,
            first: (self.first_row, self.first_column),
            pointer: *position,
            origin: *position,
        };
        if let DragKind::Range { anchor } = kind {
            self.session
                .select(anchor, geometry.hit(*position).expect("cell hit"))
                .map_err(err)?;
            drag.generation = self.session.generation();
        }
        ui.runtime.apply(Mutation::Focus(Some(self.parts.canvas)))?;
        ui.runtime
            .apply(Mutation::CapturePointer(Some(self.parts.canvas)))?;
        self.drag = Some(drag);
        Ok(true)
    }
}
