//! Column-local filter popup. Layout is shared by painting and input.
use super::*;
use rust_desktop_ui_core::StrokeSegment;

const ROW: f32 = 24.;

struct Entry {
    value: CellValue,
    label: String,
    folded: String,
    count: usize,
    checked: bool,
}

pub(super) struct FilterPopup {
    pub(super) field: String,
    pub(super) rule: String,
    pub(super) error: Option<String>,
    rows: Vec<Entry>,
    hidden_selection: Vec<CellValue>,
    matches: Vec<usize>,
    matched_count: usize,
    checked_matches: usize,
    select_text: bool,
    search: String,
    cursor: usize,
    first: usize,
    dirty: bool,
    applied: bool,
    rules: bool,
    rule_hint: &'static str,
    typing: bool,
}

pub(super) enum Action {
    Stay,
    Close,
    Apply,
    Clear,
}

struct Layout {
    area: Rect,
    rules: Rect,
    values: Rect,
    input: Rect,
    all: Rect,
    list: Rect,
    clear: Rect,
    apply: Rect,
}

impl Layout {
    fn new(bounds: Rect, anchor: Rect, rows: usize) -> Self {
        let w = bounds.width.min(314.);
        let desired = bounds.height.min(132. + rows.clamp(2, 7) as f32 * ROW);
        let below = (bounds.y + bounds.height - anchor.y - anchor.height).max(0.);
        let above = (anchor.y - bounds.y).max(0.);
        let (y, h) = if below >= desired || below >= above && below >= 150. {
            (anchor.y + anchor.height, desired.min(below))
        } else if above >= 150. {
            let h = desired.min(above);
            (anchor.y - h, h)
        } else {
            (
                (anchor.y + anchor.height).clamp(bounds.y, bounds.y + bounds.height - desired),
                desired,
            )
        };
        let x = anchor.x.clamp(bounds.x, bounds.x + bounds.width - w);
        let area = Rect::new(x, y, w, h);
        let inset = 9_f32.min(w / 4.);
        let inner = (w - 2. * inset).max(0.);
        let button = (inner / 2. - 3.).clamp(0., 106.);
        let footer = (h - 34.).max(0.);
        let rect = |dx, dy, width, height| Rect::new(x + dx, y + dy, width, height);
        Self {
            area,
            rules: rect(inset, 1., inner / 2., 29.),
            values: rect(inset + inner / 2., 1., inner / 2., 29.),
            input: rect(inset, 38., inner, 27.),
            all: rect(inset, 70., inner, ROW),
            list: rect(inset, 70. + ROW, inner, (footer - 74. - ROW).max(0.)),
            clear: rect(inset, footer + 3., button, 25.),
            apply: rect(w - inset - button, footer + 3., button, 25.),
        }
    }
    fn page(&self) -> usize {
        ((self.list.height / ROW).floor() as usize).max(1)
    }
}

impl FilterPopup {
    pub(super) fn new(source: &SnapshotSource, field: String) -> Result<Self, ControlError> {
        let facet = source.filters.facets.get(&field);
        let mut chosen: HashSet<_> = facet
            .into_iter()
            .flatten()
            .map(projection::value_identity)
            .collect();
        let mut rows: Vec<_> = source
            .facet_counts(&field)?
            .into_iter()
            .map(|(value, count)| {
                let label = match &value {
                    CellValue::Null => "(NULL)".into(),
                    CellValue::Text(s) if s.is_empty() => "(пустая строка)".into(),
                    _ => source.group_key_label(&field, &value),
                };
                let checked = facet.is_none() || chosen.remove(&projection::value_identity(&value));
                Entry {
                    folded: label.to_lowercase(),
                    label,
                    value,
                    count,
                    checked,
                }
            })
            .collect();
        if source.choice_labels.contains_key(&field) {
            rows.sort_by(|a, b| a.folded.cmp(&b.folded));
        }
        let hidden_selection = facet
            .into_iter()
            .flatten()
            .filter(|v| chosen.contains(&projection::value_identity(v)))
            .cloned()
            .collect();
        let rule = source
            .filters
            .inputs
            .get(&field)
            .cloned()
            .unwrap_or_default();
        let rule_hint = match source.table.schema().field(&field).map(|f| f.kind) {
            Some(rust_desktop_ui_data::FieldType::Text) => "Текст: подстрока или =значение",
            Some(rust_desktop_ui_data::FieldType::Bool) => "Значение: true, false или NULL",
            _ => "Диапазон: от..до     Пустое значение: NULL",
        };
        Ok(Self {
            field,
            rule_hint,
            applied: facet.is_some() || !rule.is_empty(),
            rules: !rule.is_empty() && facet.is_none(),
            rule,
            hidden_selection,
            matches: (0..rows.len()).collect(),
            matched_count: rows.iter().map(|r| r.count).sum(),
            checked_matches: rows.iter().filter(|r| r.checked).count(),
            select_text: false,
            rows,
            search: String::new(),
            cursor: 0,
            first: 0,
            dirty: false,
            typing: false,
            error: None,
        })
    }

