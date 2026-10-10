use crate::{books::BookSource, ribbon_model};
use rust_desktop_ui_controls::{Controls, MenuItem, Ribbon, TabControl, Theme};
use rust_desktop_ui_core::{
    Axis, Color, Edges, InputEvent, Key, LayoutKind, LayoutStyle, Length, Modifiers, Mutation,
    Point, PointerButton, Rect, Scene, Size, WidgetId,
};
use rust_desktop_ui_platform_winit::{ClipboardRequest, DesktopApp, EventResponse, PlatformEvent};
use rust_desktop_ui_treegrid::{
    ColumnId, GridFrame, GridKey, GridPalette, RowKey, SelectionModifiers, TreeDataSource, TreeGrid,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, String>;
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
fn fixed(width: f32, height: f32) -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(width),
        height: Length::Px(height),
        flex_shrink: 0.0,
        ..Default::default()
    }
}
fn row(height: f32) -> LayoutStyle {
    LayoutStyle {
        kind: LayoutKind::Flex(Axis::Horizontal),
        height: Length::Px(height),
        flex_shrink: 0.0,
        gap: 8.0,
        ..Default::default()
    }
}
fn flexible(kind: LayoutKind) -> LayoutStyle {
    LayoutStyle {
        kind,
        height: Length::Px(0.0),
        flex_grow: 1.0,
        ..Default::default()
    }
}

pub struct Gallery {
    pub(crate) ui: Controls,
    pub(crate) grid: TreeGrid,
    pub(crate) frame: GridFrame,
    pub(crate) source: BookSource,
    pub(crate) grid_node: WidgetId,
    pub(crate) tabs: TabControl,
    ribbon: Ribbon,
    search: WidgetId,
    status: WidgetId,
    menu_button: WidgetId,
    editor: Option<WidgetId>,
    editor_key: Option<(RowKey, ColumnId)>,
    viewport: Size,
    grid_bounds: Rect,
    query_text: String,
    pub(crate) message: String,
    state_file: PathBuf,
    clipboard: Vec<ClipboardRequest>,
    last_blink: Instant,
    suppress_text: bool,
    pub(crate) accessible_widgets: HashMap<WidgetId, u64>,
    pub(crate) next_accessible_id: u64,
}

