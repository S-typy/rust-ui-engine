//! A small, explicit XAML profile compiled to a Rust window definition.
//!
//! This profile supports one `Window` containing one `TextBox`. Elements use
//! [`UI_NAMESPACE`]; `x:Name` uses [`XAML_NAMESPACE`]. It does not load WPF types,
//! execute markup extensions, resolve external entities, or implement bindings.
//! Unknown syntax is an error instead of being silently ignored.
//!
//! ```
//! let window = rust_desktop_ui_xaml::parse(
//!     r#"<Window xmlns="urn:rust-ui-engine:ui" Title="Example">
//!          <TextBox Text="Hello" PlaceholderText="Enter text" />
//!        </Window>"#,
//! ).unwrap();
//! assert_eq!(window.edit_box.text, "Hello");
//! ```

use std::{fmt, fs, io::Read, path::Path};

use roxmltree::{Attribute, Document, Node, ParsingOptions};
use unicode_segmentation::UnicodeSegmentation;

/// Namespace for this library's controls; not the WPF presentation namespace.
pub const UI_NAMESPACE: &str = "urn:rust-ui-engine:ui";
/// XAML language namespace. Only the `Name` directive is supported.
pub const XAML_NAMESPACE: &str = "http://schemas.microsoft.com/winfx/2006/xaml";
/// Maximum UTF-8 source length accepted by the compiler, including comments.
pub const MAX_SOURCE_BYTES: usize = 1_048_576;
/// Largest supported size or spacing value, in logical pixels.
pub const MAX_DIMENSION: f64 = 16_384.0;
/// Largest supported editor font size, in logical pixels.
pub const MAX_FONT_SIZE: f64 = 512.0;
/// Largest `MaxLength` value accepted by this profile, in extended graphemes.
pub const MAX_TEXT_LENGTH: usize = 1_048_576;
/// Maximum UTF-8 length of an editor's validation message.
pub const MAX_VALIDATION_MESSAGE_BYTES: usize = 4_096;

/// Initial form appearance, independent of the platform and control renderer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

/// Validated startup properties for a native form.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowDefinition {
    pub title: String,
    pub width: f64,
    pub height: f64,
    /// Left, top, right, bottom padding in logical pixels.
    pub padding: [f32; 4],
    pub theme: Theme,
    pub edit_box: EditBoxDefinition,
}

impl Default for WindowDefinition {
    fn default() -> Self {
        Self {
            title: "Rust UI Form".into(),
            width: 640.0,
            height: 360.0,
            padding: [32.0; 4],
            theme: Theme::Light,
            edit_box: EditBoxDefinition::default(),
        }
    }
}

/// Startup properties for the form's single text editor.
#[derive(Clone, Debug, PartialEq)]
pub struct EditBoxDefinition {
    pub name: Option<String>,
    pub text: String,
    pub placeholder: String,
    /// Accessible name, supplied by the `AutomationName` attribute.
    pub label: String,
    /// `None` stretches to the available form content width.
    pub width: Option<f32>,
    pub height: f32,
    /// Left, top, right, bottom margin in logical pixels.
    pub margin: [f32; 4],
    pub is_enabled: bool,
    /// Allows focus, selection and copy while preventing user text mutations.
    pub is_read_only: bool,
    /// Maximum extended graphemes. `None` means no grapheme limit (`MaxLength="0"`).
    pub max_length: Option<usize>,
    /// Require a non-whitespace value when the draft is accepted.
    pub is_required: bool,
    /// Message shown after a failed required-value check.
    pub validation_message: String,
    /// Attempt to accept the draft when keyboard/window focus is lost.
    pub commit_on_lost_focus: bool,
    /// `None` uses the current control theme's metric.
    pub font_size: Option<f32>,
    /// Optional left, top, right, bottom content padding in logical pixels.
    pub padding: Option<[f32; 4]>,
    /// Uniform corner radius in logical pixels, clamped to the rendered bounds.
    pub corner_radius: Option<f32>,
    /// Uniform border thickness in logical pixels.
    pub border_thickness: Option<f32>,
    /// Optional straight-alpha sRGB colors, in red/green/blue/alpha byte order.
    pub background: Option<[u8; 4]>,
    pub foreground: Option<[u8; 4]>,
    pub border_brush: Option<[u8; 4]>,
    pub focus_brush: Option<[u8; 4]>,
    pub placeholder_foreground: Option<[u8; 4]>,
    pub selection_brush: Option<[u8; 4]>,
    /// Validation border and focus-accent color; `None` follows the theme.
    pub validation_brush: Option<[u8; 4]>,
}

