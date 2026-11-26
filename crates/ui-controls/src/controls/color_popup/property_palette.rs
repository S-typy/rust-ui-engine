use super::*;

const COLUMNS: usize = 8;
const CELL: f32 = 24.;
const GAP: f32 = 3.;

pub(in crate::controls) fn property_colors() -> Vec<Srgba8> {
    [
        0xffffff, 0xf1f1f1, 0xd4d4d4, 0xadadad, 0x858585, 0x5c5c5c, 0x333333, 0x000000, 0xf6c6ca,
        0xf8d0bb, 0xf7dfaa, 0xd4e6bf, 0xb4e5dc, 0xbadcf3, 0xd1c9ef, 0xebc7e7, 0xd83941, 0xe16d39,
        0xe7b13a, 0x57983c, 0x1c9c89, 0x2383c2, 0x8262ba, 0xb9509b, 0x7f1e26, 0x83391b, 0x795917,
        0x315d25, 0x125a52, 0x174d75, 0x49376f, 0x692e5a,
    ]
    .map(|rgb| Srgba8::rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8))
    .to_vec()
}

#[derive(Clone, Debug)]
pub(super) struct PropertyPalette {
    pub detail: [WidgetId; 4],
    pub palette_label: WidgetId,
    pub palette_host: WidgetId,
    pub recent_label: WidgetId,
    pub recent_host: WidgetId,
    pub mode: WidgetId,
    pub expanded: bool,
}

fn grid_height(count: usize) -> f32 {
    let rows = count.div_ceil(COLUMNS);
    rows as f32 * CELL + rows.saturating_sub(1) as f32 * GAP
}

impl Controls {
    pub(super) fn set_property_color_part(
        &mut self,
        part: WidgetId,
        show: bool,
    ) -> Result<(), ControlError> {
        self.edit_props(part, |props| {
            props.visible = show;
            props.collapsed = !show;
        })
    }

    fn arrange_property_swatches(
        &mut self,
        host: WidgetId,
        swatches: &[WidgetId],
        count: usize,
        focus: WidgetId,
    ) -> Result<(), ControlError> {
        self.edit_props(host, |props| {
            props.style.grid_rows = vec![GridTrack::Px(CELL); count.div_ceil(COLUMNS)];
            props.style.gap = GAP;
            props.style.height = Length::Px(grid_height(count));
        })?;
        // Tab visits a palette once; arrow keys move among its swatches.
        let tab_stop = swatches[..count]
            .iter()
            .position(|&b| b == focus)
            .unwrap_or(0);
        for (index, &button) in swatches.iter().enumerate() {
            self.edit_props(button, |p| {
                p.style.height = Length::Px(CELL);
                p.style.grid_column.start = Some((index % COLUMNS + 1) as u16);
                p.style.grid_row.start = Some((index / COLUMNS + 1) as u16);
                p.collapsed = index >= count;
                p.visible = index < count;
                p.tab_stop = index == tab_stop && index < count;
            })?;
        }
        Ok(())
    }

