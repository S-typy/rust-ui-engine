//! Internal, bounded DTOs shared by composition document formats.
use rust_desktop_ui_data::*;
use serde::{Deserialize, Serialize};

pub(crate) fn error(e: impl ToString) -> String {
    e.to_string()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct QueryDto {
    filter: Option<String>,
    order: Vec<(String, bool)>,
    groups: Vec<String>,
    aggregates: Vec<(String, Option<String>)>,
}
impl QueryDto {
    pub fn encode(q: &QueryRequest) -> Result<Self, String> {
        Ok(Self {
            filter: q
                .filter
                .as_ref()
                .map(format_criteria_text)
                .transpose()
                .map_err(error)?,
            order: q
                .order_by
                .iter()
                .map(|v| {
                    Ok((
                        format_expression_text(&v.expression).map_err(error)?,
                        v.direction == SortDirection::Descending,
                    ))
                })
                .collect::<Result<_, String>>()?,
            groups: q
                .group_by
                .iter()
                .map(format_expression_text)
                .collect::<Result<_, _>>()
                .map_err(error)?,
            aggregates: q
                .aggregates
                .iter()
                .map(|v| {
                    Ok((
                        format!("{:?}", v.function),
                        v.expression
                            .as_ref()
                            .map(format_expression_text)
                            .transpose()
                            .map_err(error)?,
                    ))
                })
                .collect::<Result<_, String>>()?,
        })
    }
    pub fn decode(&self, schema: &TableSchema) -> Result<QueryRequest, String> {
        if self.order.len() + self.groups.len() + self.aggregates.len() > 128 {
            return Err("Query term limit exceeded".into());
        }
        let request = QueryRequest {
            filter: self
                .filter
                .as_ref()
                .map(|s| parse_criteria_text(schema, s))
                .transpose()
                .map_err(error)?,
            order_by: self
                .order
                .iter()
                .map(|(s, desc)| {
                    Ok(QueryOrder {
                        expression: parse_expression_text(schema, s).map_err(error)?,
                        direction: if *desc {
                            SortDirection::Descending
                        } else {
                            SortDirection::Ascending
                        },
                    })
                })
                .collect::<Result<_, String>>()?,
            group_by: self
                .groups
                .iter()
                .map(|s| parse_expression_text(schema, s))
                .collect::<Result<_, _>>()
                .map_err(error)?,
            aggregates: self
                .aggregates
                .iter()
                .map(|(function, s)| {
                    Ok(Aggregate {
                        function: match function.as_str() {
                            "CountRows" => AggregateFunction::CountRows,
                            "CountValues" => AggregateFunction::CountValues,
                            "Sum" => AggregateFunction::Sum,
                            "Minimum" => AggregateFunction::Minimum,
                            "Maximum" => AggregateFunction::Maximum,
                            "Average" => AggregateFunction::Average,
                            _ => return Err("Unknown aggregate".into()),
                        },
                        expression: s
                            .as_ref()
                            .map(|s| parse_expression_text(schema, s))
                            .transpose()
                            .map_err(error)?,
                    })
                })
                .collect::<Result<_, String>>()?,
        };
        QueryEngine::compile(schema, &request).map_err(error)?;
        Ok(request)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FieldDto {
    name: String,
    title: String,
    kind: String,
    nullable: bool,
    read_only: bool,
}
impl FieldDto {
    pub fn encode(f: &FieldSchema) -> Self {
        Self {
            name: f.name.clone(),
            title: f.title.clone(),
            kind: format!("{:?}", f.kind),
            nullable: f.nullable,
            read_only: f.read_only,
        }
    }
    pub fn decode(self) -> Result<FieldSchema, String> {
        let kind = match self.kind.as_str() {
            "Bool" => FieldType::Bool,
            "Integer" => FieldType::Integer,
            "Real" => FieldType::Real,
            "Text" => FieldType::Text,
            "Decimal" => FieldType::Decimal,
            "Date" => FieldType::Date,
            "Time" => FieldType::Time,
            "Binary" => FieldType::Binary,
            _ => return Err("Unknown field type".into()),
        };
        Ok(FieldSchema {
            name: self.name,
            title: self.title,
            kind,
            nullable: self.nullable,
            read_only: self.read_only,
        })
    }
}

pub(crate) fn literal_encode(v: &CellValue) -> Result<String, String> {
    format_expression_text(&QueryExpression::Literal(v.clone())).map_err(error)
}
pub(crate) fn literal_decode(v: &str, schema: &TableSchema) -> Result<CellValue, String> {
    match parse_expression_text(schema, v).map_err(error)? {
        QueryExpression::Literal(v) => Ok(v),
        _ => Err("Expected a literal value".into()),
    }
}