impl Default for EditBoxDefinition {
    fn default() -> Self {
        Self {
            name: None,
            text: String::new(),
            placeholder: String::new(),
            label: "Text".into(),
            width: None,
            height: 36.0,
            margin: [0.0; 4],
            is_enabled: true,
            is_read_only: false,
            max_length: None,
            is_required: false,
            validation_message: "A value is required.".into(),
            commit_on_lost_focus: true,
            font_size: None,
            padding: None,
            corner_radius: None,
            border_thickness: None,
            background: None,
            foreground: None,
            border_brush: None,
            focus_brush: None,
            placeholder_foreground: None,
            selection_brush: None,
            validation_brush: None,
        }
    }
}

/// A source diagnostic with one-based line and column numbers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub line: u32,
    pub column: u32,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for Diagnostic {}

/// Parse and validate the supported form profile.
///
/// Dimensions must be finite and in `(0, MAX_DIMENSION]`; spacing must be in
/// `[0, MAX_DIMENSION]`. Padding and margin accept one value, horizontal/vertical
/// values, or left/top/right/bottom values separated by commas. Names match
/// `[A-Za-z_][A-Za-z0-9_]*`. Namespace aliases may be chosen freely.
pub fn parse(source: &str) -> Result<WindowDefinition, Diagnostic> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(Diagnostic {
            line: 1,
            column: 1,
            message: format!("source exceeds the {MAX_SOURCE_BYTES}-byte limit"),
        });
    }
    let document = Document::parse_with_options(
        source,
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: 128,
            ..ParsingOptions::default()
        },
    )
    .map_err(|error| Diagnostic {
        line: error.pos().row,
        column: error.pos().col,
        message: format!("invalid XML: {error}"),
    })?;
    if let Some(node) = document.descendants().find(Node::is_pi) {
        return Err(at_node(node, "processing instructions are not supported"));
    }
    let window = document.root_element();
    require_element(window, "Window")?;
    let mut definition = WindowDefinition::default();
    for attribute in window.attributes() {
        reject_extension(&document, attribute)?;
        if attribute.namespace().is_some() {
            return Err(at_attribute(
                &document,
                attribute,
                "Window does not support namespaced properties or directives",
            ));
        }
        match attribute.name() {
            "Title" => definition.title = attribute.value().into(),
            "Width" => definition.width = dimension(&document, attribute)?,
            "Height" => definition.height = dimension(&document, attribute)?,
            "Padding" => definition.padding = thickness(&document, attribute)?,
            "Theme" => {
                definition.theme = match attribute.value() {
                    "Light" => Theme::Light,
                    "Dark" => Theme::Dark,
                    _ => {
                        return Err(at_attribute(
                            &document,
                            attribute,
                            "Theme must be Light or Dark",
                        ));
                    }
                };
            }
            property => {
                return Err(at_attribute(
                    &document,
                    attribute,
                    format!("unsupported Window property '{property}'"),
                ));
            }
        }
    }
    let mut editor = None;
    for child in window.children() {
        if child.is_element() {
            if editor.is_some() {
                return Err(at_node(child, "Window must contain exactly one TextBox"));
            }
            editor = Some(parse_editor(child)?);
        } else {
            require_empty_content(child)?;
        }
    }
    definition.edit_box =
        editor.ok_or_else(|| at_node(window, "Window must contain exactly one TextBox"))?;
    Ok(definition)
}

