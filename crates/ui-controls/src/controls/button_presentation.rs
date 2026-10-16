use super::*;

const SPLIT_PRIMARY_HEIGHT: f32 = 40.0;
const SPLIT_ARROW_HEIGHT: f32 = 20.0;
const SPLIT_ARROW_WIDTH: f32 = 24.0;
const SPLIT_CAPTION_ARROW_WIDTH: f32 = 20.0;
const SPLIT_PADDING: f32 = 4.0;

fn validate_content_measure(
    label: &str,
    style: &ButtonStyle,
    icon: Option<ButtonIcon>,
    available_width: f32,
) -> Result<(), ControlError> {
    style.validate()?;
    if let Some(icon) = icon {
        icon.validate()?;
    }
    TextLimits::default().validate(label).map_err(edit_error)?;
    if !available_width.is_finite() || !(0.0..=16_384.0).contains(&available_width) {
        return Err(ControlError::Invalid(
            "Button measure width must be finite and in [0, 16384]".into(),
        ));
    }
    Ok(())
}

impl Controls {
    fn require_content_button(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.runtime.tree().node(id).is_some()
            && self
                .controls
                .get(&id)
                .is_some_and(|control| matches!(control.kind, Kind::Button | Kind::Toggle))
        {
            Ok(())
        } else {
            Err(ControlError::Invalid(
                "Content presentation requires a Button or ToggleButton".into(),
            ))
        }
    }

    /// Changes only the visual arrangement; command, label, focus and ID remain.
    pub fn set_button_content_layout(
        &mut self,
        id: WidgetId,
        layout: ButtonContentLayout,
    ) -> Result<(), ControlError> {
        self.require_content_button(id)?;
        let control = self.controls.get_mut(&id).unwrap();
        if control.button_content_layout != layout {
            control.button_content_layout = layout;
            self.scene_dirty = true;
        }
        Ok(())
    }

    pub fn button_content_layout(&self, id: WidgetId) -> Option<ButtonContentLayout> {
        self.require_content_button(id).ok()?;
        Some(self.controls[&id].button_content_layout)
    }

    pub fn set_button_chrome(
        &mut self,
        id: WidgetId,
        chrome: ButtonChrome,
    ) -> Result<(), ControlError> {
        self.require_content_button(id)?;
        let control = self.controls.get_mut(&id).unwrap();
        if control.button_chrome != chrome {
            control.button_chrome = chrome;
            if chrome == ButtonChrome::Link {
                control.role = Some(Role::Link);
            } else if control.role == Some(Role::Link) {
                control.role = None;
            }
            self.scene_dirty = true;
        }
        Ok(())
    }

    pub fn button_chrome(&self, id: WidgetId) -> Option<ButtonChrome> {
        self.require_content_button(id).ok()?;
        Some(self.controls[&id].button_chrome)
    }

    /// Adds a paint-only navigation badge. This does not register a shortcut,
    /// alter accessibility text, change focus or invalidate the layout.
    /// Text uses the shared one-MiB limit and consists of alphanumeric characters.
    pub fn set_key_tip(&mut self, id: WidgetId, text: Option<String>) -> Result<(), ControlError> {
        self.require_content_button(id)?;
        if let Some(text) = &text {
            TextLimits::default().validate(text).map_err(edit_error)?;
            if text.is_empty() || !text.chars().all(char::is_alphanumeric) {
                return Err(ControlError::Invalid(
                    "KeyTip must contain alphanumeric characters".into(),
                ));
            }
        }
        let control = self.controls.get_mut(&id).unwrap();
        if control.key_tip != text {
            control.key_tip = text;
            self.scene_dirty = true;
        }
        Ok(())
    }

    pub fn key_tip(&self, id: WidgetId) -> Option<&str> {
        self.require_content_button(id).ok()?;
        self.controls[&id].key_tip.as_deref()
    }

