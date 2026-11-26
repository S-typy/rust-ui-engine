use super::*;

// Put most of the grip on the label side so the first character stays clickable.
const GRIP_IN_LABEL: f32 = 6.;
const GRIP_IN_VALUE: f32 = 2.;
const MIN_NAME: f32 = 64.;
const MIN_VALUE: f32 = 80.;

fn name_limits(row_width: f32) -> (f32, f32) {
    if row_width <= MIN_NAME + MIN_VALUE {
        let name = row_width * (MIN_NAME / (MIN_NAME + MIN_VALUE));
        (name, name)
    } else {
        (MIN_NAME, row_width - MIN_VALUE)
    }
}

impl PropertyGrid {
    pub(super) fn divider_geometry(&self, ui: &Controls, row: WidgetId) -> Option<(Rect, f32)> {
        let property = self.properties.iter().find(|p| p.row == row)?;
        if property.descriptor.full_width || !ui.is_enabled(row) {
            return None;
        }
        let bounds = ui.runtime.bounds(row)?;
        let value = ui.runtime.bounds(property.host)?;
        Some((bounds, value.x - 1.))
    }

    pub(super) fn divider_at(&self, ui: &Controls, point: Point) -> Option<WidgetId> {
        let mut hit = ui.runtime.hit_test(point)?;
        loop {
            if matches!(
                ui.controls.get(&hit)?.inspector_part,
                Some(InspectorPart::Row { .. })
            ) {
                let (_, divider) = self.divider_geometry(ui, hit)?;
                return ((divider - GRIP_IN_LABEL..=divider + GRIP_IN_VALUE).contains(&point.x))
                    .then_some(hit);
            }
            if hit == self.container {
                return None;
            }
            hit = ui.runtime.tree().parent(hit)?;
        }
    }

    pub(super) fn divider_track_for_x(
        &self,
        ui: &Controls,
        row: WidgetId,
        x: f32,
    ) -> Option<GridTrack> {
        let (bounds, _) = self.divider_geometry(ui, row)?;
        if !x.is_finite() || bounds.width <= 0. {
            return None;
        }
        let (min, max) = name_limits(bounds.width);
        let width = (x - bounds.x).clamp(min, max);
        match self.label_track {
            GridTrack::Px(_) => Some(GridTrack::Px(width)),
            GridTrack::Percent(_) => Some(GridTrack::Percent(width / bounds.width)),
            _ => None,
        }
    }

    pub(super) fn set_label_track(
        &mut self,
        ui: &mut Controls,
        track: GridTrack,
    ) -> Result<(), ControlError> {
        if self.label_track == track {
            return Ok(());
        }
        self.apply_column_widths(ui, track)?;
        self.label_track = track;
        Ok(())
    }

    /// Fit the preferred name width to the latest arranged rows. A host that
    /// changes the viewport can call this after layout and lay out again if needed.
    pub fn fit_columns(&self, ui: &mut Controls) -> Result<bool, ControlError> {
        if matches!(self.label_track, GridTrack::Px(_)) {
            self.apply_column_widths(ui, self.label_track)
        } else {
            Ok(false)
        }
    }

    fn apply_column_widths(
        &self,
        ui: &mut Controls,
        track: GridTrack,
    ) -> Result<bool, ControlError> {
        // Validate the required tracks before changing any row, including hidden groups.
        let mut styles = Vec::new();
        for p in self.properties.iter().filter(|p| !p.descriptor.full_width) {
            let node = ui
                .runtime
                .tree()
                .node(p.row)
                .ok_or_else(|| invalid("Missing property row"))?;
            let current = node
                .props
                .style
                .grid_columns
                .first()
                .ok_or_else(|| invalid("Missing name column"))?;
            // Keep the preferred DIP width for when the viewport grows again.
            let fitted = match (track, ui.runtime.layout_bounds(p.row)) {
                (GridTrack::Px(preferred), Some(row)) if row.width > 0. => {
                    GridTrack::Px(preferred.min(name_limits(row.width).1))
                }
                _ => track,
            };
            if *current == fitted {
                continue;
            }
            let mut style = node.props.style.clone();
            style.grid_columns[0] = fitted;
            styles.push(Mutation::SetStyle { node: p.row, style });
        }
        let changed = !styles.is_empty();
        for change in styles {
            ui.runtime.apply(change)?;
        }
        ui.scene_dirty |= changed;
        Ok(changed)
    }

    pub(super) fn highlight_column_dividers(&self, ui: &mut Controls, highlighted: bool) {
        for p in &self.properties {
            if let Some(InspectorPart::Row {
                divider_highlighted,
                ..
            }) = ui
                .controls
                .get_mut(&p.row)
                .and_then(|c| c.inspector_part.as_mut())
            {
                *divider_highlighted = highlighted;
            }
        }
        ui.scene_dirty = true;
    }
}
