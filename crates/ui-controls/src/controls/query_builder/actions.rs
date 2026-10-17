use super::*;
impl Controls {
    pub(in crate::controls) fn query_builder_before_input(
        &mut self,
        event: &InputEvent,
    ) -> Result<bool, ControlError> {
        let InputEvent::KeyDown {
            key,
            modifiers,
            repeat,
        } = event
        else {
            return Ok(false);
        };
        if self.popup.is_some() || !matches!(key, Key::Escape | Key::Enter) {
            return Ok(false);
        }
        let Some(focus) = self.runtime.focused() else {
            return Ok(false);
        };
        let Some(owner) = self
            .query_builders
            .entries
            .keys()
            .copied()
            .find(|owner| self.flyout_descendant(focus, *owner))
        else {
            return Ok(false);
        };
        if *key == Key::Enter && !modifiers.control {
            return Ok(false);
        }
        if self
            .document(focus)
            .is_some_and(|d| d.composition().is_some())
        {
            if *key == Key::Escape {
                self.cancel_preedit();
                return Ok(true);
            }
            return Ok(false);
        }
        let report = self.runtime.dispatch(event.clone());
        if report.default_prevented || *repeat {
            return Ok(true);
        }
        let result = if *key == Key::Escape {
            self.cancel_query_builder(owner)
        } else {
            self.apply_query_builder(owner).map(|_| ())
        };
        if let Err(error) = result {
            self.builder_error(owner, Some(error.to_string()))?;
        }
        Ok(true)
    }
    pub(in crate::controls) fn activate_query_builder(
        &mut self,
        id: WidgetId,
    ) -> Result<bool, ControlError> {
        let Some((owner, action)) = self.query_builders.entries.iter().find_map(|(&owner, e)| {
            e.buttons
                .iter()
                .find(|(button, _)| *button == id)
                .map(|(_, action)| (owner, *action))
        }) else {
            return Ok(false);
        };
        self.check_query_builder(owner)?;
        if !self.effective_enabled(owner) {
            return Ok(true);
        }
        let result = match action {
            Action::Visual => self.set_query_builder_mode(owner, QueryBuilderMode::Visual),
            Action::Text => self.set_query_builder_mode(owner, QueryBuilderMode::Text),
            Action::Cancel => self.cancel_query_builder(owner),
            _ if !self.query_builder_available(owner) => Ok(()),
            Action::Apply => self.apply_query_builder(owner).map(|_| ()),
            Action::Complete => self.complete_builder_text(owner),
            Action::Condition => self.builder_condition(owner),
            Action::And | Action::Or | Action::Not | Action::Remove => {
                self.builder_group_action(owner, action)
            }
        };
        if let Err(error) = result {
            self.builder_error(owner, Some(error.to_string()))?;
        }
        Ok(true)
    }
    fn builder_path(&self, id: WidgetId) -> Vec<usize> {
        let e = &self.query_builders.entries[&id];
        self.tree_selected_keys(e.parts.tree)
            .and_then(|keys| keys.first().map(|k| k.0 as usize))
            .and_then(|key| e.paths.get(key.saturating_sub(1)).cloned())
            .unwrap_or_default()
    }
    fn builder_group_action(&mut self, id: WidgetId, action: Action) -> Result<(), ControlError> {
        let QueryBuilderValue::Criteria(mut value) = self.current_builder_value(id)? else {
            return Err(invalid("Condition groups require a FilterBuilder"));
        };
        let path = self.builder_path(id);
        let selected = criteria_at(&value, &path)
            .ok_or_else(|| invalid("Selected criterion no longer exists"))?
            .clone();
        match action {
            Action::And => {
                replace_criteria(
                    &mut value,
                    &path,
                    Criteria::All(vec![selected, Criteria::Constant(true)]),
                );
            }
            Action::Or => {
                replace_criteria(
                    &mut value,
                    &path,
                    Criteria::Any(vec![selected, Criteria::Constant(false)]),
                );
            }
            Action::Not => {
                replace_criteria(&mut value, &path, Criteria::Not(Box::new(selected)));
            }
            Action::Remove => {
                if path.is_empty() {
                    value = Criteria::Constant(true);
                } else {
                    let mut parent = criteria_at(&value, &path[..path.len() - 1])
                        .unwrap()
                        .clone();
                    match &mut parent {
                        Criteria::All(v) | Criteria::Any(v) => {
                            v.remove(*path.last().unwrap());
                        }
                        Criteria::Not(v) => **v = Criteria::Constant(true),
                        _ => return Err(invalid("Selected node cannot be removed")),
                    };
                    replace_criteria(&mut value, &path[..path.len() - 1], parent);
                }
            }
            _ => unreachable!(),
        }
        self.install_builder_draft(id, QueryBuilderValue::Criteria(value))
    }
    fn builder_condition(&mut self, id: WidgetId) -> Result<(), ControlError> {
        self.guard_builder_composition(id)?;
        let e = &self.query_builders.entries[&id];
        let field = self
            .selected_keys(e.parts.field.container)
            .and_then(|v| v.first())
            .ok_or_else(|| invalid("Choose a field"))?;
        let field = e
            .schema
            .field(field)
            .ok_or_else(|| invalid("Unknown field"))?;
        let operator = self
            .selected_keys(e.parts.operator.container)
            .and_then(|v| v.first())
            .ok_or_else(|| invalid("Choose an operator"))?;
        let text = self.document(e.parts.operand).unwrap().text();
        let left = QueryExpression::Field(field.name.clone());
        let right = if matches!(operator.as_str(), "is_null" | "field" | "lower" | "length") {
            QueryExpression::Literal(CellValue::Null)
        } else {
            QueryExpression::Literal(field.parse(text).map_err(invalid)?)
        };
        let value = match self.current_builder_value(id)? {
            QueryBuilderValue::Criteria(mut root) => {
                let condition = match operator.as_str() {
                    "is_null" => Criteria::IsNull(left),
                    "contains" | "starts_with" | "ends_with" | "icontains" => Criteria::TextMatch {
                        text: left,
                        pattern: right,
                        operator: match operator.as_str() {
                            "contains" | "icontains" => TextMatchOperator::Contains,
                            "starts_with" => TextMatchOperator::StartsWith,
                            _ => TextMatchOperator::EndsWith,
                        },
                        comparison: if operator == "icontains" {
                            TextComparison::UnicodeLowercase
                        } else {
                            TextComparison::Ordinal
                        },
                    },
                    _ => Criteria::Compare {
                        left,
                        right,
                        operator: match operator.as_str() {
                            "eq" => ComparisonOperator::Equal,
                            "ne" => ComparisonOperator::NotEqual,
                            "lt" => ComparisonOperator::Less,
                            "le" => ComparisonOperator::LessOrEqual,
                            "gt" => ComparisonOperator::Greater,
                            "ge" => ComparisonOperator::GreaterOrEqual,
                            _ => return Err(invalid("Unsupported operator")),
                        },
                    },
                };
                replace_criteria(&mut root, &self.builder_path(id), condition);
                QueryBuilderValue::Criteria(root)
            }
            QueryBuilderValue::Expression(_) => {
                QueryBuilderValue::Expression(match operator.as_str() {
                    "field" => left,
                    "literal" => right,
                    "lower" => QueryExpression::Lowercase(Box::new(left)),
                    "length" => QueryExpression::TextLength(Box::new(left)),
                    "coalesce" => QueryExpression::Coalesce(Box::new(left), Box::new(right)),
                    "add" | "sub" | "mul" | "div" => QueryExpression::Arithmetic {
                        left: Box::new(left),
                        right: Box::new(right),
                        operator: match operator.as_str() {
                            "add" => ArithmeticOperator::Add,
                            "sub" => ArithmeticOperator::Subtract,
                            "mul" => ArithmeticOperator::Multiply,
                            _ => ArithmeticOperator::Divide,
                        },
                    },
                    _ => return Err(invalid("Unsupported expression operator")),
                })
            }
        };
        self.install_builder_draft(id, value)
    }
    fn complete_builder_text(&mut self, id: WidgetId) -> Result<(), ControlError> {
        let e = &self.query_builders.entries[&id];
        let insertion = self
            .selected_keys(e.parts.completion.container)
            .and_then(|v| v.first())
            .ok_or_else(|| invalid("Choose a completion"))?
            .clone();
        let document = self.document(e.parts.text).unwrap();
        let mut text = document.text().to_owned();
        let at = document.selection().focus.min(text.len());
        if !text.is_char_boundary(at) {
            return Err(invalid("Completion caret is not a scalar boundary"));
        }
        if text.len().saturating_add(insertion.len()) > MAX_QUERY_TEXT_BYTES {
            return Err(invalid("Query text exceeds 16 KiB"));
        }
        text.insert_str(at, &insertion);
        self.set_query_builder_text(id, &text)
    }
}
