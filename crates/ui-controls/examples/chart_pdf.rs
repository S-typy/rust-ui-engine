//! Export a chart to an explicitly supplied, new PDF file without opening a window.
use rust_desktop_ui_controls::{
    ChartModel, ChartPoint, ChartSeries, ChartSettings, ChartView,
    printing::{ChartPrintOptions, export_pdf, from_chart},
};
use rust_desktop_ui_text::TextEngine;
use std::{fs::OpenOptions, io::Write, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("Usage: chart_pdf <new-output.pdf>")?,
    );
    let model = ChartModel::new(
        vec![ChartSeries {
            id: 1,
            label: "Выручка".into(),
            points: [24.0, 38.0, 31.0, 46.0]
                .into_iter()
                .enumerate()
                .map(|(index, value)| ChartPoint {
                    id: index as i64,
                    x: index as f64 + 1.0,
                    y: Some(value),
                })
                .collect(),
        }],
        ChartSettings {
            view: ChartView::Bar,
            x_title: "Квартал".into(),
            y_title: "Выручка, млн".into(),
            ..Default::default()
        },
    )?;
    let mut engine = TextEngine::new();
    let document = from_chart(
        &model,
        &mut engine,
        "Годовой отчёт",
        ChartPrintOptions::default(),
        || false,
    )?;
    let bytes = export_pdf(&document, &mut engine, || false)?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)?
        .write_all(&bytes)?;
    println!(
        "Saved {} page to {}",
        document.pages().len(),
        target.display()
    );
    Ok(())
}