    /// Measures the same content plan used by painting, including style padding.
    /// `available_width` is the outer width in logical pixels, in `[0, 16384]`.
    /// Above keeps that width and limits its visual caption to two lines; other
    /// layouts return their natural width capped to the available width.
    pub fn measure_button_content(
        &mut self,
        label: &str,
        style: &ButtonStyle,
        icon: Option<ButtonIcon>,
        layout: ButtonContentLayout,
        available_width: f32,
    ) -> Result<Size, ControlError> {
        validate_content_measure(label, style, icon, available_width)?;
        let style = style.resolve(self.theme, ButtonVisualState::default(), false);
        let content_width = (available_width - style.padding.horizontal()).max(0.0);
        let plan = content_plan(
            &mut self.engine,
            label,
            icon,
            layout,
            content_width,
            style.font_size,
        )?;
        Ok(Size::new(
            if layout == ButtonContentLayout::Above {
                available_width
            } else {
                (plan.size.width + style.padding.horizontal()).min(available_width)
            },
            plan.size.height + style.padding.vertical(),
        ))
    }

    /// Measures the two areas installed by `set_split_button_presentation`.
    /// The caption footer has its own padding and reserves the popup indicator
    /// width; measuring it as an ordinary Above button can lose a wrapped line.
    pub(crate) fn measure_split_button_content(
        &mut self,
        label: &str,
        style: &ButtonStyle,
        icon: Option<ButtonIcon>,
        layout: ButtonContentLayout,
        available_width: f32,
    ) -> Result<Size, ControlError> {
        validate_content_measure(label, style, icon, available_width)?;
        let style = ButtonStyle {
            padding: Some(Edges::all(SPLIT_PADDING)),
            ..*style
        };
        match layout {
            ButtonContentLayout::Above => {
                let caption_width = if icon.is_some() {
                    (available_width - SPLIT_CAPTION_ARROW_WIDTH).max(0.0)
                } else {
                    available_width
                };
                let caption = self.measure_button_content(
                    label,
                    &style,
                    None,
                    ButtonContentLayout::Above,
                    caption_width,
                )?;
                Ok(Size::new(
                    available_width,
                    if icon.is_some() {
                        SPLIT_PRIMARY_HEIGHT + caption.height.max(SPLIT_ARROW_WIDTH)
                    } else {
                        caption.height + SPLIT_ARROW_HEIGHT
                    },
                ))
            }
            ButtonContentLayout::Leading | ButtonContentLayout::IconOnly => {
                let primary = self.measure_button_content(
                    label,
                    &style,
                    icon,
                    layout,
                    (available_width - SPLIT_ARROW_WIDTH).max(0.0),
                )?;
                Ok(Size::new(
                    (primary.width + SPLIT_ARROW_WIDTH).min(available_width),
                    primary.height.max(SPLIT_ARROW_WIDTH),
                ))
            }
        }
    }

    pub(crate) fn set_ribbon_command(&mut self, id: WidgetId) -> Result<(), ControlError> {
        self.require_content_button(id)?;
        self.controls.get_mut(&id).unwrap().ribbon_command = true;
        self.scene_dirty = true;
        Ok(())
    }

    pub(crate) fn set_ribbon_surface(&mut self, id: WidgetId) -> Result<(), ControlError> {
        if self.runtime.tree().node(id).is_none()
            || !self
                .controls
                .get(&id)
                .is_some_and(|control| matches!(control.kind, Kind::Panel))
        {
            return Err(ControlError::Invalid(
                "Ribbon surface requires a Panel".into(),
            ));
        }
        self.controls.get_mut(&id).unwrap().ribbon_surface = true;
        self.scene_dirty = true;
        Ok(())
    }

    pub(crate) fn set_ribbon_group_caption(&mut self, id: WidgetId) -> Result<(), ControlError> {
        if self.runtime.tree().node(id).is_none()
            || !self
                .controls
                .get(&id)
                .is_some_and(|control| matches!(control.kind, Kind::Label))
        {
            return Err(ControlError::Invalid(
                "Ribbon caption requires a Label".into(),
            ));
        }
        self.controls.get_mut(&id).unwrap().ribbon_caption = true;
        self.scene_dirty = true;
        Ok(())
    }

