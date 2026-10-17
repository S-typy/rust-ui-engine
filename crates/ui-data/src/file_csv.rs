use super::*;

struct Cell {
    text: String,
    quoted: bool,
}
/// An RFC-style quoted-field scanner; quotes are legal only at the field start,
/// and a closing quote is followed only by delimiter, line break or EOF.
struct Scanner<'a> {
    bytes: &'a [u8],
    index: usize,
    delimiter: u8,
    cell_limit: usize,
    record: usize,
    column: usize,
}
impl Scanner<'_> {
    fn row(&mut self) -> Result<Option<Vec<Cell>>, DataError> {
        if self.index == self.bytes.len() {
            return Ok(None);
        }
        self.record += 1;
        let mut cells = Vec::new();
        loop {
            self.column = cells.len() + 1;
            if cells.len() > DataLimits::default().max_fields {
                return Err(DataError::Capacity("CSV column limit exceeded".into()));
            }
            let quoted = self.bytes.get(self.index) == Some(&b'"');
            if quoted {
                self.index += 1;
            }
            let mut data = Vec::new();
            let mut closed = !quoted;
            while self.index < self.bytes.len() {
                let byte = self.bytes[self.index];
                if quoted {
                    if byte == b'"' {
                        self.index += 1;
                        if self.bytes.get(self.index) == Some(&b'"') {
                            self.index += 1;
                            self.push(&mut data, b'"')?;
                            continue;
                        }
                        closed = true;
                        break;
                    }
                } else if byte == self.delimiter || matches!(byte, b'\r' | b'\n') {
                    break;
                } else if byte == b'"' {
                    return Err(self.error("quote inside unquoted field"));
                }
                self.push(&mut data, byte)?;
                self.index += 1;
            }
            if !closed {
                return Err(self.error("unterminated quoted field"));
            }
            let text = String::from_utf8(data).map_err(|_| self.error("invalid UTF-8 cell"))?;
            cells.push(Cell { text, quoted });
            match self.bytes.get(self.index).copied() {
                Some(byte) if byte == self.delimiter => {
                    self.index += 1;
                }
                Some(b'\n') => {
                    self.index += 1;
                    break;
                }
                Some(b'\r') => {
                    self.index += 1;
                    if self.bytes.get(self.index) == Some(&b'\n') {
                        self.index += 1;
                    }
                    break;
                }
                None => break,
                _ => return Err(self.error("unexpected text after closing quote")),
            }
        }
        Ok(Some(cells))
    }
    fn error(&self, message: &str) -> DataError {
        DataError::Adapter(format!(
            "CSV record {}, column {}: {message}",
            self.record, self.column
        ))
    }
    fn push(&self, output: &mut Vec<u8>, byte: u8) -> Result<(), DataError> {
        if output.len() >= self.cell_limit {
            return Err(self.error("cell byte budget exceeded"));
        }
        output.push(byte);
        Ok(())
    }
}
pub(super) fn decode(
    text: &str,
    mapping: &FileTableMapping,
    options: DataFileOptions,
    csv: CsvOptions,
) -> Result<DataTable, DataError> {
    let limit = options
        .limits
        .max_text_bytes
        .max(
            options
                .limits
                .max_binary_bytes
                .saturating_mul(2)
                .saturating_add(2),
        )
        .max(256)
        .min(options.max_file_bytes);
    let mut scanner = Scanner {
        bytes: text.as_bytes(),
        index: 0,
        delimiter: csv.delimiter,
        cell_limit: limit,
        record: 0,
        column: 0,
    };
    let headers = scanner
        .row()?
        .ok_or_else(|| DataError::Schema("CSV requires a header record".into()))?;
    if headers.len() != mapping.columns.len() + 1 {
        return Err(DataError::Schema(
            "CSV header width differs from mapping".into(),
        ));
    }
    let mut seen = HashSet::new();
    let mut positions = Vec::new();
    for header in headers {
        let index = mapping
            .position(&header.text)
            .ok_or_else(|| DataError::Schema("CSV header contains an unmapped column".into()))?;
        if !seen.insert(index) {
            return Err(DataError::Schema(
                "CSV header contains a duplicate column".into(),
            ));
        }
        positions.push(index);
    }
    let mut builder = TableBuilder::new(mapping, options);
    while let Some(cells) = scanner.row()? {
        let record = builder.rows.len() + 1;
        if cells.len() != positions.len() {
            return Err(diagnostic(
                record,
                "<row>",
                "CSV row width differs from header",
            ));
        }
        let mut key = None;
        let mut values = vec![CellValue::Null; mapping.columns.len()];
        for (cell, &index) in cells.into_iter().zip(&positions) {
            let error = |message| diagnostic(record, mapping.column(index), message);
            if index == 0 {
                key =
                    Some(parse_key(&cell.text).ok_or_else(|| error("key requires canonical i64"))?);
                continue;
            }
            let field = &mapping.schema.fields()[index - 1];
            let value = if !cell.quoted && cell.text == "\\N" {
                CellValue::Null
            } else {
                parse_scalar(&cell.text, field.kind)
                    .ok_or_else(|| error("invalid canonical typed value"))?
            };
            field
                .validate(&value)
                .map_err(|_| error("wrong type or forbidden null"))?;
            values[index - 1] = value;
        }
        builder.push(DataRow::new(key.expect("header has key"), values))?;
    }
    builder.finish()
}
fn cell(out: &mut TextWriter, text: &str, null: bool, csv: CsvOptions) -> Result<(), DataError> {
    let quoted = !null
        && (text == "\\N"
            || text
                .bytes()
                .any(|b| b == csv.delimiter || matches!(b, b'"' | b'\n' | b'\r')));
    if quoted {
        out.push("\"")?;
    }
    for chunk in text.split_inclusive('"') {
        out.push(chunk)?;
        if chunk.ends_with('"') {
            out.push("\"")?;
        }
    }
    if quoted {
        out.push("\"")?;
    }
    Ok(())
}
pub(super) fn encode(
    table: &DataTable,
    mapping: &FileTableMapping,
    options: DataFileOptions,
    csv: CsvOptions,
) -> Result<String, DataError> {
    let mut out = TextWriter::new(options);
    let delimiter = String::from(char::from(csv.delimiter));
    for index in 0..=mapping.columns.len() {
        if index > 0 {
            out.push(&delimiter)?;
        }
        cell(&mut out, mapping.column(index), false, csv)?;
    }
    out.push("\r\n")?;
    for row in table.rows() {
        cell(&mut out, &row.key.to_string(), false, csv)?;
        for value in &row.values {
            out.push(&delimiter)?;
            cell(
                &mut out,
                &scalar_text(value),
                matches!(value, CellValue::Null),
                csv,
            )?;
        }
        out.push("\r\n")?;
    }
    Ok(out.text)
}
