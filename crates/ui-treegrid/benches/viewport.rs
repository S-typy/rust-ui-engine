//! Headless CPU viewport benchmark. This does not measure GPU or OS input.

use rust_desktop_ui_core::{Rect, Size};
use rust_desktop_ui_treegrid::{ColumnId, GridColumn, RowKey, TreeDataSource, TreeGrid};
use std::{hint::black_box, time::Instant};

struct Books {
    count: usize,
}
impl TreeDataSource for Books {
    fn child_count(&self, parent: Option<RowKey>) -> Option<usize> {
        Some(if parent.is_none() { self.count } else { 0 })
    }
    fn child_at(&self, parent: Option<RowKey>, index: usize) -> Option<RowKey> {
        (parent.is_none() && index < self.count).then_some(RowKey(index as u64 + 1))
    }
    fn parent(&self, _row: RowKey) -> Option<RowKey> {
        None
    }
    fn position(&self, row: RowKey) -> Option<usize> {
        (row.0 > 0 && row.0 <= self.count as u64).then_some(row.0.saturating_sub(1) as usize)
    }
    fn cell(&self, row: RowKey, column: ColumnId) -> String {
        match column.0 {
            0 => format!("Книга {}", row.0),
            1 => format!("Автор {}", row.0 % 173),
            2 => format!("{}", 1900 + row.0 % 125),
            3 => format!("{} стр.", 80 + row.0 % 920),
            _ => format!("Поле {}: {}", column.0, row.0),
        }
    }
}

fn main() {
    let iterations = 500usize;
    println!(
        "{{\"kind\":\"headless_cpu_viewport\",\"os\":\"{}\",\"arch\":\"{}\",\"iterations\":{},\"results\":[",
        std::env::consts::OS,
        std::env::consts::ARCH,
        iterations
    );
    for (test, count) in [1_000usize, 100_000, 1_000_000].into_iter().enumerate() {
        let mut source = Books { count };
        let columns = (0..50)
            .map(|index| {
                let mut column =
                    GridColumn::new(ColumnId(index), format!("Колонка {index}"), 140.0);
                column.pinned = index == 0;
                column
            })
            .collect();
        let mut grid = TreeGrid::new(columns).unwrap();
        grid.refresh(&mut source).unwrap();
        grid.set_viewport(Size::new(1280.0, 720.0)).unwrap();
        let mut timings = Vec::with_capacity(iterations);
        let mut rows = 0;
        let mut cells = 0;
        let mut columns = 0;
        for iteration in 0..iterations + 30 {
            let index = iteration.wrapping_mul(7919) % count;
            grid.scroll_to((iteration % 20) as f64 * 70.0, index as f64 * 28.0 + 7.0)
                .unwrap();
            let start = Instant::now();
            let frame = grid
                .paint(black_box(&source), Rect::new(0.0, 0.0, 1280.0, 720.0))
                .unwrap();
            rows = rows.max(frame.stats.materialized_rows);
            cells = cells.max(frame.stats.cells_read);
            columns = columns.max(frame.stats.visible_columns);
            black_box(frame);
            if iteration >= 30 {
                timings.push(start.elapsed().as_secs_f64() * 1_000_000.0);
            }
        }
        timings.sort_by(f64::total_cmp);
        assert!(rows <= 31 && cells <= 31 * 11);
        assert_eq!(grid.index_segment_count(), 1);
        if test != 0 {
            println!(",");
        }
        print!(
            "{{\"logical_rows\":{count},\"p50_us\":{:.3},\"p95_us\":{:.3},\"p99_us\":{:.3},\"max_materialized_rows\":{rows},\"max_visible_columns\":{columns},\"max_cells_read\":{cells},\"index_segments\":{}}}",
            timings[iterations / 2],
            timings[iterations * 95 / 100],
            timings[iterations * 99 / 100],
            grid.index_segment_count()
        );
    }
    println!("]}}");
}