fn parse_editor(node: Node<'_, '_>) -> Result<EditBoxDefinition, Diagnostic> {
    require_element(node, "TextBox")?;
    for child in node.children() {
        if child.is_element() {
            return Err(at_node(child, "TextBox cannot contain child elements"));
        }
        require_empty_content(child)?;
    }
    let document = node.document();
    let mut definition = EditBoxDefinition::default();
    for attribute in node.attributes() {
        reject_extension(document, attribute)?;
        if attribute.namespace() == Some(XAML_NAMESPACE) && attribute.name() == "Name" {
            let name = attribute.value();
            let mut chars = name.chars();
            if !chars
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                return Err(at_attribute(
                    document,
                    attribute,
                    "x:Name must match [A-Za-z_][A-Za-z0-9_]*",
                ));
            }
            definition.name = Some(name.into());
            continue;
        }
        if attribute.namespace().is_some() {
            return Err(at_attribute(
                document,
                attribute,
                "unsupported TextBox namespace or directive; only x:Name is supported",
            ));
        }
        match attribute.name() {
            "Text" => definition.text = attribute.value().into(),
            "PlaceholderText" => definition.placeholder = attribute.value().into(),
            "AutomationName" => definition.label = attribute.value().into(),
            "Width" => definition.width = Some(editor_dimension(document, attribute)?),
            "Height" => definition.height = editor_dimension(document, attribute)?,
            "Margin" => definition.margin = thickness(document, attribute)?,
            "IsEnabled" => definition.is_enabled = boolean(document, attribute)?,
            "IsReadOnly" => definition.is_read_only = boolean(document, attribute)?,
            "MaxLength" => definition.max_length = max_length(document, attribute)?,
            "IsRequired" => definition.is_required = boolean(document, attribute)?,
            "CommitOnLostFocus" => definition.commit_on_lost_focus = boolean(document, attribute)?,
            "ValidationMessage" => {
                if attribute.value().len() > MAX_VALIDATION_MESSAGE_BYTES {
                    return Err(at_attribute(
                        document,
                        attribute,
                        format!(
                            "ValidationMessage exceeds the {MAX_VALIDATION_MESSAGE_BYTES}-byte UTF-8 limit"
                        ),
                    ));
                }
                definition.validation_message = attribute.value().into();
            }
            "FontSize" => {
                definition.font_size =
                    Some(editor_metric(document, attribute, MAX_FONT_SIZE, false)?);
            }
            "Padding" => definition.padding = Some(thickness(document, attribute)?),
            "CornerRadius" => {
                definition.corner_radius =
                    Some(editor_metric(document, attribute, MAX_DIMENSION, true)?);
            }
            "BorderThickness" => {
                definition.border_thickness =
                    Some(editor_metric(document, attribute, MAX_DIMENSION, true)?);
            }
            "Background" => definition.background = Some(color(document, attribute)?),
            "Foreground" => definition.foreground = Some(color(document, attribute)?),
            "BorderBrush" => definition.border_brush = Some(color(document, attribute)?),
            "FocusBrush" => definition.focus_brush = Some(color(document, attribute)?),
            "PlaceholderForeground" => {
                definition.placeholder_foreground = Some(color(document, attribute)?)
            }
            "SelectionBrush" => definition.selection_brush = Some(color(document, attribute)?),
            "ValidationBrush" => definition.validation_brush = Some(color(document, attribute)?),
            property => {
                return Err(at_attribute(
                    document,
                    attribute,
                    format!("unsupported TextBox property '{property}'"),
                ));
            }
        }
    }
    // Match the single-line control's policy before checking the initial value.
    // Preserve the source text in the definition; mounting normalizes it too.
    if let Some(limit) = definition.max_length {
        let text: String = definition
            .text
            .chars()
            .filter(|c| !matches!(c, '\r' | '\n' | '\t' | '\u{85}' | '\u{2028}' | '\u{2029}'))
            .collect();
        if text.graphemes(true).count() > limit {
            return Err(at_attribute(
                document,
                node.attribute_node("Text")
                    .expect("nonempty Text has an attribute"),
                "Text exceeds MaxLength after single-line normalization",
            ));
        }
    }
    Ok(definition)
}

fn boolean(document: &Document<'_>, attribute: Attribute<'_, '_>) -> Result<bool, Diagnostic> {
    match attribute.value() {
        "True" | "true" => Ok(true),
        "False" | "false" => Ok(false),
        _ => Err(at_attribute(
            document,
            attribute,
            format!("{} must be True or False", attribute.name()),
        )),
    }
}

fn max_length(
    document: &Document<'_>,
    attribute: Attribute<'_, '_>,
) -> Result<Option<usize>, Diagnostic> {
    let value = attribute.value();
    let parsed = (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.parse::<usize>().ok())
        .flatten()
        .filter(|length| *length <= MAX_TEXT_LENGTH)
        .ok_or_else(|| {
            at_attribute(
                document,
                attribute,
                format!("MaxLength must be an integer in [0, {MAX_TEXT_LENGTH}]"),
            )
        })?;
    Ok((parsed != 0).then_some(parsed))
}

fn require_element(node: Node<'_, '_>, name: &str) -> Result<(), Diagnostic> {
    if node.tag_name().namespace() != Some(UI_NAMESPACE) || node.tag_name().name() != name {
        return Err(at_node(
            node,
            format!("expected '{name}' in namespace '{UI_NAMESPACE}'"),
        ));
    }
    Ok(())
}

fn require_empty_content(node: Node<'_, '_>) -> Result<(), Diagnostic> {
    if node.is_text() && !node.text().unwrap_or_default().trim().is_empty() {
        return Err(at_node(
            node,
            "text content is not supported; use the Text property",
        ));
    }
    Ok(())
}