    /// Opt-in composition of existing split parts; ordinary SplitButton keeps
    /// its default presentation and popup behavior.
    pub(crate) fn set_split_button_presentation(
        &mut self,
        split: SplitButton,
        layout: ButtonContentLayout,
    ) -> Result<(), ControlError> {
        self.require_content_button(split.primary)?;
        self.require_content_button(split.dropdown)?;
        if split.primary == split.dropdown
            || self.runtime.tree().parent(split.primary) != Some(split.container)
            || self.runtime.tree().parent(split.dropdown) != Some(split.container)
            || !self
                .controls
                .get(&split.container)
                .is_some_and(|control| matches!(control.kind, Kind::Panel))
        {
            return Err(ControlError::Invalid(
                "Split presentation requires two distinct child buttons".into(),
            ));
        }
        let above = layout == ButtonContentLayout::Above;
        let has_icon = self.controls[&split.primary].button_icon.is_some();
        self.edit_props(split.container, |props| {
            props.style.kind = LayoutKind::Flex(if above {
                Axis::Vertical
            } else {
                Axis::Horizontal
            });
            props.style.gap = 0.0;
        })?;
        self.edit_props(split.primary, |props| {
            props.style.width = if above {
                Length::Percent(1.0)
            } else {
                Length::Px(0.0)
            };
            props.style.height = if above {
                Length::Px(if has_icon { SPLIT_PRIMARY_HEIGHT } else { 0.0 })
            } else {
                Length::Percent(1.0)
            };
            props.style.flex_grow = if above && has_icon { 0.0 } else { 1.0 };
            props.style.flex_shrink = 0.0;
        })?;
        self.edit_props(split.dropdown, |props| {
            props.style.width = if above {
                Length::Percent(1.0)
            } else {
                Length::Px(SPLIT_ARROW_WIDTH)
            };
            props.style.height = if above {
                Length::Px(if has_icon { 0.0 } else { SPLIT_ARROW_HEIGHT })
            } else {
                Length::Percent(1.0)
            };
            props.style.flex_grow = if above && has_icon { 1.0 } else { 0.0 };
            props.style.flex_shrink = 0.0;
        })?;
        self.controls.get_mut(&split.container).unwrap().split_parts = Some(split);
        for id in [split.primary, split.dropdown] {
            let control = self.controls.get_mut(&id).unwrap();
            control.split_owner = Some(split.container);
            control.button_chrome = ButtonChrome::Flat;
            control.button_style.padding = Some(Edges::all(SPLIT_PADDING));
        }
        self.controls
            .get_mut(&split.primary)
            .unwrap()
            .button_content_layout = if above && has_icon {
            ButtonContentLayout::IconOnly
        } else if above {
            ButtonContentLayout::Above
        } else {
            layout
        };
        self.controls
            .get_mut(&split.dropdown)
            .unwrap()
            .button_content_layout = if above {
            ButtonContentLayout::Above
        } else {
            ButtonContentLayout::Leading
        };
        self.scene_dirty = true;
        Ok(())
    }

