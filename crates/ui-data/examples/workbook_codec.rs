//! Explicit XLSX interchange utility using the bounded workbook profile.
use rust_desktop_ui_data::workbook::{CellAddress, Workbook, WorkbookCell, WorkbookEdit};
use rust_desktop_ui_data::{CellValue, DateValue};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("sample") if args.len() == 3 => {
            let mut book = Workbook::new("Данные")?;
            let mut styled = WorkbookCell::value(CellValue::Text("План & результат 🦀".into()));
            styled.style.bold = true;
            styled.style.foreground = Some(0x1266aa);
            styled.style.background = Some(0xe0f0ff);
            styled.style.wrap = true;
            let mut edits = vec![
                WorkbookEdit::Set {
                    sheet: 0,
                    address: CellAddress::parse("A1")?,
                    cell: styled,
                },
                WorkbookEdit::AddSheet {
                    name: "Итоги".into(),
                },
            ];
            for (address, cell) in [
                ("B1", WorkbookCell::value(CellValue::Integer(7))),
                ("B2", WorkbookCell::value(CellValue::Real(2.5))),
                ("C1", WorkbookCell::value(CellValue::Bool(true))),
                (
                    "D1",
                    WorkbookCell::value(CellValue::Date(DateValue::new(2026, 10, 18)?)),
                ),
                ("A3", WorkbookCell::formula("=SUM(B1:B2)")),
            ] {
                edits.push(WorkbookEdit::Set {
                    sheet: 0,
                    address: CellAddress::parse(address)?,
                    cell,
                });
            }
            edits.push(WorkbookEdit::Set {
                sheet: 1,
                address: CellAddress::parse("A1")?,
                cell: WorkbookCell::formula("='Данные'!A3*2"),
            });
            book.apply(&edits)?;
            std::fs::write(&args[2], book.to_xlsx()?)?;
        }
        Some("inspect" | "roundtrip") if args.len() == 3 || args.len() == 4 => {
            let imported = Workbook::from_xlsx(&std::fs::read(&args[2])?)?;
            for diagnostic in &imported.diagnostics {
                eprintln!("{}: {}", diagnostic.part, diagnostic.message);
            }
            for (sheet, s) in imported.workbook.sheets().iter().enumerate() {
                for address in s.cells().keys() {
                    println!(
                        "{}!{} = {:?}",
                        s.name(),
                        address.a1(),
                        imported.workbook.value(sheet, *address)?
                    );
                }
            }
            if args[1] == "roundtrip" {
                let path = args.get(3).ok_or("roundtrip requires output path")?;
                std::fs::write(path, imported.workbook.to_xlsx()?)?;
            }
        }
        _ => return Err(
            "Usage: workbook_codec sample OUT.xlsx | inspect IN.xlsx | roundtrip IN.xlsx OUT.xlsx"
                .into(),
        ),
    }
    Ok(())
}