fn reject_extension(
    document: &Document<'_>,
    attribute: Attribute<'_, '_>,
) -> Result<(), Diagnostic> {
    if attribute.value().trim_start().starts_with('{') {
        return Err(at_attribute(
            document,
            attribute,
            "markup extensions and bindings are not supported in this profile",
        ));
    }
    Ok(())
}

fn dimension(document: &Document<'_>, attribute: Attribute<'_, '_>) -> Result<f64, Diagnostic> {
    attribute
        .value()
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.0 && *value <= MAX_DIMENSION)
        .ok_or_else(|| {
            at_attribute(
                document,
                attribute,
                format!(
                    "{} must be a finite number in (0, {MAX_DIMENSION}]",
                    attribute.name()
                ),
            )
        })
}

fn editor_dimension(
    document: &Document<'_>,
    attribute: Attribute<'_, '_>,
) -> Result<f32, Diagnostic> {
    let value = dimension(document, attribute)? as f32;
    if value == 0.0 {
        return Err(at_attribute(
            document,
            attribute,
            format!(
                "{} is too small for the editor's logical pixel precision",
                attribute.name()
            ),
        ));
    }
    Ok(value)
}

fn editor_metric(
    document: &Document<'_>,
    attribute: Attribute<'_, '_>,
    maximum: f64,
    allow_zero: bool,
) -> Result<f32, Diagnostic> {
    attribute
        .value()
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| {
            value.is_finite()
                && *value <= maximum
                && if allow_zero { *value >= 0.0 } else { *value > 0.0 && (*value as f32) > 0.0 }
        })
        .map(|value| value as f32)
        .ok_or_else(|| {
            at_attribute(
                document,
                attribute,
                format!(
                    "{} must be a finite number in {}0, {maximum}] representable as an editor metric",
                    attribute.name(),
                    if allow_zero { "[" } else { "(" }
                ),
            )
        })
}

fn color(document: &Document<'_>, attribute: Attribute<'_, '_>) -> Result<[u8; 4], Diagnostic> {
    let value = attribute.value();
    let digits = value
        .strip_prefix('#')
        .filter(|digits| {
            matches!(digits.len(), 6 | 8) && digits.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        .ok_or_else(|| {
            at_attribute(
                document,
                attribute,
                format!("{} must be #RRGGBB or #AARRGGBB", attribute.name()),
            )
        })?;
    // ASCII-only validated hex makes each byte pair a valid UTF-8 slice.
    let byte = |offset| u8::from_str_radix(&digits[offset..offset + 2], 16).expect("validated hex");
    let (offset, alpha) = if digits.len() == 8 {
        (2, byte(0))
    } else {
        (0, 255)
    };
    Ok([byte(offset), byte(offset + 2), byte(offset + 4), alpha])
}

fn thickness(
    document: &Document<'_>,
    attribute: Attribute<'_, '_>,
) -> Result<[f32; 4], Diagnostic> {
    let error = || {
        at_attribute(
            document,
            attribute,
            format!(
                "{} needs 1, 2, or 4 comma-separated finite numbers in [0, {MAX_DIMENSION}]",
                attribute.name()
            ),
        )
    };
    let mut values = [0.0; 4];
    let mut count = 0;
    for item in attribute.value().split(',') {
        let value = item.trim().parse::<f64>().map_err(|_| error())?;
        if count == 4 || !value.is_finite() || !(0.0..=MAX_DIMENSION).contains(&value) {
            return Err(error());
        }
        values[count] = value as f32;
        count += 1;
    }
    match count {
        1 => Ok([values[0]; 4]),
        2 => Ok([values[0], values[1], values[0], values[1]]),
        4 => Ok(values),
        _ => Err(error()),
    }
}

fn at_node(node: Node<'_, '_>, message: impl Into<String>) -> Diagnostic {
    at_offset(node.document(), node.range().start, message)
}

fn at_attribute(
    document: &Document<'_>,
    attribute: Attribute<'_, '_>,
    message: impl Into<String>,
) -> Diagnostic {
    at_offset(document, attribute.range().start, message)
}

fn at_offset(document: &Document<'_>, offset: usize, message: impl Into<String>) -> Diagnostic {
    let position = document.text_pos_at(offset);
    Diagnostic {
        line: position.row,
        column: position.col,
        message: message.into(),
    }
}

/// Compile XAML into a Rust `window_definition()` function for `include!`.
///
/// The generated code references `rust_desktop_ui_xaml`, which must also be a
/// regular dependency of the consuming crate. All user strings are emitted as
/// escaped Rust literals; XAML never supplies executable Rust expressions.
pub fn compile(source: &str) -> Result<String, Diagnostic> {
    let window = parse(source)?;
    let edit = &window.edit_box;
    let name = match &edit.name {
        Some(name) => format!("Some({name:?}.into())"),
        None => "None".into(),
    };
    let width = match edit.width {
        Some(width) => format!("Some({width:?})"),
        None => "None".into(),
    };
    Ok(format!(
        "\
pub fn window_definition() -> rust_desktop_ui_xaml::WindowDefinition {{
    rust_desktop_ui_xaml::WindowDefinition {{
        title: {title:?}.into(),
        width: {window_width:?},
        height: {window_height:?},
        padding: {padding:?},
        theme: rust_desktop_ui_xaml::Theme::{theme:?},
        edit_box: rust_desktop_ui_xaml::EditBoxDefinition {{
            name: {name},
            text: {text:?}.into(),
            placeholder: {placeholder:?}.into(),
            label: {label:?}.into(),
            width: {width},
            height: {height:?},
            margin: {margin:?},
            is_enabled: {enabled},
            is_read_only: {read_only},
            max_length: {max_length:?},
            is_required: {is_required},
            validation_message: {validation_message:?}.into(),
            commit_on_lost_focus: {commit_on_lost_focus},
            font_size: {font_size:?},
            padding: {editor_padding:?},
            corner_radius: {corner_radius:?},
            border_thickness: {border_thickness:?},
            background: {background:?},
            foreground: {foreground:?},
            border_brush: {border_brush:?},
            focus_brush: {focus_brush:?},
            placeholder_foreground: {placeholder_foreground:?},
            selection_brush: {selection_brush:?},
            validation_brush: {validation_brush:?},
        }},
    }}
}}
",
        title = window.title,
        window_width = window.width,
        window_height = window.height,
        padding = window.padding,
        theme = window.theme,
        text = edit.text,
        placeholder = edit.placeholder,
        label = edit.label,
        height = edit.height,
        margin = edit.margin,
        enabled = edit.is_enabled,
        read_only = edit.is_read_only,
        max_length = edit.max_length,
        is_required = edit.is_required,
        validation_message = &edit.validation_message,
        commit_on_lost_focus = edit.commit_on_lost_focus,
        font_size = edit.font_size,
        editor_padding = edit.padding,
        corner_radius = edit.corner_radius,
        border_thickness = edit.border_thickness,
        background = edit.background,
        foreground = edit.foreground,
        border_brush = edit.border_brush,
        focus_brush = edit.focus_brush,
        placeholder_foreground = edit.placeholder_foreground,
        selection_brush = edit.selection_brush,
        validation_brush = edit.validation_brush,
    ))
}

