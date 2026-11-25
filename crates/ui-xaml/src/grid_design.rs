//! Build a form from a typed schema and validate the generated XAML before returning it.
use rust_desktop_ui_data::TableSchema;
use std::fmt::Write as _;
/// Generate a complete form with explicit ordered columns and read-only flags.
/// Empty fields uses schema order. DataSource is resolved on mount by the host.
pub fn design_grid_form(
    source: &str,
    schema: &TableSchema,
    fields: &[String],
) -> Result<String, String> {
    let selected: Vec<_> = if fields.is_empty() {
        schema.fields().iter().collect()
    } else {
        let mut seen = std::collections::HashSet::new();
        fields
            .iter()
            .map(|f| {
                if !seen.insert(f) {
                    return Err(format!("Duplicate field {f}"));
                }
                schema.field(f).ok_or_else(|| format!("Unknown column {f}"))
            })
            .collect::<Result<_, _>>()?
    };
    fn attr(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('"', "&quot;")
            .replace('\n', "&#10;")
            .replace('\r', "&#13;")
            .replace('\t', "&#9;")
    }
    let mut form = format!(
        r#"<Window xmlns="urn:rust-ui-engine:ui" Title="Grid"><GridControl DataSource="{}"><GridControl.Columns>"#,
        attr(source)
    );
    for f in selected {
        write!(
            form,
            r#"<GridColumn FieldName="{}" Header="{}" IsReadOnly="{}"/>"#,
            attr(&f.name),
            attr(&f.title),
            if f.read_only { "True" } else { "False" }
        )
        .unwrap();
    }
    form.push_str("</GridControl.Columns></GridControl></Window>");
    crate::compile(&form).map_err(|e| e.to_string())?;
    Ok(form)
}
