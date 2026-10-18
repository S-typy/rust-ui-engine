//! Original sample content for the rich-document form.
use rust_desktop_ui_controls::rich_document::*;

pub(crate) fn sample_document() -> Result<RichDocument, DocumentError> {
    let mut document = RichDocument::from_text(
        "Документ и форматирование\nВыделите часть текста слева и нажмите B, I или U. Справа отображается страница с форматированием.\nРусский текст · العربية · 日本語 · 👩‍💻\nТаблица ниже редактируется по ячейкам кнопками перехода между абзацами.",
    )?;
    if let RichBlock::Paragraph(p) = &mut document.blocks[0] {
        let end = p.plain_text().len();
        p.format(
            0..end,
            RunStyle {
                size: 26.0,
                bold: true,
                color: [0, 90, 180],
                ..Default::default()
            },
        )?;
        p.space_after = 20.0;
    }
    document.blocks.push(RichBlock::Table(RichTable {
        columns: vec![330.0, 200.0],
        rows: vec![
            vec![Paragraph::text("Компонент"), Paragraph::text("Количество")],
            vec![Paragraph::text("Редактор"), Paragraph::text("1")],
            vec![Paragraph::text("Страница"), Paragraph::text("1")],
        ],
    }));
    document.blocks.push(RichBlock::Paragraph(Paragraph::text("Code открывает JSON документа. Apply проверяет весь документ перед заменой. Undo возвращает текст, стили и выделение вместе.")));
    document.validate()?;
    Ok(document)
}