    pub(super) fn paint_split_surface(&mut self, split: SplitButton, bounds: Rect, clip: Rect) {
        let Some(primary) = self.button_visual_state(split.primary) else {
            return;
        };
        let Some(dropdown) = self.button_visual_state(split.dropdown) else {
            return;
        };
        let state = ButtonVisualState {
            enabled: primary.enabled || dropdown.enabled,
            hovered: primary.hovered || dropdown.hovered,
            pressed: false,
            focused: false,
            checked: primary.checked,
        };
        let style = self.controls[&split.primary].button_style.resolve_chrome(
            self.theme,
            state,
            ButtonChrome::Flat,
        );
        editor_fill(
            &mut self.scene,
            bounds,
            clip,
            style.corner_radius,
            style.background,
        );
        if state.hovered || state.checked {
            shape_border(
                &mut self.scene,
                bounds,
                clip,
                style.corner_radius,
                1.0,
                self.theme.tokens().border,
            );
            let Some(dropdown_bounds) = self.runtime.bounds(split.dropdown) else {
                return;
            };
            let rule = if self.controls[&split.dropdown].button_content_layout
                == ButtonContentLayout::Above
            {
                Rect::new(
                    bounds.x + 3.0,
                    dropdown_bounds.y,
                    (bounds.width - 6.0).max(0.0),
                    1.0,
                )
            } else {
                Rect::new(
                    dropdown_bounds.x,
                    bounds.y + 3.0,
                    1.0,
                    (bounds.height - 6.0).max(0.0),
                )
            };
            fill_clipped(&mut self.scene, rule, clip, self.theme.tokens().border);
        }
    }

    pub(super) fn button_paint_label(&self, id: WidgetId) -> &str {
        let control = &self.controls[&id];
        if let Some(owner) = control.split_owner
            && let Some(split) = self
                .controls
                .get(&owner)
                .and_then(|control| control.split_parts)
            && split.dropdown == id
            && control.button_content_layout == ButtonContentLayout::Above
        {
            return self
                .controls
                .get(&split.primary)
                .filter(|control| control.button_icon.is_some())
                .map_or("", |control| control.label.as_str());
        }
        control.display_label(&self.commands)
    }

    pub(super) fn paint_key_tips(&mut self) -> Result<(), ControlError> {
        let viewport = Rect::new(0.0, 0.0, self.viewport.width, self.viewport.height);
        let tokens = self.theme.tokens();
        for id in self.runtime.tree().preorder() {
            let Some(text) = self
                .controls
                .get(&id)
                .and_then(|control| control.key_tip.clone())
            else {
                continue;
            };
            let (Some(anchor), Some(clip)) = (self.runtime.bounds(id), self.runtime.clip(id))
            else {
                continue;
            };
            if anchor
                .intersection(clip)
                .and_then(|bounds| bounds.intersection(viewport))
                .is_none()
            {
                continue;
            }
            let measured = self.measure_text_with_font_size(&text, 512.0, false, 11.0)?;
            let width = (measured.width.ceil() + 10.0)
                .clamp(20.0, 512.0)
                .min(viewport.width);
            let height = (measured.height.ceil() + 4.0).min(viewport.height);
            let bounds = Rect::new(
                (anchor.x + (anchor.width - width) * 0.5)
                    .clamp(0.0, (viewport.width - width).max(0.0)),
                (anchor.y + anchor.height - height * 0.5)
                    .clamp(0.0, (viewport.height - height).max(0.0)),
                width,
                height,
            );
            self.scene
                .rounded_fill(bounds, viewport, 3.0, tokens.surface);
            self.scene
                .rounded_stroke(bounds, viewport, 3.0, 1.0, tokens.border);
            let content = Rect::new(
                bounds.x + 3.0,
                bounds.y + 2.0,
                (bounds.width - 6.0).max(0.0),
                (bounds.height - 4.0).max(0.0),
            );
            self.scene.text(TextRun {
                bounds: content,
                clip: content
                    .intersection(viewport)
                    .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
                text,
                font_size: 11.0,
                color: if self.is_enabled(id) {
                    tokens.text
                } else {
                    tokens.muted
                },
                weight: 400,
                align: TextAlign::Center,
                wrap: false,
            });
        }
        Ok(())
    }
}

struct ContentPlan {
    size: Size,
    caption: String,
    caption_size: Size,
    icon_size: f32,
    gap: f32,
}