impl Gallery {
    pub fn new(rows: usize, state_file: PathBuf) -> Result<Self> {
        let mut ui = Controls::new(Theme::Light).map_err(err)?;
        let root = ui.root();
        let mut ribbon = Ribbon::new(ribbon_model::model(&mut ui)).map_err(err)?;
        ribbon.mount(&mut ui, root).map_err(err)?;
        let tabs = ui
            .tab_control(
                root,
                &["Библиотека", "Компоненты", "О проекте"],
                flexible(LayoutKind::Flex(Axis::Vertical)),
            )
            .map_err(err)?;
        for page in &tabs.pages {
            ui.runtime
                .apply(Mutation::SetStyle {
                    node: *page,
                    style: LayoutStyle {
                        kind: LayoutKind::Flex(Axis::Vertical),
                        gap: 8.0,
                        padding: Edges::all(4.0),
                        ..Default::default()
                    },
                })
                .map_err(err)?;
        }
        let toolbar = ui.panel(tabs.pages[0], row(34.0)).map_err(err)?;
        ui.label(toolbar, "Поиск", fixed(60.0, 34.0)).map_err(err)?;
        let search = ui
            .text_box(
                toolbar,
                "Поиск по названию или автору",
                "",
                LayoutStyle {
                    flex_grow: 1.0,
                    height: Length::Px(34.0),
                    ..Default::default()
                },
            )
            .map_err(err)?;
        ui.button(
            toolbar,
            "Сбросить",
            Some("clear".into()),
            fixed(104.0, 34.0),
        )
        .map_err(err)?;
        let grid_node = ui
            .panel(tabs.pages[0], flexible(LayoutKind::Overlay))
            .map_err(err)?;
        let mut props = ui
            .runtime
            .tree()
            .node(grid_node)
            .expect("grid node exists")
            .props
            .clone();
        props.focusable = true;
        props.clip = true;
        ui.runtime
            .apply(Mutation::SetProps {
                node: grid_node,
                props,
            })
            .map_err(err)?;
        ui.set_label(grid_node, "Библиотека книг").map_err(err)?;
        let status = ui
            .label(
                tabs.pages[0],
                "",
                LayoutStyle {
                    height: Length::Px(26.0),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )
            .map_err(err)?;
        let menu_button = Self::controls_page(&mut ui, tabs.pages[1])?;
        Self::about_page(&mut ui, tabs.pages[2])?;
        let mut source = BookSource::new(rows).map_err(err)?;
        let mut grid = TreeGrid::new(BookSource::columns()).map_err(err)?;
        grid.refresh(&mut source).map_err(err)?;
        let mut message =
            "F2: изменить · стрелки: навигация · Shift+колесо: колонки · Alt: команды".to_owned();
        match std::fs::read_to_string(&state_file) {
            Ok(saved) => {
                if let Err(error) = grid.restore_columns(&saved) {
                    message = format!("Не удалось загрузить колонки: {error}");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => message = format!("Не удалось прочитать колонки: {error}"),
        }
        Ok(Self {
            ui,
            grid,
            frame: GridFrame::default(),
            source,
            grid_node,
            tabs,
            ribbon,
            search,
            status,
            menu_button,
            editor: None,
            editor_key: None,
            viewport: Size::ZERO,
            grid_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
            query_text: String::new(),
            message,
            state_file,
            clipboard: Vec::new(),
            last_blink: Instant::now(),
            suppress_text: false,
            accessible_widgets: HashMap::new(),
            next_accessible_id: 100,
        })
    }

    fn controls_page(ui: &mut Controls, parent: WidgetId) -> Result<WidgetId> {
        let scroll = ui
            .scroll_view(parent, flexible(LayoutKind::Overlay), 860.0)
            .map_err(err)?;
        let parent = scroll.content;
        ui.label(
            parent,
            "Базовые компоненты · Unicode · ввод и фокус",
            LayoutStyle {
                height: Length::Px(36.0),
                ..Default::default()
            },
        )
        .map_err(err)?;
        let buttons = ui.panel(parent, row(36.0)).map_err(err)?;
        ui.button(
            buttons,
            "Сохранить",
            Some("save".into()),
            fixed(125.0, 34.0),
        )
        .map_err(err)?;
        ui.toggle(
            buttons,
            "Избранное",
            Some("favorite".into()),
            false,
            fixed(155.0, 34.0),
        )
        .map_err(err)?;
        let menu = ui
            .button(buttons, "Меню v", None, fixed(110.0, 34.0))
            .map_err(err)?;
        let disabled = ui
            .button(
                buttons,
                "Недоступно",
                Some("disabled".into()),
                fixed(135.0, 34.0),
            )
            .map_err(err)?;
        ui.set_enabled(disabled, false).map_err(err)?;
        ui.checkbox(
            parent,
            "Показывать сведения о книге",
            true,
            fixed(340.0, 34.0),
        )
        .map_err(err)?;
        ui.text_box(
            parent,
            "Редактируемый текст",
            "Русский текст — café — Ελληνικά",
            LayoutStyle {
                height: Length::Px(38.0),
                ..Default::default()
            },
        )
        .map_err(err)?;
        ui.text_box(
            parent,
            "Смешанное направление",
            "English العربية עברית 123",
            LayoutStyle {
                height: Length::Px(38.0),
                ..Default::default()
            },
        )
        .map_err(err)?;
        ui.label(
            parent,
            "Выделение: Shift+стрелки / мышь. Ctrl/Cmd+A, C, X, V, Z; Ctrl+Y / Cmd+Shift+Z.",
            LayoutStyle {
                height: Length::Px(32.0),
                ..Default::default()
            },
        )
        .map_err(err)?;
        for text in [
            "Кириллица: Съешь ещё этих мягких французских булок.",
            "Latin: The quick brown fox jumps over the lazy dog.",
            "Fallback: 日本語 中文 한글 · 😀 🌍",
            "RTL: العربية · עברית",
            "Комбинируемые символы: é ä · emoji: 👩‍💻",
        ] {
            ui.label(
                parent,
                text,
                LayoutStyle {
                    height: Length::Px(40.0),
                    ..Default::default()
                },
            )
            .map_err(err)?;
        }
        for (id, title) in [
            ("light", "Светлая тема"),
            ("dark", "Тёмная тема"),
            ("compact", "Компактная тема"),
        ] {
            ui.button(parent, title, Some(id.into()), fixed(240.0, 34.0))
                .map_err(err)?;
        }
        ui.label(parent, "Конец прокручиваемой области", fixed(400.0, 40.0))
            .map_err(err)?;
        Ok(menu)
    }

    fn about_page(ui: &mut Controls, parent: WidgetId) -> Result<()> {
        for text in [
            "Rust UI Engine · 0.1.0 alpha candidate",
            "Нативный retained UI: winit · wgpu · Parley · AccessKit",
            "Ribbon: Alt или F10 → KeyTip вкладки → KeyTip команды.",
            "TreeGrid: F2 / Enter — редактор; Enter — принять; Escape — отменить.",
            "Заголовок: сортировка. Перетаскивание: порядок. Правая граница: ширина.",
            "Первый столбец закреплён. Shift+колесо — горизонтальная прокрутка.",
            "Поиск, сортировка и загрузка дочерних записей выполняются в источнике данных.",
            "Пример работает локально. Лицензия исходного кода: Apache-2.0.",
        ] {
            ui.label(
                parent,
                text,
                LayoutStyle {
                    height: Length::Px(36.0),
                    flex_shrink: 0.0,
                    ..Default::default()
                },
            )
            .map_err(err)?;
        }
        Ok(())
    }

    pub(crate) fn library_visible(&self) -> bool {
        self.ui.selected_tab(self.tabs.container) == Some(0)
    }
    pub(crate) fn grid_focused(&self) -> bool {
        self.library_visible() && self.ui.runtime.focused() == Some(self.grid_node)
    }

    fn save_columns(&mut self) {
        let result = (|| -> std::io::Result<()> {
            if let Some(parent) = self.state_file.parent()
                && !parent.as_os_str().is_empty()
            {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&self.state_file, self.grid.save_columns())
        })();
        self.message = match result {
            Ok(()) => "Расположение колонок сохранено".into(),
            Err(error) => format!("Не удалось сохранить колонки: {error}"),
        };
    }

    fn sync_editor(&mut self) -> Result<()> {
        let key = self.grid.editor().map(|e| (e.row, e.column));
        if self.editor_key != key {
            if let Some(id) = self.editor.take() {
                self.ui.remove(id).map_err(err)?;
            }
            self.editor_key = key;
            if let Some(edit) = self.grid.editor() {
                let id = self
                    .ui
                    .text_box(
                        self.grid_node,
                        "Значение ячейки",
                        edit.value.clone(),
                        fixed(180.0, 28.0),
                    )
                    .map_err(err)?;
                self.ui
                    .runtime
                    .apply(Mutation::Focus(Some(id)))
                    .map_err(err)?;
                self.editor = Some(id);
            }
        }
        if let (Some(id), Some((row, column))) = (self.editor, key) {
            let cell = self.frame.cell_bounds(row, column);
            let mut props = self
                .ui
                .runtime
                .tree()
                .node(id)
                .ok_or("Cell editor no longer exists")?
                .props
                .clone();
            if props.visible != cell.is_some() {
                // Resizing can temporarily clip the edited row/column out of
                // the frame. Keep its document and validation state; hiding
                // clears native focus/IME exposure without discarding input.
                props.visible = cell.is_some();
                self.ui
                    .runtime
                    .apply(Mutation::SetProps { node: id, props })
                    .map_err(err)?;
            }
            if let Some(bounds) = cell {
                self.ui
                    .runtime
                    .apply(Mutation::SetStyle {
                        node: id,
                        style: LayoutStyle {
                            offset: Point::new(
                                bounds.x - self.grid_bounds.x,
                                bounds.y - self.grid_bounds.y,
                            ),
                            ..fixed(bounds.width, bounds.height)
                        },
                    })
                    .map_err(err)?;
            }
        }
        Ok(())
    }

    fn commit_editor(&mut self) -> Result<bool> {
        let Some(id) = self.editor else {
            return Ok(true);
        };
        if let Some(document) = self.ui.document(id) {
            self.grid.set_edit_text(document.text()).map_err(err)?;
        }
        match self.grid.commit_edit(&mut self.source) {
            Ok(()) => {
                self.message = "Значение сохранено".into();
                self.sync_editor()?;
                self.ui
                    .runtime
                    .apply(Mutation::Focus(Some(self.grid_node)))
                    .map_err(err)?;
                self.search_changed()?;
                Ok(true)
            }
            Err(error) => {
                self.message = error.to_string();
                Ok(false)
            }
        }
    }

    /// A query completion replaces the view and clears its editor. Keep that
    /// transition outside editing: wait for pending queries before opening a
    /// cell, and defer new search requests until the current edit finishes.
    pub(crate) fn cell_edit_ready(&mut self) -> Result<bool> {
        if !self.editor_finished()? {
            return Ok(false);
        }
        if self.source.query_pending() {
            self.message = "Дождитесь завершения поиска/сортировки".into();
            Ok(false)
        } else {
            Ok(true)
        }
    }

    /// Keep the existing draft until explicit commit/cancel. Keyboard,
    /// Ribbon and accessibility must not scroll, collapse or replace it.
    pub(crate) fn editor_finished(&mut self) -> Result<bool> {
        if self.grid.editor().is_none() {
            return Ok(true);
        }
        self.message = "Завершите или отмените текущее редактирование".into();
        if let Some(editor) = self.editor {
            self.ui.select_tab(self.tabs.container, 0).map_err(err)?;
            if self
                .ui
                .runtime
                .tree()
                .node(editor)
                .is_some_and(|node| node.props.visible)
                && self.ui.runtime.bounds(editor).is_some()
            {
                self.ui
                    .runtime
                    .apply(Mutation::Focus(Some(editor)))
                    .map_err(err)?;
            } else {
                self.message =
                    "Верните редактируемую ячейку в видимую область и завершите или отмените ввод"
                        .into();
            }
        }
        Ok(false)
    }

    fn input(&mut self, event: InputEvent) -> Result<()> {
        self.suppress_text = self.ribbon.key_tips_visible();
        if self
            .ribbon
            .handle_input(&mut self.ui, &event)
            .map_err(err)?
        {
            self.suppress_text = true;
            self.commands()?;
            return Ok(());
        }
        if let InputEvent::KeyDown { key, .. } = &event
            && self.editor.is_some()
            && self.ui.runtime.focused() == self.editor
            && self
                .editor
                .and_then(|id| self.ui.document(id))
                .is_none_or(|document| document.composition().is_none())
        {
            if *key == Key::Enter {
                self.commit_editor()?;
                return Ok(());
            }
            if *key == Key::Escape {
                self.grid.cancel_edit();
                self.sync_editor()?;
                self.ui
                    .runtime
                    .apply(Mutation::Focus(Some(self.grid_node)))
                    .map_err(err)?;
                self.search_changed()?;
                return Ok(());
            }
        }
        let popup = self.ui.popup_open();
        if let InputEvent::PointerDown { position, .. } = &event {
            let in_editor = self
                .editor
                .and_then(|id| self.ui.runtime.bounds(id))
                .is_some_and(|b| b.contains(position.x, position.y));
            if self.editor.is_some() && !in_editor && !popup && !self.commit_editor()? {
                return Ok(());
            }
        }
        let mut consumed = false;
        if !popup && self.library_visible() {
            match &event {
                InputEvent::PointerDown {
                    position,
                    button: PointerButton::Primary,
                    modifiers,
                } if self.grid_bounds.contains(position.x, position.y)
                    && !self.editor.is_some_and(|id| {
                        self.ui
                            .runtime
                            .bounds(id)
                            .is_some_and(|b| b.contains(position.x, position.y))
                    }) =>
                {
                    self.ui
                        .runtime
                        .apply(Mutation::Focus(Some(self.grid_node)))
                        .map_err(err)?;
                    self.grid
                        .pointer_down(
                            &mut self.source,
                            &self.frame,
                            *position,
                            selection(*modifiers),
                        )
                        .map_err(err)?;
                    consumed = true;
                }
                InputEvent::PointerMoved { position } if self.grid.pointer_captured() => {
                    self.grid.pointer_move(*position).map_err(err)?;
                    consumed = true;
                }
                InputEvent::PointerUp {
                    position,
                    button: PointerButton::Primary,
                    ..
                } if self.grid.pointer_captured() => {
                    self.grid
                        .pointer_up(&mut self.source, &self.frame, *position)
                        .map_err(err)?;
                    self.save_columns();
                    consumed = true;
                }
                InputEvent::Wheel {
                    position,
                    delta,
                    modifiers,
                } if self.grid_bounds.contains(position.x, position.y) => {
                    if self.editor.is_none() || self.commit_editor()? {
                        let (x, y) = if modifiers.shift {
                            (delta.y + delta.x, 0.0)
                        } else {
                            (delta.x, delta.y)
                        };
                        self.grid
                            .scroll_by(f64::from(x), f64::from(y))
                            .map_err(err)?;
                    }
                    consumed = true;
                }
                InputEvent::KeyDown { key, modifiers, .. } if self.grid_focused() => {
                    if ((modifiers.control && !modifiers.alt) || modifiers.super_key)
                        && *key == Key::Character("a".into())
                    {
                        self.grid.select_all(&self.source).map_err(err)?;
                        consumed = true;
                    } else if ((modifiers.control && !modifiers.alt) || modifiers.super_key)
                        && *key == Key::Character("c".into())
                    {
                        self.copy_cell();
                        consumed = true;
                    } else if let Some(key) = grid_key(key) {
                        let ready = if key == GridKey::Escape {
                            true
                        } else if matches!(key, GridKey::F2 | GridKey::Enter) {
                            self.cell_edit_ready()?
                        } else {
                            self.editor_finished()?
                        };
                        if ready
                            && let Err(error) =
                                self.grid
                                    .navigate(&mut self.source, key, selection(*modifiers))
                        {
                            self.message = error.to_string();
                        }
                        consumed = true;
                    }
                }
                InputEvent::WindowFocused(false) => self.grid.cancel_pointer(),
                _ => {}
            }
        }
        if !consumed {
            self.ui.dispatch(event).map_err(err)?;
        }
        self.process_activations()?;
        self.search_changed()?;
        Ok(())
    }

    /// Native input and accessibility actions must drain the same activation
    /// queue, including view-only Ribbon tabs and the demonstration menu.
    pub(crate) fn process_activations(&mut self) -> Result<()> {
        let activated = self.ribbon.process(&mut self.ui).map_err(err)?;
        if activated.contains(&self.menu_button) {
            self.ui
                .popup_menu(
                    self.menu_button,
                    &[
                        MenuItem::new("Сохранить", "save"),
                        MenuItem::new("Избранное", "favorite").toggling(),
                        MenuItem::new("Недоступно", "disabled"),
                    ],
                )
                .map_err(err)?;
        }
        self.commands()?;
        Ok(())
    }

    pub(crate) fn search_changed(&mut self) -> Result<()> {
        let text = self
            .ui
            .document(self.search)
            .map_or("", |d| d.text())
            .to_owned();
        if text != self.query_text {
            if self.grid.editor().is_some() {
                self.message = "Завершите или отмените редактирование перед поиском".into();
                return Ok(());
            }
            self.query_text = text.clone();
            self.source.set_search(&mut self.grid, text).map_err(err)?;
        }
        Ok(())
    }

    fn copy_cell(&mut self) {
        if let Some(row) = self.grid.focused() {
            let column = self.grid.focused_column().unwrap_or(ColumnId(0));
            self.clipboard
                .push(ClipboardRequest::Copy(self.source.cell(row, column)));
            self.message = "Значение ячейки скопировано".into();
        }
    }

    pub(crate) fn commands(&mut self) -> Result<()> {
        for invocation in self.ui.commands_mut().drain_invocations() {
            if matches!(
                invocation.id.0.as_str(),
                "restore" | "pin" | "first" | "last" | "expand" | "collapse"
            ) && !self.editor_finished()?
            {
                continue;
            }
            match invocation.id.0.as_str() {
                "light" => self.theme(Theme::Light)?,
                "dark" => self.theme(Theme::Dark)?,
                "compact" => self.theme(Theme::Compact)?,
                "library" | "open" => self.ui.select_tab(self.tabs.container, 0).map_err(err)?,
                "controls" => self.ui.select_tab(self.tabs.container, 1).map_err(err)?,
                "about" => self.ui.select_tab(self.tabs.container, 2).map_err(err)?,
                "clear" => {
                    self.ui.set_text(self.search, "").map_err(err)?;
                    self.search_changed()?;
                }
                "save" => self.save_columns(),
                "restore" => {
                    self.grid
                        .restore_columns(
                            &TreeGrid::new(BookSource::columns())
                                .map_err(err)?
                                .save_columns(),
                        )
                        .map_err(err)?;
                    self.save_columns();
                }
                "qat_add" => {
                    self.ribbon.add_quick_access("edit".into());
                }
                "qat_remove" => {
                    self.ribbon.remove_quick_access(&"edit".into());
                }
                "pin" => {
                    let column = self.grid.focused_column().unwrap_or(ColumnId(1));
                    let pinned = self
                        .grid
                        .columns()
                        .iter()
                        .find(|c| c.id == column)
                        .is_some_and(|c| c.pinned);
                    self.grid.pin_column(column, !pinned).map_err(err)?;
                    self.save_columns();
                }
                "first" => self
                    .grid
                    .navigate(
                        &mut self.source,
                        GridKey::Home,
                        SelectionModifiers::default(),
                    )
                    .map_err(err)?,
                "last" => self
                    .grid
                    .navigate(
                        &mut self.source,
                        GridKey::End,
                        SelectionModifiers::default(),
                    )
                    .map_err(err)?,
                "expand" | "collapse" => {
                    if let Some(row) = self.grid.focused() {
                        self.grid
                            .set_expanded(&mut self.source, row, invocation.id.0 == "expand")
                            .map_err(err)?;
                    }
                }
                "edit" => {
                    if self.cell_edit_ready()?
                        && let Err(error) = self.grid.navigate(
                            &mut self.source,
                            GridKey::F2,
                            SelectionModifiers::default(),
                        )
                    {
                        self.message = error.to_string();
                    }
                }
                "copy" => self.copy_cell(),
                "favorite" => {
                    self.message = format!(
                        "Избранное: {}",
                        if invocation.checked.unwrap_or(false) {
                            "да"
                        } else {
                            "нет"
                        }
                    )
                }
                "menu" => {
                    self.message = "Используйте стрелку рядом с командой для открытия меню".into()
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn theme(&mut self, theme: Theme) -> Result<()> {
        self.ui.set_theme(theme).map_err(err)?;
        let t = theme.tokens();
        self.grid.palette = GridPalette {
            background: t.surface,
            alternate_row: t.canvas,
            header: t.hover,
            text: t.text,
            muted: t.muted,
            line: t.border,
            selection: t.selected,
            selection_text: t.text,
            focus: t.focus,
            error: Color::rgb(206, 55, 65),
        };
        Ok(())
    }
}

impl DesktopApp for Gallery {
    fn update(&mut self, viewport: Size, _scale: f32) -> Result<()> {
        self.viewport = viewport;
        self.ribbon
            .set_context((!self.grid.selected().is_empty()).then_some("book"));
        self.ribbon
            .update(&mut self.ui, (viewport.width - 16.0).max(0.0))
            .map_err(err)?;
        self.ui.update(viewport).map_err(err)?;
        if self.library_visible()
            && let Some(bounds) = self.ui.runtime.bounds(self.grid_node)
        {
            self.grid_bounds = bounds;
            self.frame = self.grid.paint(&self.source, bounds).map_err(err)?;
            self.ui
                .set_custom_scene(self.grid_node, self.frame.scene.clone())
                .map_err(err)?;
            self.sync_editor()?;
            let s = self.frame.stats;
            self.ui
                .set_label(
                    self.status,
                    format!(
                        "{} строк · видно {} × {} · выбрано {} | {}",
                        s.logical_rows,
                        s.materialized_rows,
                        s.visible_columns,
                        self.grid.selected().len(),
                        self.message
                    ),
                )
                .map_err(err)?;
            self.ui.update(viewport).map_err(err)?;
        }
        Ok(())
    }
    fn scene(&self) -> &Scene {
        self.ui.scene()
    }
    fn title(&self) -> String {
        let page = ["Библиотека", "Компоненты", "О проекте"][self
            .ui
            .selected_tab(self.tabs.container)
            .unwrap_or(0)
            .min(2)];
        let theme = match self.ui.theme() {
            Theme::Light => "Светлая",
            Theme::Dark => "Тёмная",
            Theme::Compact => "Компактная",
            Theme::FluentLight => "Fluent Light",
            Theme::FluentDark => "Fluent Dark",
        };
        format!(
            "Rust UI Engine · {page} · {theme} · {} / {} строк{}",
            self.grid.row_count(),
            self.source.root_rows(),
            if self.source.query_pending() {
                " · поиск…"
            } else {
                ""
            }
        )
    }
    fn ime_cursor(&self) -> Option<Rect> {
        self.ui.caret_bounds()
    }
    fn ime_target(&self) -> Option<WidgetId> {
        self.ui
            .runtime
            .focused()
            .filter(|id| self.ui.document(*id).is_some())
    }
    fn ime_composition_active(&self) -> Option<bool> {
        self.ime_target()
            .and_then(|id| self.ui.document(id))
            .map(|document| document.composition().is_some())
    }
    fn wake_after(&self) -> Option<Duration> {
        if self.source.has_pending() {
            Some(Duration::from_millis(16))
        } else if self.ui.caret_bounds().is_some() {
            Some(Duration::from_millis(500))
        } else {
            None
        }
    }
    fn event(&mut self, event: PlatformEvent) -> Result<EventResponse> {
        let mut redraw = true;
        match event {
            PlatformEvent::Input(input) => self.input(input)?,
            PlatformEvent::Text(text) => {
                if !self.suppress_text {
                    self.ui.text_input(&text).map_err(err)?;
                    self.search_changed()?;
                }
            }
            PlatformEvent::Paste(text) | PlatformEvent::ImeCommit(text) => {
                self.ui.ime_commit(&text).map_err(err)?;
                self.search_changed()?;
            }
            PlatformEvent::ImePreedit(text, selection) => {
                self.ui.ime_preedit(&text, selection).map_err(err)?
            }
            PlatformEvent::ImeCancel => self.ui.cancel_preedit(),
            PlatformEvent::Tick => {
                redraw = self.source.poll(&mut self.grid).map_err(err)?;
                if self.last_blink.elapsed() >= Duration::from_millis(500) {
                    redraw |= self.ui.blink_caret();
                    self.last_blink = Instant::now();
                }
            }
            PlatformEvent::Accessibility(request) => self.accessibility_action(request)?,
        }
        for request in self.ui.drain_clipboard_requests() {
            self.clipboard.push(match request {
                rust_desktop_ui_controls::ClipboardRequest::Copy(text) => {
                    ClipboardRequest::Copy(text)
                }
                rust_desktop_ui_controls::ClipboardRequest::Paste => ClipboardRequest::Paste,
            });
        }
        Ok(EventResponse {
            redraw,
            clipboard: std::mem::take(&mut self.clipboard),
        })
    }
    fn accessibility(
        &mut self,
        scale: f32,
    ) -> rust_desktop_ui_platform_winit::accesskit::TreeUpdate {
        self.accessibility_tree(scale)
    }
}

fn selection(m: Modifiers) -> SelectionModifiers {
    SelectionModifiers {
        shift: m.shift,
        control: m.control || m.super_key,
    }
}
fn grid_key(key: &Key) -> Option<GridKey> {
    Some(match key {
        Key::ArrowUp => GridKey::Up,
        Key::ArrowDown => GridKey::Down,
        Key::ArrowLeft => GridKey::Left,
        Key::ArrowRight => GridKey::Right,
        Key::PageUp => GridKey::PageUp,
        Key::PageDown => GridKey::PageDown,
        Key::Home => GridKey::Home,
        Key::End => GridKey::End,
        Key::F2 => GridKey::F2,
        Key::Enter => GridKey::Enter,
        Key::Escape => GridKey::Escape,
        _ => return None,
    })
}