    /// Switches between swatches and exact input inside the same pending edit.
    pub(super) fn show_property_color_page(
        &mut self,
        id: WidgetId,
        expanded: bool,
    ) -> Result<WidgetId, ControlError> {
        let entry = self.color_editors.entries.get_mut(&id).unwrap();
        let popup = entry.popup.as_mut().unwrap();
        popup.property_palette.as_mut().unwrap().expanded = expanded;
        popup.revealed_focus = None;
        let popup = popup.clone();
        let page = popup.property_palette.as_ref().unwrap();
        let more = popup.more_colors.unwrap();
        let palette_count = popup.palette.len();
        let recent_count = entry.recent.len();
        let allow_empty = entry.settings.allow_empty;
        let has_error = entry.error.is_some();
        let focus = if expanded {
            popup.hex
        } else {
            let current = entry.draft.value.as_ref().and_then(|brush| match brush {
                BrushValue::Solid(color) => Some(*color),
                BrushValue::Linear(_) => None,
            });
            current
                .and_then(|color| {
                    entry
                        .settings
                        .palette
                        .iter()
                        .position(|&c| c == color)
                        .map(|index| popup.palette[index])
                        .or_else(|| {
                            entry
                                .recent
                                .iter()
                                .position(|&c| c == color)
                                .map(|index| popup.recent[index])
                        })
                })
                .or_else(|| popup.palette.first().copied())
                .unwrap_or(more)
        };
        let size = if expanded {
            Size::new(
                entry.settings.popup_size.width,
                entry.settings.popup_size.height.min(372.),
            )
        } else {
            let mut height = 16. + 8. + 28. + 8.;
            if palette_count > 0 {
                height += 22. + grid_height(palette_count) + 8.;
            }
            if recent_count > 0 {
                height += 22. + grid_height(recent_count) + 8.;
            }
            if allow_empty {
                height += 32.;
            }
            Size::new(264., height)
        };
        self.set_flyout_placement(popup.flyout, size, FlyoutPlacement::Below)?;
        self.edit_props(popup.scroll.content, |p| {
            p.style.gap = 4.;
            p.style.padding = Edges {
                right: 10.,
                ..Edges::all(2.)
            };
        })?;
        self.arrange_property_swatches(page.palette_host, &popup.palette, palette_count, focus)?;
        self.arrange_property_swatches(page.recent_host, &popup.recent, recent_count, focus)?;
        for part in page.detail {
            self.set_property_color_part(part, expanded)?;
        }
        self.set_property_color_part(popup.spectrum.as_ref().unwrap().host, expanded)?;
        for (part, visible) in [
            (page.palette_label, !expanded && palette_count > 0),
            (page.palette_host, !expanded && palette_count > 0),
            (page.recent_label, !expanded && recent_count > 0),
            (page.recent_host, !expanded && recent_count > 0),
            (page.mode, !expanded && allow_empty),
            (more, !expanded),
            (popup.gradient, false),
            (popup.solid, false),
            (popup.linear, false),
            (popup.error, has_error),
        ] {
            self.set_property_color_part(part, visible)?;
        }
        self.edit_props(page.mode, |p| p.style.height = Length::Px(28.))?;
        self.set_button_chrome(popup.empty, ButtonChrome::Flat)?;
        Ok(focus)
    }

    pub(super) fn choose_palette_color(
        &mut self,
        id: WidgetId,
        color: Srgba8,
        quick: bool,
    ) -> Result<(), ControlError> {
        self.set_color_draft(id, color)?;
        if quick {
            self.apply_color_popup(id)?;
        }
        Ok(())
    }

    pub(super) fn property_palette_key(
        &mut self,
        id: WidgetId,
        focus: WidgetId,
        key: &Key,
        repeat: bool,
    ) -> Result<bool, ControlError> {
        let entry = &self.color_editors.entries[&id];
        let popup = entry.popup.as_ref().unwrap();
        if popup
            .property_palette
            .as_ref()
            .is_none_or(|page| page.expanded)
        {
            return Ok(false);
        }
        let recent = &popup.recent[..entry.recent.len()];
        let Some((swatches, i)) =
            [popup.palette.as_slice(), recent]
                .into_iter()
                .find_map(|swatches| {
                    swatches
                        .iter()
                        .position(|&b| b == focus)
                        .map(|i| (swatches, i))
                })
        else {
            return Ok(false);
        };
        let last = swatches.len() - 1;
        let next = match key {
            Key::ArrowRight => (i + 1).min(last),
            Key::ArrowLeft => i.saturating_sub(1),
            Key::ArrowDown if i / COLUMNS < last / COLUMNS => (i + COLUMNS).min(last),
            Key::ArrowUp if i >= COLUMNS => i - COLUMNS,
            Key::ArrowDown | Key::ArrowUp => i,
            Key::Home => 0,
            Key::End => last,
            Key::Enter | Key::Space => {
                if !repeat {
                    self.activate(focus)?;
                }
                return Ok(true);
            }
            _ => return Ok(false),
        };
        let next = swatches[next];
        if focus != next {
            self.edit_props(focus, |p| p.tab_stop = false)?;
            self.edit_props(next, |p| p.tab_stop = true)?;
        }
        self.runtime.apply(Mutation::Focus(Some(next)))?;
        self.synchronize_editor_focus()?;
        Ok(true)
    }
}