fn shape(
    engine: &mut TextEngine,
    text: &str,
    width: f32,
    font_size: f32,
    wrap: bool,
) -> Result<PreparedText, ControlError> {
    engine
        .prepare(
            &TextRun {
                bounds: Rect::new(0.0, 0.0, width, 0.0),
                clip: Rect::new(0.0, 0.0, 0.0, 0.0),
                text: text.to_owned(),
                font_size,
                color: Color::rgb(0, 0, 0),
                weight: 400,
                align: TextAlign::Start,
                wrap,
            },
            1.0,
        )
        .map_err(|error| ControlError::Text(error.to_string()))
}

fn fits_lines(prepared: &PreparedText, text: &str, width: f32, max_lines: usize) -> bool {
    if prepared.width > width + 0.001 {
        return false;
    }
    let mut rows: Vec<_> = prepared
        .selection_rects(0..text.len())
        .into_iter()
        .map(|rect| rect.y)
        .collect();
    rows.sort_by(f32::total_cmp);
    rows.dedup_by(|left, right| (*left - *right).abs() < 0.1);
    rows.len() <= max_lines
}

fn limited_caption(
    engine: &mut TextEngine,
    label: &str,
    width: f32,
    font_size: f32,
    max_lines: usize,
) -> Result<(String, Size), ControlError> {
    if width <= 0.0 || label.is_empty() {
        return Ok((String::new(), Size::ZERO));
    }
    let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
    let prepared = shape(engine, &label, width, font_size, max_lines > 1)?;
    if fits_lines(&prepared, &label, width, max_lines) {
        return Ok((label, Size::new(prepared.width, prepared.height)));
    }
    let ellipsis = shape(engine, "…", width, font_size, false)?;
    if ellipsis.width > width {
        return Ok((String::new(), Size::ZERO));
    }
    let boundaries = TextDocument::new(label.clone()).grapheme_boundaries();
    let mut low = 0;
    let mut high = boundaries.len().saturating_sub(1);
    let mut best = ("…".to_owned(), Size::new(ellipsis.width, ellipsis.height));
    while low <= high {
        let middle = low + (high - low) / 2;
        let candidate = format!("{}…", label[..boundaries[middle]].trim_end());
        let prepared = shape(engine, &candidate, width, font_size, max_lines > 1)?;
        if fits_lines(&prepared, &candidate, width, max_lines) {
            best = (candidate, Size::new(prepared.width, prepared.height));
            low = middle + 1;
        } else if middle == 0 {
            break;
        } else {
            high = middle - 1;
        }
    }
    Ok(best)
}

fn content_plan(
    engine: &mut TextEngine,
    label: &str,
    icon: Option<ButtonIcon>,
    layout: ButtonContentLayout,
    width: f32,
    font_size: f32,
) -> Result<ContentPlan, ControlError> {
    let icon_size = icon.map_or(0.0, |icon| icon.size.min(width));
    let hide_caption = layout == ButtonContentLayout::IconOnly && icon.is_some();
    let gap = if hide_caption || label.is_empty() {
        0.0
    } else {
        icon.map_or(0.0, |icon| icon.spacing)
    };
    let caption_width = if layout == ButtonContentLayout::Above {
        width
    } else {
        (width - icon_size - gap).max(0.0)
    };
    let (caption, caption_size) = if hide_caption || label.is_empty() || caption_width <= 0.0 {
        (String::new(), Size::ZERO)
    } else if layout == ButtonContentLayout::Above {
        limited_caption(engine, label, caption_width, font_size, 2)?
    } else {
        let prepared = shape(engine, label, caption_width, font_size, false)?;
        (
            label.to_owned(),
            Size::new(prepared.width.min(caption_width), prepared.height),
        )
    };
    let size = if layout == ButtonContentLayout::Above {
        Size::new(width, icon_size + gap + caption_size.height)
    } else {
        Size::new(
            (icon_size + gap + caption_size.width).min(width),
            icon_size.max(caption_size.height),
        )
    };
    Ok(ContentPlan {
        size,
        caption,
        caption_size,
        icon_size,
        gap,
    })
}

