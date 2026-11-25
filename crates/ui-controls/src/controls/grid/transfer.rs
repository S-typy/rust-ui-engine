//! Keyed cell ranges, clipboard tables and atomic row transfers.
use super::*;
use rust_desktop_ui_data::{DataRow, DataWrite, FieldType, spreadsheet::parse_tsv};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridCellRange {
    pub rows: Vec<i64>,
    pub fields: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridTransfer {
    pub tsv: String,
    pub html: String,
}
#[derive(Default)]
pub(super) struct TransferState {
    pub(super) ranges: Vec<GridCellRange>,
    masks: HashMap<i64, u64>,
    pub(super) anchor: Option<(i64, String)>,
}
impl TransferState {
    pub(super) fn clear_ranges(&mut self) {
        self.ranges.clear();
        self.masks.clear();
    }
}
pub(super) fn html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn tsv_cell(value: &str) -> String {
    if value.contains(['\t', '\r', '\n', '"']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}
impl GridState {
    pub(super) fn visual_keys(&self) -> Result<Vec<i64>, ControlError> {
        let mut keys = Vec::new();
        for i in 0..self.model.row_count() {
            if let Some(row) = self
                .model
                .row(&self.source, i)
                .map_err(error)?
                .and_then(|r| r.key)
                && !self.source.projection.synthetic.contains_key(&row)
            {
                keys.push(data_key(row));
            }
        }
        Ok(keys)
    }
    pub(super) fn range_contains(&self, key: i64, field: &str) -> bool {
        self.transfer.masks.get(&key).is_some_and(|mask| {
            self.source
                .fields
                .iter()
                .position(|f| f == field)
                .is_some_and(|i| mask & (1u64 << i) != 0)
        })
    }
}
impl Controls {
    pub fn grid_cell_ranges(&self, id: WidgetId) -> Option<&[GridCellRange]> {
        Some(&self.grids.get(&id)?.transfer.ranges)
    }
    pub fn select_grid_range(
        &mut self,
        id: WidgetId,
        from: (i64, &str),
        to: (i64, &str),
        append: bool,
    ) -> Result<(), ControlError> {
        let g = self
            .grids
            .get_mut(&id)
            .ok_or_else(|| error("Unknown grid"))?;
        let keys = g.visual_keys()?;
        let cols = g
            .model
            .display_column_order()
            .into_iter()
            .map(|c| g.source.fields[c.0 as usize].clone())
            .collect::<Vec<_>>();
        let row = |key| {
            keys.iter()
                .position(|k| *k == key)
                .ok_or_else(|| error("Range row is not visible"))
        };
        let col = |field| {
            cols.iter()
                .position(|f| f == field)
                .ok_or_else(|| error("Range field is not visible"))
        };
        let (a, b, c, d) = (row(from.0)?, row(to.0)?, col(from.1)?, col(to.1)?);
        if append && g.transfer.ranges.len() >= 64 {
            return Err(error("Too many disjoint ranges"));
        }
        let range = GridCellRange {
            rows: keys[a.min(b)..=a.max(b)].to_vec(),
            fields: cols[c.min(d)..=c.max(d)].to_vec(),
        };
        if !append {
            g.transfer.clear_ranges();
        }
        let mask = range
            .fields
            .iter()
            .filter_map(|f| g.source.fields.iter().position(|field| field == f))
            .fold(0u64, |mask, i| mask | (1 << i));
        for &key in &range.rows {
            *g.transfer.masks.entry(key).or_default() |= mask;
        }
        g.transfer.ranges.push(range);
        self.scene_dirty = true;
        Ok(())
    }
    pub fn copy_grid_selection(
        &self,
        id: WidgetId,
        headers: bool,
    ) -> Result<GridTransfer, ControlError> {
        let g = self.grids.get(&id).ok_or_else(|| error("Unknown grid"))?;
        let cols = g.model.display_column_order();
        let rows = g.visual_keys()?;
        let ranges = &g.transfer.ranges;
        let cols = cols
            .into_iter()
            .filter(|c| {
                ranges.is_empty()
                    || ranges
                        .iter()
                        .any(|r| r.fields.contains(&g.source.fields[c.0 as usize]))
            })
            .collect::<Vec<_>>();
        let mut result = GridTransfer {
            tsv: String::new(),
            html: "<table>".into(),
        };
        let mut count = 0;
        let mut add = |values: Vec<String>, tag: &str| -> Result<(), ControlError> {
            if count > 0 {
                result.tsv.push_str("\r\n");
            }
            count += 1;
            result.html.push_str("<tr>");
            for (i, value) in values.iter().enumerate() {
                if i > 0 {
                    result.tsv.push('\t');
                }
                result.tsv.push_str(&tsv_cell(value));
                result
                    .html
                    .push_str(&format!("<{tag}>{}</{tag}>", html(value)));
                if result.html.len() + result.tsv.len() > 4 * 1024 * 1024 {
                    return Err(error("Grid clipboard limit exceeded"));
                }
            }
            result.html.push_str("</tr>");
            Ok(())
        };
        if headers {
            add(
                cols.iter()
                    .map(|c| g.settings.columns[c.0 as usize].title.clone())
                    .collect(),
                "th",
            )?;
        }
        let mut any = false;
        for key in rows {
            let included = if ranges.is_empty() {
                g.model.is_selected(row_key(key)) || g.selected_key == Some(key)
            } else {
                g.transfer.masks.contains_key(&key)
            };
            if !included {
                continue;
            }
            any = true;
            add(
                cols.iter()
                    .map(|c| {
                        if ranges.is_empty()
                            || g.range_contains(key, &g.source.fields[c.0 as usize])
                        {
                            g.source.cell(row_key(key), *c)
                        } else {
                            String::new()
                        }
                    })
                    .collect(),
                "td",
            )?;
        }
        if !any {
            return Err(error("No visible selection"));
        }
        result.html.push_str("</table>");
        Ok(result)
    }
    pub fn paste_grid_tsv(
        &mut self,
        id: WidgetId,
        origin: (i64, &str),
        text: &str,
        headers: bool,
    ) -> Result<bool, ControlError> {
        let g = self.grids.get(&id).ok_or_else(|| error("Unknown grid"))?;
        if g.records.draft.is_some() {
            return Err(error("Finish the active record first"));
        }
        let matrix = parse_tsv(text).map_err(error)?;
        let cols = g.model.display_column_order();
        let keys = g.visual_keys()?;
        let r = keys
            .iter()
            .position(|k| *k == origin.0)
            .ok_or_else(|| error("Paste row is not visible"))?;
        let c = cols
            .iter()
            .position(|c| g.source.fields[c.0 as usize] == origin.1)
            .ok_or_else(|| error("Paste column is hidden"))?;
        let width = matrix.first().map_or(0, Vec::len);
        if width == 0
            || c + width > cols.len()
            || matrix.len() <= usize::from(headers)
            || matrix.len() - usize::from(headers) > keys.len() - r
            || matrix.iter().any(|v| v.len() != width)
        {
            return Err(error("Paste rectangle does not fit"));
        }
        let cols = &cols[c..c + width];
        if headers
            && !matrix[0]
                .iter()
                .zip(cols)
                .all(|(h, c)| h == &g.settings.columns[c.0 as usize].title)
        {
            return Err(error("Paste headers differ from target fields"));
        }
        let mut ops = Vec::new();
        for (i, row) in matrix.iter().skip(usize::from(headers)).enumerate() {
            let key = keys[r + i];
            for (text, col) in row.iter().zip(cols) {
                let field = &g.source.fields[col.0 as usize];
                if !g.source.editable(row_key(key), *col)
                    || g.settings.columns[col.0 as usize].read_only
                {
                    return Err(error("Read-only paste target"));
                }
                let schema = g.source.table.schema().field(field).unwrap();
                let value = schema.parse(text).map_err(error)?;
                ops.push(DataWrite::Update {
                    key,
                    field: field.clone(),
                    expected: g.source.table.value(key, field).unwrap().clone(),
                    value,
                });
            }
        }
        self.apply_grid_batch(id, &ops, None)
    }
    /// Place selected keys before a target in an application-owned integer order field.
    pub fn move_grid_rows(
        &mut self,
        id: WidgetId,
        keys: &[i64],
        before: Option<i64>,
        order_field: &str,
    ) -> Result<bool, ControlError> {
        let g = self.grids.get(&id).ok_or_else(|| error("Unknown grid"))?;
        if g.records.draft.is_some() {
            return Err(error("Finish the active record first"));
        }
        let table = &g.source.table;
        let schema = table
            .schema()
            .field(order_field)
            .ok_or_else(|| error("Missing order field"))?;
        if schema.kind != FieldType::Integer || schema.read_only {
            return Err(error("Order field must be a writable integer"));
        }
        let picked: HashSet<_> = keys.iter().copied().collect();
        if picked.len() != keys.len()
            || keys.is_empty()
            || keys.iter().any(|k| !g.source.positions.contains_key(k))
            || before.is_some_and(|k| !g.source.positions.contains_key(&k) || picked.contains(&k))
        {
            return Err(error("Invalid move endpoints"));
        }
        let mut all = table
            .rows()
            .iter()
            .filter(|row| g.source.check_relation_row(table.schema(), row).is_ok())
            .map(|r| r.key)
            .collect::<Vec<_>>();
        all.sort_by_key(|key| {
            (
                match table.value(*key, order_field) {
                    Some(CellValue::Integer(i)) => Some(*i),
                    _ => None,
                },
                *key,
            )
        });
        let moving = all
            .iter()
            .copied()
            .filter(|k| picked.contains(k))
            .collect::<Vec<_>>();
        all.retain(|k| !picked.contains(k));
        let at = before
            .and_then(|key| all.iter().position(|k| *k == key))
            .unwrap_or(all.len());
        let rank = |key: &i64| match table.value(*key, order_field) {
            Some(CellValue::Integer(n)) => Some(i128::from(*n)),
            _ => None,
        };
        let count = moving.len() as i128;
        let low = at.checked_sub(1).and_then(|i| all.get(i)).and_then(rank);
        let high = all.get(at).and_then(rank);
        let bounds = match (low, high) {
            (Some(a), Some(b)) => Some((a, b)),
            (None, Some(b)) if at == 0 => Some((b - 1024 * (count + 1), b)),
            (Some(a), None) if at == all.len() => Some((a, a + 1024 * (count + 1))),
            _ => None,
        };
        let gap = bounds.and_then(|(a, b)| {
            let step = (b - a) / (count + 1);
            (step > 0 && i64::try_from(a + step).is_ok() && i64::try_from(a + step * count).is_ok())
                .then_some((a, step))
        });
        let ordered: Vec<_> = if let Some((start, step)) = gap {
            moving
                .into_iter()
                .enumerate()
                .map(|(i, k)| (k, (start + (i as i128 + 1) * step) as i64))
                .collect()
        } else {
            all.splice(at..at, moving);
            all.into_iter()
                .enumerate()
                .map(|(i, k)| (k, i as i64 * 1024))
                .collect()
        };
        let ops: Vec<_> = ordered
            .into_iter()
            .filter_map(|(key, n)| {
                let expected = table.value(key, order_field)?.clone();
                let value = CellValue::Integer(n);
                (value != expected).then(|| DataWrite::Update {
                    key,
                    field: order_field.to_owned(),
                    expected,
                    value,
                })
            })
            .collect();
        self.apply_grid_batch(id, &ops, None)
    }
    /// Copy records in one target transaction. Colliding keys reject the whole transfer.
    pub fn copy_grid_rows_to(
        &mut self,
        from: WidgetId,
        to: WidgetId,
        keys: &[i64],
    ) -> Result<bool, ControlError> {
        let g = self
            .grids
            .get(&from)
            .ok_or_else(|| error("Unknown drag source"))?;
        let target = self
            .grids
            .get(&to)
            .ok_or_else(|| error("Unknown drop target"))?;
        if target.records.draft.is_some() {
            return Err(error("Finish the target record first"));
        }
        let source = g.source.handle.try_borrow().map_err(error)?;
        let dest = target.source.handle.try_borrow().map_err(error)?;
        if source.table().schema() != dest.table().schema() {
            return Err(error("Transfer schemas differ"));
        }
        let mut ops = Vec::new();
        for &key in keys {
            if !g.source.positions.contains_key(&key) {
                return Err(error("Dragged record outside source view"));
            }
            let row: &DataRow = source
                .table()
                .row(key)
                .ok_or_else(|| error("Missing dragged record"))?;
            ops.push(DataWrite::Insert(row.clone()));
        }
        drop(dest);
        drop(source);
        self.apply_grid_batch(to, &ops, None)
    }
}
