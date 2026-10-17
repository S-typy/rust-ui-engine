use super::*;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::fmt;

enum Scalar {
    Null,
    Bool(bool),
    Signed(i64),
    Unsigned(u64),
    Real(f64),
    Text(String),
}
struct ScalarSeed;
impl<'de> DeserializeSeed<'de> for ScalarSeed {
    type Value = Scalar;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Scalar, D::Error> {
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for ScalarSeed {
    type Value = Scalar;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a scalar cell")
    }
    fn visit_unit<E: de::Error>(self) -> Result<Scalar, E> {
        Ok(Scalar::Null)
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Scalar, E> {
        Ok(Scalar::Bool(v))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Scalar, E> {
        Ok(Scalar::Signed(v))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Scalar, E> {
        Ok(Scalar::Unsigned(v))
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Scalar, E> {
        Ok(Scalar::Real(v))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Scalar, E> {
        Ok(Scalar::Text(v.into()))
    }
    fn visit_string<E: de::Error>(self, v: String) -> Result<Scalar, E> {
        Ok(Scalar::Text(v))
    }
}
struct RowSeed<'a> {
    mapping: &'a FileTableMapping,
    options: DataFileOptions,
    record: usize,
}
impl<'de> DeserializeSeed<'de> for RowSeed<'_> {
    type Value = DataRow;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<DataRow, D::Error> {
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for RowSeed<'_> {
    type Value = DataRow;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a mapped row object")
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<DataRow, E> {
        Err(E::custom(format!(
            "record {}: expected row object",
            self.record
        )))
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<DataRow, M::Error> {
        let mut seen = vec![false; self.mapping.columns.len() + 1];
        let mut key = None;
        let mut cells = vec![CellValue::Null; self.mapping.columns.len()];
        while let Some(name) = map.next_key::<String>()? {
            let index = self.mapping.position(&name).ok_or_else(|| {
                de::Error::custom(format!("record {}: unmapped column", self.record))
            })?;
            let error = |message| {
                de::Error::custom(diagnostic(self.record, self.mapping.column(index), message))
            };
            if std::mem::replace(&mut seen[index], true) {
                return Err(error("duplicate column"));
            }
            let scalar = map.next_value_seed(ScalarSeed)?;
            if index == 0 {
                key = Some(match scalar {
                    Scalar::Signed(v) => v,
                    Scalar::Unsigned(v) => {
                        i64::try_from(v).map_err(|_| error("key outside i64"))?
                    }
                    Scalar::Text(v) => {
                        parse_key(&v).ok_or_else(|| error("key requires canonical i64"))?
                    }
                    _ => return Err(error("key requires canonical i64 string or integer token")),
                });
                continue;
            }
            let field = &self.mapping.schema.fields()[index - 1];
            let value = match (field.kind, scalar) {
                (_, Scalar::Null) => CellValue::Null,
                (FieldType::Bool, Scalar::Bool(v)) => CellValue::Bool(v),
                (FieldType::Integer, Scalar::Signed(v)) => CellValue::Integer(v),
                (FieldType::Integer, Scalar::Unsigned(v)) => {
                    CellValue::Integer(i64::try_from(v).map_err(|_| error("integer outside i64"))?)
                }
                (FieldType::Real, Scalar::Real(v)) if v.is_finite() => CellValue::Real(v),
                (FieldType::Real, Scalar::Signed(v)) => {
                    // Exact integer tokens must not silently round in a Real field.
                    let f = v as f64;
                    if f as i128 != i128::from(v) {
                        return Err(error("integer token cannot be represented exactly as real"));
                    }
                    CellValue::Real(f)
                }
                (FieldType::Real, Scalar::Unsigned(v)) => {
                    let f = v as f64;
                    if f as u128 != u128::from(v) {
                        return Err(error("integer token cannot be represented exactly as real"));
                    }
                    CellValue::Real(f)
                }
                (kind, Scalar::Text(v)) if !matches!(kind, FieldType::Bool | FieldType::Real) => {
                    let limit = if kind == FieldType::Binary {
                        self.options
                            .limits
                            .max_binary_bytes
                            .saturating_mul(2)
                            .saturating_add(2)
                    } else if kind == FieldType::Text {
                        self.options.limits.max_text_bytes
                    } else {
                        128
                    };
                    if v.len() > limit {
                        return Err(error("cell byte budget exceeded"));
                    }
                    parse_scalar(&v, kind).ok_or_else(|| error("invalid canonical typed value"))?
                }
                _ => return Err(error("wrong JSON scalar type")),
            };
            field
                .validate(&value)
                .map_err(|_| error("wrong type or forbidden null"))?;
            cells[index - 1] = value;
        }
        if let Some(index) = seen.iter().position(|seen| !*seen) {
            return Err(de::Error::custom(diagnostic(
                self.record,
                self.mapping.column(index),
                "missing column",
            )));
        }
        Ok(DataRow::new(key.expect("key presence checked"), cells))
    }
}
struct TableSeed<'a> {
    mapping: &'a FileTableMapping,
    options: DataFileOptions,
}
impl<'de> DeserializeSeed<'de> for TableSeed<'_> {
    type Value = DataTable;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<DataTable, D::Error> {
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for TableSeed<'_> {
    type Value = DataTable;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("an array of row objects")
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<DataTable, E> {
        Err(E::custom("expected array of row objects"))
    }
    fn visit_seq<S: SeqAccess<'de>>(self, mut sequence: S) -> Result<DataTable, S::Error> {
        let mut builder = TableBuilder::new(self.mapping, self.options);
        while let Some(row) = sequence.next_element_seed(RowSeed {
            mapping: self.mapping,
            options: self.options,
            record: builder.rows.len() + 1,
        })? {
            builder.push(row).map_err(de::Error::custom)?;
        }
        builder.finish().map_err(de::Error::custom)
    }
}
pub(super) fn decode(
    text: &str,
    mapping: &FileTableMapping,
    options: DataFileOptions,
) -> Result<DataTable, DataError> {
    validate_overflow_integer_tokens(text)?;
    let mut parser = serde_json::Deserializer::from_str(text);
    let table = TableSeed { mapping, options }
        .deserialize(&mut parser)
        .map_err(|e| DataError::Adapter(format!("JSON: {e}")))?;
    parser
        .end()
        .map_err(|e| DataError::Adapter(format!("JSON: {e}")))?;
    Ok(table)
}

/// serde_json dispatches integer tokens outside its i64/u64 range as f64. Check
/// their lexical value first, so a Real field cannot silently accept rounding.
/// Strings are skipped without allocation; JSON syntax is still checked by the
/// deserializer. The largest finite fixed-point f64 expansion is bounded.
fn validate_overflow_integer_tokens(text: &str) -> Result<(), DataError> {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                index += 1;
                while index < bytes.len() {
                    match bytes[index] {
                        b'\\' => index = (index + 2).min(bytes.len()),
                        b'"' => {
                            index += 1;
                            break;
                        }
                        _ => index += 1,
                    }
                }
            }
            b'-' | b'0'..=b'9' => {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && matches!(bytes[index], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
                {
                    index += 1;
                }
                let token = &text[start..index];
                if !token.bytes().any(|b| matches!(b, b'.' | b'e' | b'E'))
                    && token.parse::<i64>().is_err()
                    && token.parse::<u64>().is_err()
                {
                    let exact = token
                        .parse::<f64>()
                        .ok()
                        .filter(|v| v.is_finite())
                        .is_some_and(|v| format!("{v:.0}") == token);
                    if !exact {
                        return Err(DataError::Adapter(format!(
                            "JSON byte {start}: integer token cannot be represented exactly; use a canonical typed string"
                        )));
                    }
                }
            }
            _ => index += 1,
        }
    }
    Ok(())
}

fn quoted(out: &mut TextWriter, value: &str) -> Result<(), DataError> {
    out.push("\"")?;
    for ch in value.chars() {
        match ch {
            '"' => out.push("\\\"")?,
            '\\' => out.push("\\\\")?,
            '\n' => out.push("\\n")?,
            '\r' => out.push("\\r")?,
            '\t' => out.push("\\t")?,
            ch if ch <= '\u{1f}' => out.push(&format!("\\u{:04x}", u32::from(ch)))?,
            ch => {
                let mut buf = [0; 4];
                out.push(ch.encode_utf8(&mut buf))?;
            }
        }
    }
    out.push("\"")
}
pub(super) fn encode(
    table: &DataTable,
    mapping: &FileTableMapping,
    options: DataFileOptions,
) -> Result<String, DataError> {
    let mut out = TextWriter::new(options);
    out.push("[")?;
    for (r, row) in table.rows().iter().enumerate() {
        if r > 0 {
            out.push(",")?;
        }
        out.push("\n  {")?;
        quoted(&mut out, &mapping.key_column)?;
        out.push(":")?;
        quoted(&mut out, &row.key.to_string())?;
        for (index, value) in row.values.iter().enumerate() {
            out.push(",")?;
            quoted(&mut out, &mapping.columns[index])?;
            out.push(":")?;
            match value {
                CellValue::Null => out.push("null")?,
                CellValue::Bool(v) => out.push(if *v { "true" } else { "false" })?,
                CellValue::Real(v) => out.push(
                    &serde_json::to_string(v)
                        .map_err(|_| DataError::Validation("nonfinite real".into()))?,
                )?,
                _ => quoted(&mut out, &scalar_text(value))?,
            }
        }
        out.push("}")?;
    }
    out.push("\n]\n")?;
    Ok(out.text)
}
