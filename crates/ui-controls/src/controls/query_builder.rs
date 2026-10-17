//! Retained visual/text editing; applying emits a typed value and never executes it.
use super::query_builder_model::*;
use super::*;
use rust_desktop_ui_core::{GridPlacement, GridTrack};
use rust_desktop_ui_data::{
    ArithmeticOperator, CellValue, ComparisonOperator, Criteria, MAX_QUERY_TEXT_BYTES,
    QueryExpression, TableSchema, TextComparison, TextMatchOperator, query_text_completions,
};
use rust_desktop_ui_treegrid::{ColumnId, IndexedTreeSource, RowKey, TreeNode};
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Debug)]
pub struct QueryBuilderParts {
    pub container: WidgetId,
    pub tree: WidgetId,
    pub text: WidgetId,
    pub field: EditorControl,
    pub operator: EditorControl,
    pub operand: WidgetId,
    pub completion: EditorControl,
    pub status: WidgetId,
    pub apply: WidgetId,
    pub cancel: WidgetId,
}
#[derive(Clone, Copy)]
enum Action {
    Visual,
    Text,
    And,
    Or,
    Not,
    Remove,
    Condition,
    Complete,
    Apply,
    Cancel,
}
struct Entry {
    parts: QueryBuilderParts,
    schema: TableSchema,
    accepted: QueryBuilderValue,
    draft: QueryBuilderValue,
    mode: QueryBuilderMode,
    read_only: bool,
    text: String,
    error: Option<String>,
    source: Rc<RefCell<IndexedTreeSource>>,
    paths: Vec<Vec<usize>>,
    parents: Vec<(WidgetId, WidgetId)>,
    buttons: Vec<(WidgetId, Action)>,
    validator: Option<QueryBuilderValidator>,
    source_validators: Vec<QueryBuilderValidator>,
}
#[derive(Default)]
pub(super) struct QueryBuilders {
    entries: HashMap<WidgetId, Entry>,
    applied: Vec<QueryBuilderApplied>,
}
fn size(w: f32, h: f32) -> LayoutStyle {
    LayoutStyle {
        width: Length::Px(w),
        height: Length::Px(h),
        ..Default::default()
    }
}
fn cell(col: u16, row: u16) -> LayoutStyle {
    LayoutStyle {
        grid_column: GridPlacement {
            start: Some(col + 1),
            span: 1,
        },
        grid_row: GridPlacement {
            start: Some(row + 1),
            span: 1,
        },
        ..Default::default()
    }
}
fn tree_nodes(value: &QueryBuilderValue) -> Result<(Vec<TreeNode>, Vec<Vec<usize>>), ControlError> {
    let mut nodes = Vec::new();
    let mut paths = Vec::new();
    fn visit(
        v: &Criteria,
        parent: Option<RowKey>,
        path: Vec<usize>,
        nodes: &mut Vec<TreeNode>,
        paths: &mut Vec<Vec<usize>>,
    ) -> Result<(), ControlError> {
        let key = RowKey(nodes.len() as u64 + 1);
        let label = match v {
            Criteria::All(_) => "Все условия".into(),
            Criteria::Any(_) => "Любое условие".into(),
            Criteria::Not(_) => "НЕ".into(),
            _ => rust_desktop_ui_data::format_criteria_text(v).map_err(invalid)?,
        };
        nodes.push(TreeNode::new(key, parent, label));
        paths.push(path.clone());
        match v {
            Criteria::All(children) | Criteria::Any(children) => {
                for (i, c) in children.iter().enumerate() {
                    let mut p = path.clone();
                    p.push(i);
                    visit(c, Some(key), p, nodes, paths)?;
                }
            }
            Criteria::Not(c) => {
                let mut p = path;
                p.push(0);
                visit(c, Some(key), p, nodes, paths)?;
            }
            _ => {}
        }
        Ok(())
    }
    match value {
        QueryBuilderValue::Criteria(v) => visit(v, None, vec![], &mut nodes, &mut paths)?,
        QueryBuilderValue::Expression(v) => {
            nodes.push(TreeNode::new(
                RowKey(1),
                None,
                rust_desktop_ui_data::format_expression_text(v).map_err(invalid)?,
            ));
            paths.push(vec![]);
        }
    }
    Ok((nodes, paths))
}
impl Controls {
    pub fn query_builder(
        &mut self,
        parent: WidgetId,
        schema: TableSchema,
        value: QueryBuilderValue,
        settings: QueryBuilderSettings,
        mut style: LayoutStyle,
    ) -> Result<QueryBuilderParts, ControlError> {
        value.validate(&schema)?;
        if schema.fields().is_empty()
            || schema.fields().len() > 128
            || self.query_builders.entries.len() >= 16
        {
            return Err(invalid(
                "Builder requires 1..128 fields; at most 16 builders",
            ));
        }
        if settings.mode == QueryBuilderMode::Visual && !visual_supported(&value) {
            return Err(invalid(
                "This query requires text mode; visual recipe is unsupported",
            ));
        }
        let text = value.to_text()?;
        self.preflight_new_editor_text(&text, &style, false)?;
        let (nodes, paths) = tree_nodes(&value)?;
        for n in &nodes {
            self.measure_text(&n.cells[&ColumnId(0)], 4096.0, false)?;
        }
        let source = Rc::new(RefCell::new(
            IndexedTreeSource::new(nodes).map_err(invalid)?,
        ));
        style.kind = LayoutKind::Grid;
        style.grid_columns = vec![GridTrack::Fr(1.0)];
        style.grid_rows = vec![GridTrack::Auto, GridTrack::Fr(1.0), GridTrack::Auto];
        style.gap = 8.0;
        let owner = self.transparent_panel(parent, style)?;
        let result = (|| {
            let toolbar = self.transparent_panel(
                owner,
                LayoutStyle {
                    kind: LayoutKind::Wrap(Axis::Horizontal),
                    gap: 4.0,
                    ..cell(0, 0)
                },
            )?;
            let mut buttons = Vec::new();
            for (label, action, width) in [
                ("Дерево", Action::Visual, 88.0),
                ("Текст", Action::Text, 76.0),
                ("И", Action::And, 40.0),
                ("ИЛИ", Action::Or, 56.0),
                ("НЕ", Action::Not, 48.0),
                ("Удалить", Action::Remove, 92.0),
            ] {
                let id = self.button(toolbar, label, None, size(width, 36.0))?;
                buttons.push((id, action));
            }
            let body = self.transparent_panel(
                owner,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    grid_columns: vec![GridTrack::Fr(1.0), GridTrack::Fr(1.5)],
                    grid_rows: vec![GridTrack::Fr(1.0)],
                    gap: 8.0,
                    ..cell(0, 1)
                },
            )?;
            let tree = self.tree_view(
                body,
                TreeSourceHandle::from_shared(source.clone()),
                ColumnId(0),
                cell(0, 0),
            )?;
            let right = self.transparent_panel(
                body,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    grid_columns: vec![GridTrack::Fr(1.0)],
                    grid_rows: vec![
                        GridTrack::Px(36.0),
                        GridTrack::Px(36.0),
                        GridTrack::Px(36.0),
                        GridTrack::Px(36.0),
                        GridTrack::Fr(1.0),
                        GridTrack::Px(36.0),
                    ],
                    gap: 4.0,
                    ..cell(1, 0)
                },
            )?;
            let field = self.combo_box(
                right,
                "Поле",
                schema
                    .fields()
                    .iter()
                    .map(|f| SelectionItem::new(&f.name, &f.title))
                    .collect(),
                ComboBoxSettings::default(),
                cell(0, 0),
            )?;
            self.set_selected_keys(field.container, &[schema.fields()[0].name.clone()])?;
            let operators = if matches!(value, QueryBuilderValue::Criteria(_)) {
                vec![
                    "eq",
                    "ne",
                    "lt",
                    "le",
                    "gt",
                    "ge",
                    "is_null",
                    "contains",
                    "starts_with",
                    "ends_with",
                    "icontains",
                ]
            } else {
                vec![
                    "field", "literal", "add", "sub", "mul", "div", "lower", "length", "coalesce",
                ]
            };
            let operator = self.combo_box(
                right,
                "Оператор",
                operators
                    .iter()
                    .map(|s| SelectionItem::new(*s, *s))
                    .collect(),
                ComboBoxSettings::default(),
                cell(0, 1),
            )?;
            self.set_selected_keys(operator.container, &[operators[0].into()])?;
            let operand = self.text_box(right, "Типизированное значение", "", cell(0, 2))?;
            let replace = self.button(right, "Заменить выбранный узел", None, cell(0, 3))?;
            buttons.push((replace, Action::Condition));
            let editor = self.text_box(right, "Текст запроса", &text, cell(0, 4))?;
            for id in [editor, operand] {
                let mut settings = self.editor_settings(id).unwrap().clone();
                settings.max_length = Some(MAX_QUERY_TEXT_BYTES);
                settings.commit_on_focus_lost = false;
                self.set_editor_settings(id, settings)?;
            }
            let complete_row = self.transparent_panel(
                right,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    grid_columns: vec![GridTrack::Fr(1.0), GridTrack::Px(96.0)],
                    gap: 4.0,
                    ..cell(0, 5)
                },
            )?;
            let completion = self.combo_box(
                complete_row,
                "Дополнение",
                query_text_completions(&schema, "")
                    .into_iter()
                    .map(|s| SelectionItem::new(&s, &s))
                    .collect(),
                ComboBoxSettings::default(),
                cell(0, 0),
            )?;
            let complete = self.button(complete_row, "Вставить", None, cell(1, 0))?;
            buttons.push((complete, Action::Complete));
            let footer = self.transparent_panel(
                owner,
                LayoutStyle {
                    kind: LayoutKind::Grid,
                    grid_columns: vec![
                        GridTrack::Fr(1.0),
                        GridTrack::Px(112.0),
                        GridTrack::Px(100.0),
                    ],
                    gap: 8.0,
                    ..cell(0, 2)
                },
            )?;
            let status = self.label(
                footer,
                "Изменения не применены",
                LayoutStyle {
                    height: Length::Px(56.0),
                    min_size: Size::new(0.0, 36.0),
                    ..cell(0, 0)
                },
            )?;
            self.set_text_auto_height(status, true)?;
            let apply = self.button(
                footer,
                "Применить",
                None,
                LayoutStyle {
                    height: Length::Px(36.0),
                    ..cell(1, 0)
                },
            )?;
            buttons.push((apply, Action::Apply));
            let cancel = self.button(
                footer,
                "Отмена",
                None,
                LayoutStyle {
                    height: Length::Px(36.0),
                    ..cell(2, 0)
                },
            )?;
            buttons.push((cancel, Action::Cancel));
            let parts = QueryBuilderParts {
                container: owner,
                tree,
                text: editor,
                field,
                operator,
                operand,
                completion,
                status,
                apply,
                cancel,
            };
            let mut parents = Vec::new();
            let mut pending = self
                .runtime
                .tree()
                .children(owner)
                .unwrap_or_default()
                .to_vec();
            while let Some(id) = pending.pop() {
                parents.push((id, self.runtime.tree().parent(id).unwrap()));
                if id != tree {
                    pending.extend(self.runtime.tree().children(id).unwrap_or_default());
                }
            }
            self.query_builders.entries.insert(
                owner,
                Entry {
                    parts: parts.clone(),
                    schema,
                    accepted: value.clone(),
                    draft: value,
                    mode: settings.mode,
                    read_only: settings.read_only,
                    text,
                    error: None,
                    source,
                    paths,
                    parents,
                    buttons,
                    validator: None,
                    source_validators: Vec::new(),
                },
            );
            self.set_query_builder_read_only(owner, settings.read_only)?;
            self.set_tree_selected_key(tree, Some(RowKey(1)))?;
            self.apply_query_builder_mode(owner)?;
            Ok(parts)
        })();
        if result.is_err() {
            self.query_builders.entries.remove(&owner);
            self.remove(owner)?;
        }
        result
    }
    pub fn query_builder_parts(&self, id: WidgetId) -> Option<&QueryBuilderParts> {
        self.query_builders.entries.get(&id).map(|e| &e.parts)
    }
    pub fn query_builder_value(&self, id: WidgetId) -> Option<&QueryBuilderValue> {
        self.query_builders.entries.get(&id).map(|e| &e.accepted)
    }
    pub fn query_builder_draft(&self, id: WidgetId) -> Option<&QueryBuilderValue> {
        self.query_builders.entries.get(&id).map(|e| &e.draft)
    }
    pub fn query_builder_error(&self, id: WidgetId) -> Option<&str> {
        self.query_builders.entries.get(&id)?.error.as_deref()
    }
    pub fn query_builder_read_only(&self, id: WidgetId) -> Option<bool> {
        self.query_builders.entries.get(&id).map(|e| e.read_only)
    }
    pub fn query_builder_text(&self, id: WidgetId) -> Option<&str> {
        let e = self.query_builders.entries.get(&id)?;
        self.document(e.parts.text).map(|d| d.text())
    }
    pub fn take_query_builder_applied(&mut self) -> Vec<QueryBuilderApplied> {
        std::mem::take(&mut self.query_builders.applied)
    }
    pub fn set_query_builder_value_text(
        &mut self,
        id: WidgetId,
        text: &str,
        discard_draft: bool,
    ) -> Result<(), ControlError> {
        self.check_query_builder(id)?;
        let e = &self.query_builders.entries[&id];
        let value = e.accepted.parse_like(&e.schema, text)?;
        self.set_query_builder_value(id, value, discard_draft)
    }
    pub fn chain_query_builder_validator(
        &mut self,
        id: WidgetId,
        validator: QueryBuilderValidator,
    ) -> Result<(), ControlError> {
        self.check_query_builder(id)?;
        let e = self.query_builders.entries.get_mut(&id).unwrap();
        e.source_validators.push(validator);
        Ok(())
    }
    pub fn set_query_builder_validator(
        &mut self,
        id: WidgetId,
        validator: Option<QueryBuilderValidator>,
    ) -> Result<(), ControlError> {
        self.check_query_builder(id)?;
        self.query_builders.entries.get_mut(&id).unwrap().validator = validator;
        Ok(())
    }
    pub fn set_query_builder_read_only(
        &mut self,
        id: WidgetId,
        read_only: bool,
    ) -> Result<(), ControlError> {
        self.check_query_builder(id)?;
        let e = &self.query_builders.entries[&id];
        let parts = e.parts.clone();
        let buttons = e.buttons.clone();
        self.set_read_only(parts.text, read_only)?;
        self.set_read_only(parts.operand, read_only)?;
        for owner in [
            parts.field.container,
            parts.operator.container,
            parts.completion.container,
        ] {
            self.set_combo_settings(
                owner,
                ComboBoxSettings {
                    read_only,
                    ..Default::default()
                },
            )?;
        }
        for (button, action) in buttons {
            self.set_enabled(
                button,
                !read_only || matches!(action, Action::Visual | Action::Text | Action::Cancel),
            )?;
        }
        self.query_builders.entries.get_mut(&id).unwrap().read_only = read_only;
        Ok(())
    }
    fn check_query_builder(&self, id: WidgetId) -> Result<(), ControlError> {
        let e = self
            .query_builders
            .entries
            .get(&id)
            .ok_or_else(|| invalid("Unknown query builder"))?;
        for &(child, parent) in &e.parents {
            if self.runtime.tree().parent(child) != Some(parent) {
                return Err(invalid(
                    "Query builder required part was removed or reparented",
                ));
            }
        }
        for editor in [e.parts.text, e.parts.operand] {
            let c = self
                .controls
                .get(&editor)
                .ok_or_else(|| invalid("Query editor missing"))?;
            if !matches!(c.kind, Kind::TextBox(_))
                || c.single_line
                || c.editor.max_length != Some(MAX_QUERY_TEXT_BYTES)
                || c.editor.commit_on_focus_lost
                || self.editor_mask_settings(editor).is_some()
            {
                return Err(invalid("Query editor recipe was modified"));
            }
        }
        let tree = self
            .trees
            .hosts
            .get(&e.parts.tree)
            .ok_or_else(|| invalid("Query tree missing"))?;
        if tree.selection_validator.is_some()
            || !tree
                .source
                .same_source(&TreeSourceHandle::from_shared(e.source.clone()))
        {
            return Err(invalid("Query tree recipe was modified"));
        }
        Ok(())
    }
    fn query_builder_available(&self, id: WidgetId) -> bool {
        self.query_builders.entries.get(&id).is_some_and(|e| {
            !e.read_only
                && self.effective_enabled(id)
                && self
                    .runtime
                    .tree()
                    .node(id)
                    .is_some_and(|n| n.props.visible)
        })
    }
    fn builder_error(&mut self, id: WidgetId, message: Option<String>) -> Result<(), ControlError> {
        let text = message.as_deref().unwrap_or("Изменения не применены");
        let safe: String = text.chars().take(240).collect();
        let safe = if self.measure_text(&safe, 4096.0, false).is_ok() {
            safe
        } else {
            "Некорректное выражение".into()
        };
        self.set_label(self.query_builders.entries[&id].parts.status, &safe)?;
        self.query_builders.entries.get_mut(&id).unwrap().error = message.map(|_| safe);
        Ok(())
    }
    fn current_builder_value(&self, id: WidgetId) -> Result<QueryBuilderValue, ControlError> {
        let e = &self.query_builders.entries[&id];
        e.accepted.parse_like(
            &e.schema,
            self.document(e.parts.text)
                .ok_or_else(|| invalid("Query text missing"))?
                .text(),
        )
    }
    fn query_builder_editors(&self, id: WidgetId) -> [WidgetId; 5] {
        let p = &self.query_builders.entries[&id].parts;
        [
            p.text,
            p.operand,
            p.field.editor,
            p.operator.editor,
            p.completion.editor,
        ]
    }
    fn guard_builder_composition(&self, id: WidgetId) -> Result<(), ControlError> {
        if self.query_builder_editors(id).into_iter().any(|editor| {
            self.document(editor)
                .is_some_and(|d| d.composition().is_some())
        }) {
            return Err(invalid("Finish text composition before changing the query"));
        }
        Ok(())
    }
    fn install_builder_draft(
        &mut self,
        id: WidgetId,
        value: QueryBuilderValue,
    ) -> Result<(), ControlError> {
        self.install_builder_draft_inner(id, value, false)
    }
    fn install_builder_draft_inner(
        &mut self,
        id: WidgetId,
        value: QueryBuilderValue,
        discard_composition: bool,
    ) -> Result<(), ControlError> {
        self.check_query_builder(id)?;
        if !discard_composition {
            self.guard_builder_composition(id)?;
        }
        let e = &self.query_builders.entries[&id];
        value.validate(&e.schema)?;
        if std::mem::discriminant(&value) != std::mem::discriminant(&e.accepted) {
            return Err(invalid("Cannot change builder kind"));
        }
        let text = value.to_text()?;
        let editor = e.parts.text;
        let tree = e.parts.tree;
        let source = e.source.clone();
        let (nodes, paths) = tree_nodes(&value)?;
        for node in &nodes {
            self.measure_text(&node.cells[&ColumnId(0)], 4096.0, false)?;
        }
        let prepared = IndexedTreeSource::new(nodes.clone()).map_err(invalid)?;
        drop(prepared);
        self.preflight_editor_text(editor, &text)?;
        if discard_composition {
            for editor in self.query_builder_editors(id) {
                if let Some(Control {
                    kind: Kind::TextBox(document),
                    ..
                }) = self.controls.get_mut(&editor)
                {
                    document.cancel_preedit();
                }
            }
        }
        self.set_text(editor, &text)?;
        source.borrow_mut().replace(nodes).map_err(invalid)?;
        self.tree_refresh(tree)?;
        let e = self.query_builders.entries.get_mut(&id).unwrap();
        e.draft = value;
        e.text = text;
        e.paths = paths;
        self.builder_error(id, None)
    }
    pub fn set_query_builder_value(
        &mut self,
        id: WidgetId,
        value: QueryBuilderValue,
        discard_draft: bool,
    ) -> Result<(), ControlError> {
        self.check_query_builder(id)?;
        let e = &self.query_builders.entries[&id];
        if !discard_draft && self.query_builder_text(id) != Some(e.accepted.to_text()?.as_str()) {
            return Err(invalid("Query draft conflicts with external replacement"));
        }
        self.install_builder_draft_inner(id, value.clone(), discard_draft)?;
        self.query_builders.entries.get_mut(&id).unwrap().accepted = value;
        Ok(())
    }
    pub fn set_query_builder_draft(
        &mut self,
        id: WidgetId,
        value: QueryBuilderValue,
    ) -> Result<(), ControlError> {
        self.install_builder_draft(id, value)
    }
    pub fn set_query_builder_text(&mut self, id: WidgetId, text: &str) -> Result<(), ControlError> {
        self.check_query_builder(id)?;
        self.guard_builder_composition(id)?;
        let editor = self.query_builders.entries[&id].parts.text;
        if text.len() > MAX_QUERY_TEXT_BYTES {
            return Err(invalid("Query text exceeds 16 KiB"));
        }
        self.set_text(editor, text)?;
        self.sync_query_builder(id)
    }
    pub fn apply_query_builder(&mut self, id: WidgetId) -> Result<bool, ControlError> {
        self.check_query_builder(id)?;
        if !self.query_builder_available(id) {
            return Err(invalid("Query builder is read-only or unavailable"));
        }
        let e = &self.query_builders.entries[&id];
        self.guard_builder_composition(id)?;
        let value = self.current_builder_value(id)?;
        for validator in &e.source_validators {
            validator.check(&value)?;
        }
        if let Some(v) = &e.validator {
            v.check(&value)?;
        }
        if value == e.accepted {
            return Ok(false);
        }
        if self.query_builders.applied.len() >= 128 {
            return Err(invalid("Consume query Apply events before continuing"));
        }
        self.install_builder_draft(id, value.clone())?;
        self.query_builders.entries.get_mut(&id).unwrap().accepted = value.clone();
        self.query_builders
            .applied
            .push(QueryBuilderApplied { id, value });
        Ok(true)
    }
    pub fn cancel_query_builder(&mut self, id: WidgetId) -> Result<(), ControlError> {
        self.check_query_builder(id)?;
        let e = &self.query_builders.entries[&id];
        let value = e.accepted.clone();
        self.install_builder_draft_inner(id, value, true)
    }
    fn apply_query_builder_mode(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let e = &self.query_builders.entries[&id];
        let tree = e.parts.tree;
        let visual = e.mode == QueryBuilderMode::Visual;
        self.edit_props(tree, |p| p.visible = visual)?;
        Ok(())
    }
    pub fn set_query_builder_mode(
        &mut self,
        id: WidgetId,
        mode: QueryBuilderMode,
    ) -> Result<(), ControlError> {
        self.check_query_builder(id)?;
        let value = self.current_builder_value(id)?;
        if mode == QueryBuilderMode::Visual && !visual_supported(&value) {
            return Err(invalid(
                "This query requires text mode; visual recipe is unsupported",
            ));
        }
        self.install_builder_draft(id, value)?;
        self.query_builders.entries.get_mut(&id).unwrap().mode = mode;
        self.apply_query_builder_mode(id)
    }
    pub(super) fn sync_query_builders(&mut self) -> Result<(), ControlError> {
        let ids: Vec<_> = self.query_builders.entries.keys().copied().collect();
        for id in ids {
            self.sync_query_builder(id)?;
        }
        Ok(())
    }
    fn sync_query_builder(&mut self, id: WidgetId) -> Result<(), ControlError> {
        self.check_query_builder(id)?;
        let e = &self.query_builders.entries[&id];
        let text = self.document(e.parts.text).unwrap().text().to_owned();
        if text == e.text {
            return Ok(());
        }
        let result = e.accepted.parse_like(&e.schema, &text);
        match result {
            Ok(value) => {
                let (nodes, paths) = tree_nodes(&value)?;
                for node in &nodes {
                    self.measure_text(&node.cells[&ColumnId(0)], 4096.0, false)?;
                }
                let e = &self.query_builders.entries[&id];
                let tree = e.parts.tree;
                e.source.borrow_mut().replace(nodes).map_err(invalid)?;
                self.tree_refresh(tree)?;
                let e = self.query_builders.entries.get_mut(&id).unwrap();
                e.draft = value;
                e.paths = paths;
                e.text = text;
                self.builder_error(id, None)?;
            }
            Err(error) => {
                self.query_builders.entries.get_mut(&id).unwrap().text = text;
                self.builder_error(id, Some(error.to_string()))?;
            }
        }
        Ok(())
    }
    pub(super) fn prune_query_builders(&mut self) {
        self.query_builders
            .entries
            .retain(|id, _| self.runtime.tree().node(*id).is_some());
        self.query_builders
            .applied
            .retain(|e| self.runtime.tree().node(e.id).is_some());
    }
    pub(super) fn preflight_query_builder_removal(&self, id: WidgetId) -> Result<(), ControlError> {
        for (&owner, e) in &self.query_builders.entries {
            if e.parents.iter().any(|(part, _)| *part == id) && !self.flyout_descendant(owner, id) {
                return Err(invalid(
                    "Remove the whole query builder, not its required parts",
                ));
            }
        }
        Ok(())
    }
    pub(super) fn preflight_query_builder_text(
        &self,
        id: WidgetId,
        text: &str,
    ) -> Result<(), ControlError> {
        if self
            .query_builders
            .entries
            .values()
            .any(|e| e.parts.text == id || e.parts.operand == id)
            && text.len() > MAX_QUERY_TEXT_BYTES
        {
            return Err(invalid("Query text exceeds 16 KiB"));
        }
        Ok(())
    }
}
mod actions;