pub(super) struct ContentPresentation {
    pub icon: Option<ButtonIcon>,
    pub layout: ButtonContentLayout,
    pub top_aligned: bool,
    pub start_aligned: bool,
    pub icon_theme: Option<Theme>,
}

pub(super) fn paint_content(
    scene: &mut Scene,
    engine: &mut TextEngine,
    content: Rect,
    ancestor_clip: Rect,
    label: &str,
    style: crate::button::ResolvedButtonStyle,
    presentation: ContentPresentation,
) -> Result<(), ControlError> {
    if content.width <= 0.0 || content.height <= 0.0 {
        return Ok(());
    }
    let Some(clip) = content.intersection(ancestor_clip) else {
        return Ok(());
    };
    let ContentPresentation {
        icon,
        layout,
        top_aligned,
        start_aligned,
        icon_theme,
    } = presentation;
    let plan = content_plan(engine, label, icon, layout, content.width, style.font_size)?;
    let above = layout == ButtonContentLayout::Above;
    let icon_size = plan.icon_size.min(content.height);
    if let Some(icon) = icon {
        let icon_bounds = Rect::new(
            if above || layout == ButtonContentLayout::IconOnly {
                content.x + (content.width - icon_size) * 0.5
            } else if start_aligned {
                content.x
            } else {
                content.x + (content.width - plan.size.width) * 0.5
            },
            if above {
                content.y
            } else {
                content.y + (content.height - icon_size) * 0.5
            },
            icon_size,
            icon_size,
        );
        if let Some(theme) = icon_theme {
            crate::primitives::paint_icon_themed(
                scene,
                icon.symbol,
                icon_bounds,
                clip,
                style.foreground,
                theme,
            );
        } else {
            crate::primitives::paint_icon(scene, icon.symbol, icon_bounds, clip, style.foreground);
        }
    }
    if !plan.caption.is_empty() {
        let bounds = if above {
            let offset = if icon.is_some() {
                icon_size + plan.gap
            } else if top_aligned {
                0.0
            } else {
                ((content.height - plan.caption_size.height) * 0.5).max(0.0)
            };
            Rect::new(
                content.x,
                content.y + offset,
                content.width,
                plan.caption_size
                    .height
                    .min((content.height - offset).max(0.0)),
            )
        } else {
            let top = ((content.height - plan.caption_size.height) * 0.5).max(0.0);
            Rect::new(
                content.x
                    + if start_aligned {
                        0.0
                    } else {
                        (content.width - plan.size.width) * 0.5
                    }
                    + icon_size
                    + plan.gap,
                content.y + top,
                plan.caption_size.width,
                (content.height - top).max(0.0),
            )
        };
        scene.text(TextRun {
            bounds,
            clip: bounds
                .intersection(clip)
                .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
            text: plan.caption,
            font_size: style.font_size,
            color: style.foreground,
            weight: 400,
            align: if above {
                TextAlign::Center
            } else {
                TextAlign::Start
            },
            wrap: above,
        });
    }
    Ok(())
}

pub(super) fn paint_group_caption(
    scene: &mut Scene,
    engine: &mut TextEngine,
    bounds: Rect,
    clip: Rect,
    label: &str,
    theme: Theme,
) -> Result<(), ControlError> {
    let width = (bounds.width - 8.0).max(0.0);
    let font_size = if theme.is_fluent() {
        12.0
    } else {
        theme.tokens().font_size
    };
    let (caption, size) = limited_caption(engine, label, width, font_size, 1)?;
    if !caption.is_empty() {
        let text_bounds = Rect::new(
            bounds.x + 4.0,
            bounds.y + ((bounds.height - size.height) * 0.5).max(0.0),
            width,
            size.height.min(bounds.height),
        );
        scene.text(TextRun {
            bounds: text_bounds,
            clip: text_bounds
                .intersection(clip)
                .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
            text: caption,
            font_size,
            color: theme.tokens().muted,
            weight: 400,
            align: TextAlign::Center,
            wrap: false,
        });
    }
    Ok(())
}