/// A file build error. Parse errors display `path:line:column: message`.
#[derive(Debug)]
pub struct BuildError {
    pub path: std::path::PathBuf,
    pub diagnostic: Option<Diagnostic>,
    io_error: Option<std::io::Error>,
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:", self.path.display())?;
        if let Some(diagnostic) = &self.diagnostic {
            write!(formatter, "{diagnostic}")
        } else if let Some(error) = &self.io_error {
            write!(formatter, " {error}")
        } else {
            write!(formatter, " compilation failed")
        }
    }
}

impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.diagnostic
            .as_ref()
            .map(|value| value as &dyn std::error::Error)
            .or_else(|| {
                self.io_error
                    .as_ref()
                    .map(|value| value as &dyn std::error::Error)
            })
    }
}

/// Compile a UTF-8 XAML file into an existing output directory.
///
/// Intended for `build.rs`; callers should emit their own
/// `cargo:rerun-if-changed=...` directive and pass an output path inside
/// `OUT_DIR`. Reading is bounded even if the source file is too large.
pub fn compile_file(input: impl AsRef<Path>, output: impl AsRef<Path>) -> Result<(), BuildError> {
    let input = input.as_ref();
    let output = output.as_ref();
    let read_error = |error| BuildError {
        path: input.to_owned(),
        diagnostic: None,
        io_error: Some(error),
    };
    let mut source = String::new();
    fs::File::open(input)
        .map_err(read_error)?
        .take(MAX_SOURCE_BYTES as u64 + 1)
        .read_to_string(&mut source)
        .map_err(read_error)?;
    let generated = compile(&source).map_err(|diagnostic| BuildError {
        path: input.to_owned(),
        diagnostic: Some(diagnostic),
        io_error: None,
    })?;
    fs::write(output, generated).map_err(|error| BuildError {
        path: output.to_owned(),
        diagnostic: None,
        io_error: Some(error),
    })
}

#[cfg(test)]
mod tests;