    /// An untouched draft preserves values hidden by other active filters.
    pub(super) fn selection(&self) -> Option<Option<Vec<CellValue>>> {
        self.dirty.then(|| {
            if self.rows.iter().all(|r| r.checked) {
                None
            } else {
                Some(
                    self.rows
                        .iter()
                        .filter(|r| r.checked)
                        .map(|r| r.value.clone())
                        .chain(self.hidden_selection.iter().cloned())
                        .collect(),
                )
            }
        })
    }
    fn refresh_search(&mut self) {
        let search = self.search.to_lowercase();
        self.matches = self
            .rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| r.folded.contains(&search).then_some(i))
            .collect();
        self.matched_count = self.matches.iter().map(|i| self.rows[*i].count).sum();
        self.checked_matches = self
            .matches
            .iter()
            .filter(|i| self.rows[**i].checked)
            .count();
        self.first = 0;
        self.cursor = 0;
    }
    pub(super) fn text(&mut self, input: &str) {
        let draft = if self.rules {
            &mut self.rule
        } else {
            &mut self.search
        };
        if input == " " && !self.typing && !self.rules {
            return;
        }
        if input.len() <= 4096 && (self.select_text || draft.len() + input.len() <= 4096) {
            if self.select_text {
                draft.clear();
                self.select_text = false;
            }
            draft.push_str(input);
            self.typing = true;
            self.error = None;
            if !self.rules {
                self.refresh_search();
            }
        }
    }
    fn toggle(&mut self) {
        if let Some(i) = self.matches.get(self.cursor) {
            self.rows[*i].checked = !self.rows[*i].checked;
            if self.rows[*i].checked {
                self.checked_matches += 1;
            } else {
                self.checked_matches -= 1;
            }
            self.dirty = true;
        }
    }
    fn move_cursor(&mut self, delta: isize, page: usize) {
        self.typing = false;
        self.select_text = false;
        self.cursor = self
            .cursor
            .saturating_add_signed(delta)
            .min(self.matches.len().saturating_sub(1));
        self.first = self
            .first
            .min(self.cursor)
            .max((self.cursor + 1).saturating_sub(page));
    }
    pub(super) fn input(&mut self, e: &InputEvent, bounds: Rect, anchor: Rect) -> Action {
        let l = Layout::new(bounds, anchor, self.rows.len());
        let hit = |r: Rect, p: &Point| r.contains(p.x, p.y) && l.area.contains(p.x, p.y);
        match e {
            InputEvent::WindowFocused(false) => return Action::Close,
            InputEvent::KeyDown { key, modifiers, .. } => match key {
                Key::Character(c) if modifiers.control && c.eq_ignore_ascii_case("a") => {
                    self.typing = true;
                    self.select_text = true;
                }
                Key::Escape => return Action::Close,
                Key::Enter => return Action::Apply,
                Key::Delete if self.applied => return Action::Clear,
                Key::Backspace => {
                    if self.rules {
                        if self.select_text {
                            self.rule.clear();
                        } else {
                            self.rule.pop();
                        }
                        self.error = None;
                    } else {
                        if self.select_text {
                            self.search.clear();
                        } else {
                            self.search.pop();
                        }
                        self.refresh_search();
                    }
                    self.select_text = false;
                }
                Key::Tab => {
                    self.rules = !self.rules;
                    self.select_text = false;
                    self.typing = self.rules;
                }
                Key::Space if !self.rules && !self.typing => self.toggle(),
                Key::ArrowUp if !self.rules => self.move_cursor(-1, l.page()),
                Key::ArrowDown if !self.rules => self.move_cursor(1, l.page()),
                Key::PageUp if !self.rules => self.move_cursor(-(l.page() as isize), l.page()),
                Key::PageDown if !self.rules => self.move_cursor(l.page() as isize, l.page()),
                _ => {}
            },
            InputEvent::Wheel { delta, .. } if !self.rules => {
                let lines = (delta.y.abs() / ROW).ceil() as usize;
                self.first = if delta.y > 0. {
                    self.first.saturating_sub(lines)
                } else {
                    self.first
                        .saturating_add(lines)
                        .min(self.matches.len().saturating_sub(l.page()))
                };
                self.cursor = self.cursor.clamp(self.first, self.first + l.page() - 1);
            }
            InputEvent::PointerDown {
                position: p,
                button,
                ..
            } => {
                if !hit(l.area, p) {
                    return Action::Close;
                }
                if *button != PointerButton::Primary {
                    return Action::Stay;
                }
                self.select_text = false;
                if hit(l.clear, p) && self.applied {
                    return Action::Clear;
                }
                if hit(l.apply, p) {
                    return Action::Apply;
                }
                if hit(l.rules, p) || hit(l.values, p) {
                    self.rules = hit(l.rules, p);
                    self.typing = self.rules;
                } else if hit(l.input, p) {
                    self.typing = true;
                } else if !self.rules && hit(l.all, p) && !self.matches.is_empty() {
                    let check = self.checked_matches < self.matches.len();
                    self.checked_matches = if check { self.matches.len() } else { 0 };
                    for i in &self.matches {
                        self.rows[*i].checked = check;
                    }
                    self.typing = false;
                    self.dirty = true;
                } else if !self.rules && hit(l.list, p) {
                    let index = self.first + ((p.y - l.list.y) / ROW) as usize;
                    if index < self.matches.len() {
                        self.cursor = index;
                        self.typing = false;
                        self.toggle();
                    }
                }
            }
            _ => {}
        }
        Action::Stay
    }
    pub(super) fn paint(&self, scene: &mut Scene, bounds: Rect, anchor: Rect, p: GridPalette) {
        let l = Layout::new(bounds, anchor, self.rows.len());
        let label = |s: &mut Scene, r: Rect, value: String, color: Color, weight| {
            if let Some(clip) = r.intersection(l.area) {
                let pad = 7_f32.min(r.width);
                let cell = Rect::new(
                    r.x + pad,
                    r.y + ((r.height - 17.) / 2.).max(0.),
                    (r.width - pad).max(0.),
                    r.height,
                );
                views::draw_text(s, cell, clip, value, color, weight, 13.);
            }
        };
        let border =
            |s: &mut Scene, r: Rect, radius, color| s.rounded_stroke(r, l.area, radius, 1., color);
        scene.rounded_fill(l.area, bounds, 6., p.header);
        border(scene, l.area, 6., p.line);
        label(
            scene,
            l.rules,
            "Правила".into(),
            p.text,
            if self.rules { 600 } else { 400 },
        );
        label(
            scene,
            l.values,
            "Значения".into(),
            p.text,
            if self.rules { 400 } else { 600 },
        );
        let tab = if self.rules { l.rules } else { l.values };
        if let Some(line) = Rect::new(
            tab.x + 6.,
            tab.y + tab.height - 2.,
            (tab.width - 12.).clamp(0., 64.),
            2.,
        )
        .intersection(l.area)
        {
            scene.fill(line, p.focus);
        }
        scene.rounded_fill(
            l.input,
            l.area,
            4.,
            if self.select_text {
                p.selection
            } else {
                p.background
            },
        );
        border(
            scene,
            l.input,
            4.,
            if self.typing { p.focus } else { p.line },
        );
        let draft = if self.rules { &self.rule } else { &self.search };
        label(
            scene,
            l.input,
            if draft.is_empty() {
                if self.rules {
                    "Введите условие…"
                } else {
                    "Поиск значений…"
                }
                .into()
            } else {
                draft.clone()
            },
            if draft.is_empty() { p.muted } else { p.text },
            400,
        );
        if self.rules {
            label(
                scene,
                l.all,
                self.error
                    .clone()
                    .unwrap_or_else(|| "=   ≠ (!=)   <   ≤ (<=)   >   ≥ (>=)".into()),
                if self.error.is_some() {
                    p.error
                } else {
                    p.muted
                },
                400,
            );
            label(
                scene,
                Rect::new(l.list.x, l.list.y, l.list.width, ROW),
                self.rule_hint.into(),
                p.muted,
                400,
            );
        } else {
            let surface = Rect::new(l.all.x, l.all.y, l.all.width, ROW + l.list.height);
            scene.rounded_fill(surface, l.area, 3., p.background);
            let row = |s: &mut Scene,
                       r: Rect,
                       name: &str,
                       count: usize,
                       check: Option<bool>,
                       cursor: bool| {
                let Some(clip) = r.intersection(surface).and_then(|r| r.intersection(l.area))
                else {
                    return;
                };
                if cursor {
                    s.fill(clip, p.selection);
                }
                let mark = Rect::new(r.x + 7., r.y + 5., 14., 14.);
                let checked = check != Some(false);
                if checked {
                    s.rounded_fill(mark, clip, 3., p.focus);
                }
                s.rounded_stroke(mark, clip, 3., 1., if checked { p.focus } else { p.line });
                if checked {
                    let points = if check == Some(true) {
                        [(3., 7.), (6., 10.), (11., 4.)]
                    } else {
                        [(3., 7.), (7., 7.), (11., 7.)]
                    };
                    let stroke = points
                        .windows(2)
                        .map(|w| StrokeSegment {
                            start: Point::new(mark.x + w[0].0, mark.y + w[0].1),
                            end: Point::new(mark.x + w[1].0, mark.y + w[1].1),
                        })
                        .collect::<Vec<_>>();
                    s.stroke_path(&stroke, clip, 1.5, p.background);
                }
                let caption = Rect::new(r.x + 26., r.y + 3., (r.width - 32.).max(0.), r.height);
                views::draw_text(
                    s,
                    caption,
                    clip,
                    format!("{name}  ({count})"),
                    p.text,
                    400,
                    13.,
                );
            };
            let all = if self.checked_matches == 0 {
                Some(false)
            } else if self.checked_matches == self.matches.len() {
                Some(true)
            } else {
                None
            };
            row(
                scene,
                l.all,
                if self.search.is_empty() {
                    "(Все)"
                } else {
                    "(Все найденные)"
                },
                self.matched_count,
                all,
                false,
            );
            for (slot, i) in self
                .matches
                .iter()
                .skip(self.first)
                .take(l.page() + 1)
                .enumerate()
            {
                let e = &self.rows[*i];
                let r = Rect::new(l.list.x, l.list.y + slot as f32 * ROW, l.list.width, ROW);
                row(
                    scene,
                    r,
                    &e.label,
                    e.count,
                    Some(e.checked),
                    self.cursor == self.first + slot && !self.typing,
                );
            }
            if self.matches.is_empty() {
                label(
                    scene,
                    Rect::new(l.list.x, l.list.y, l.list.width, ROW),
                    "Нет совпадений".into(),
                    p.muted,
                    400,
                );
            }
            if self.matches.len() > l.page() && l.list.height > 12. {
                let thumb = (l.list.height * l.page() as f32 / self.matches.len() as f32)
                    .max(10.)
                    .min(l.list.height);
                let y = l.list.y
                    + (l.list.height - thumb) * self.first as f32
                        / (self.matches.len() - l.page()) as f32;
                scene.rounded_fill(
                    Rect::new(l.list.x + l.list.width - 4., y, 3., thumb),
                    l.list,
                    1.5,
                    p.muted,
                );
            }
            border(scene, surface, 3., p.line);
        }
        for (r, title, enabled) in [
            (l.clear, "Сбросить", self.applied),
            (l.apply, "Применить", true),
        ] {
            scene.rounded_fill(r, l.area, 4., p.background);
            border(scene, r, 4., p.line);
            label(
                scene,
                r,
                title.into(),
                if enabled { p.text } else { p.muted },
                400,
            );
        }
    }
}
