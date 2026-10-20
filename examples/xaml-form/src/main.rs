//! A native form whose structure and initial properties are compiled from XAML.
mod composition_app;
mod output_app;
mod generated_output {
    include!(concat!(env!("OUT_DIR"), "/output_window.rs"));
}
mod image_editor_app;
mod planning_app;
mod rich_document_app;
mod rich_document_model;
mod spatial_app;
mod generated_rich_document {
    include!(concat!(env!("OUT_DIR"), "/rich_document_window.rs"));
}
mod generated_spatial {
    include!(concat!(env!("OUT_DIR"), "/spatial_window.rs"));
}
mod generated_planning {
    include!(concat!(env!("OUT_DIR"), "/planning_window.rs"));
}
mod generated_image_editor {
    include!(concat!(env!("OUT_DIR"), "/image_editor_window.rs"));
}
mod charts_model;
mod pivot_model;
mod generated_charts {
    include!(concat!(env!("OUT_DIR"), "/charts_window.rs"));
}
mod query_model;
mod generated_pivot {
    include!(concat!(env!("OUT_DIR"), "/pivot_window.rs"));
}
mod generated_query {
    include!(concat!(env!("OUT_DIR"), "/query_window.rs"));
}
mod notifications_app;
mod generated_notifications {
    include!(concat!(env!("OUT_DIR"), "/notifications_window.rs"));
}
mod accessibility;
mod badge_model;
mod barcode_model;
#[cfg(test)]
mod batch_binding_tests;
mod batch_diagnostics;
mod batch_model;
#[cfg(test)]
mod binding_adapter_tests;
mod breadcrumb_model;
mod color_model;
#[cfg(test)]
mod data_adapter_tests;
mod data_editors_model;
mod data_form_model;
mod data_model;
#[cfg(test)]
mod density_adapter_tests;
mod docking_accessibility;
mod docking_app;
mod font_model;
mod form;
mod form_chrome;
mod forms_model;
#[cfg(test)]
mod forms_tests;
#[cfg(test)]
mod foundation_accessibility_tests;
mod gallery_diagnostics;
mod gallery_model;
mod gauge_model;
mod grid_workspace_model;
#[cfg(test)]
mod group_adapter_tests;
#[cfg(test)]
mod layout_adapter_tests;
mod navigation_diagnostics;
mod navigation_model;
mod popup_diagnostics;
mod property_model;
mod rating_model;
#[cfg(test)]
mod ribbon_tests;
mod ribbon_workspace_model;
mod search_model;
mod selection_model;
mod spell_model;
#[cfg(test)]
mod status_adapter_tests;
#[cfg(test)]
mod template_adapter_tests;
#[cfg(test)]
mod tests;
mod tile_model;
mod tree_model;
mod view;
mod view_ribbon;
mod visual_model;
#[cfg(test)]
mod visual_tests;
mod wizard_model;
mod workspace_app;
mod workspace_model;

mod generated_visuals {
    include!(concat!(env!("OUT_DIR"), "/visuals_window.rs"));
}
mod generated_batch {
    include!(concat!(env!("OUT_DIR"), "/batch_window.rs"));
}
mod generated_workspace {
    include!(concat!(env!("OUT_DIR"), "/workspace_window.rs"));
}
mod generated_dialog {
    include!(concat!(env!("OUT_DIR"), "/dialog_window.rs"));
}

mod generated {
    include!(concat!(env!("OUT_DIR"), "/main_window.rs"));
}
mod generated_controls {
    include!(concat!(env!("OUT_DIR"), "/controls_window.rs"));
}
mod generated_editors {
    include!(concat!(env!("OUT_DIR"), "/editors_window.rs"));
}
mod generated_groups {
    include!(concat!(env!("OUT_DIR"), "/groups_window.rs"));
}
mod generated_status {
    include!(concat!(env!("OUT_DIR"), "/status_window.rs"));
}
mod generated_data {
    include!(concat!(env!("OUT_DIR"), "/data_window.rs"));
}
mod generated_data_editors {
    include!(concat!(env!("OUT_DIR"), "/data_editors_window.rs"));
}
mod generated_ribbon_workspace {
    include!(concat!(env!("OUT_DIR"), "/ribbon_workspace_window.rs"));
}
mod generated_selection {
    include!(concat!(env!("OUT_DIR"), "/selection_window.rs"));
}
mod generated_breadcrumb {
    include!(concat!(env!("OUT_DIR"), "/breadcrumb_window.rs"));
}
mod generated_badges {
    include!(concat!(env!("OUT_DIR"), "/badge_window.rs"));
}
mod generated_font {
    include!(concat!(env!("OUT_DIR"), "/font_window.rs"));
}
mod generated_navigation {
    include!(concat!(env!("OUT_DIR"), "/navigation_window.rs"));
}
mod generated_properties {
    include!(concat!(env!("OUT_DIR"), "/property_window.rs"));
}
mod generated_gallery {
    include!(concat!(env!("OUT_DIR"), "/gallery_window.rs"));
}
mod generated_tiles {
    include!(concat!(env!("OUT_DIR"), "/tile_window.rs"));
}
mod generated_color {
    include!(concat!(env!("OUT_DIR"), "/color_window.rs"));
}
mod generated_rating {
    include!(concat!(env!("OUT_DIR"), "/rating_window.rs"));
}
mod generated_spell {
    include!(concat!(env!("OUT_DIR"), "/spell_window.rs"));
}
mod generated_gauge {
    include!(concat!(env!("OUT_DIR"), "/gauge_window.rs"));
}
mod generated_barcode {
    include!(concat!(env!("OUT_DIR"), "/barcode_window.rs"));
}
mod generated_wizard {
    include!(concat!(env!("OUT_DIR"), "/wizard_window.rs"));
}
mod generated_search {
    include!(concat!(env!("OUT_DIR"), "/search_window.rs"));
}
mod generated_data_form {
    include!(concat!(env!("OUT_DIR"), "/data_form_window.rs"));
}
mod generated_grid_workspace {
    include!(concat!(env!("OUT_DIR"), "/grid_workspace_window.rs"));
}
mod generated_tree {
    include!(concat!(env!("OUT_DIR"), "/tree_window.rs"));
}
mod generated_documents {
    include!(concat!(env!("OUT_DIR"), "/document_window.rs"));
}
mod generated_docking {
    include!(concat!(env!("OUT_DIR"), "/dock_window.rs"));
}

use rust_desktop_ui_platform_winit::{DesktopApp, RunOptions, run};
use rust_desktop_ui_xaml::{Theme, WindowDefinition};
use std::process::ExitCode;

fn main() -> ExitCode {
    match start() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn start() -> Result<(), String> {
    let mut theme = None;
    let mut image_editor = false;
    let mut charts_3d = false;
    let mut map = false;
    let mut diagram = false;
    let mut scheduler = false;
    let mut gantt = false;
    let mut rich_editor = false;
    let mut output_mode = None;
    let mut composition_mode = None;
    let mut composition_preview = false;
    let mut media_file = None;
    let mut spatial_preview = None;
    let mut planning_preview = None;
    let mut query_builder = false;
    let mut pivot = false;
    let mut charts = false;
    let mut notifications = false;
    let mut controls = false;
    let mut editors = false;
    let mut groups = false;
    let mut status = false;
    let mut data = false;
    let mut data_editors = false;
    let mut ribbon_workspace = false;
    let mut selection = false;
    let mut search = false;
    let mut rating = false;
    let mut color = false;
    let mut navigation = false;
    let mut tiles = false;
    let mut gallery = false;
    let mut properties = false;
    let mut badges = false;
    let mut breadcrumb = false;
    let mut font = false;
    let mut wizard = false;
    let mut barcode = false;
    let mut spell = false;
    let mut gauge = false;
    let mut data_form = false;
    let mut tree = false;
    let mut grid_workspace = false;
    let mut visuals = false;
    let mut batch = false;
    let mut workspace = false;
    let mut dialog_directory = None;
    let mut density = None;
    let mut recovery = false;
    let mut docking = false;
    let mut documents = false;
    let mut float_pane = None;
    let mut layout_file = None;
    let mut smoke_test = false;
    let mut diagnostics = false;
    let mut ribbon_minimized = false;
    let mut native_title_bar = false;
    let mut activate = true;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--dark" => theme = Some(Theme::Dark),
            "--light" => theme = Some(Theme::Light),
            "--high-contrast-light" => theme = Some(Theme::HighContrastLight),
            "--high-contrast-dark" => theme = Some(Theme::HighContrastDark),
            "--compact" => density = Some(rust_desktop_ui_xaml::ThemeDensity::Compact),
            "--touch" => density = Some(rust_desktop_ui_xaml::ThemeDensity::Touch),
            "--visuals" => visuals = true,
            "--batch" => batch = true,
            "--workspace" => workspace = true,
            "--dialog-directory" => {
                dialog_directory = Some(std::path::PathBuf::from(
                    arguments
                        .next()
                        .ok_or("--dialog-directory requires a path")?,
                ))
            }
            "--recovery-smoke" => {
                recovery = true;
                smoke_test = true;
            }
            "--image-editor" => image_editor = true,
            "--charts-3d" => charts_3d = true,
            "--map" => map = true,
            "--diagram" => diagram = true,
            "--scheduler" => scheduler = true,
            "--gantt" => gantt = true,
            "--rich-editor" => rich_editor = true,
            "--report-designer" | "--dashboard" | "--media" => {
                if composition_mode.is_some() {
                    return Err("Choose one composition example".into());
                }
                composition_mode = Some(match argument.as_str() {
                    "--report-designer" => composition_app::CompositionMode::ReportDesigner,
                    "--dashboard" => composition_app::CompositionMode::Dashboard,
                    _ => composition_app::CompositionMode::Media,
                });
            }
            "--composition-preview" => composition_preview = true,
            "--media-file" => {
                media_file = Some(std::path::PathBuf::from(
                    arguments.next().ok_or("--media-file requires a path")?,
                ))
            }
            "--spreadsheet" | "--pdf" | "--print-preview" | "--report" => {
                if output_mode.is_some() {
                    return Err("Choose one document example".into());
                }
                output_mode = Some(match argument.as_str() {
                    "--spreadsheet" => output_app::OutputMode::Spreadsheet,
                    "--pdf" => output_app::OutputMode::Pdf,
                    "--print-preview" => output_app::OutputMode::Preview,
                    _ => output_app::OutputMode::Report,
                });
            }
            "--planning-preview" => {
                planning_preview = Some(
                    arguments
                        .next()
                        .ok_or("--planning-preview requires a view index")?
                        .parse::<u32>()
                        .map_err(|_| "invalid planning preview index")?,
                )
            }
            "--spatial-preview" => {
                spatial_preview = Some(
                    arguments
                        .next()
                        .ok_or("--spatial-preview requires an action index")?
                        .parse::<u32>()
                        .map_err(|_| "invalid spatial preview index")?,
                )
            }
            "--query-builder" => query_builder = true,
            "--pivot" => pivot = true,
            "--charts" => charts = true,
            "--notifications" => notifications = true,
            "--controls" => controls = true,
            "--editors" => editors = true,
            "--groups" => groups = true,
            "--status" => status = true,
            "--data" => data = true,
            "--data-editors" => data_editors = true,
            "--ribbon-workspace" => ribbon_workspace = true,
            "--selection" => selection = true,
            "--search" => search = true,
            "--rating" => rating = true,
            "--color" => color = true,
            "--navigation" => navigation = true,
            "--tiles" => tiles = true,
            "--gallery" => gallery = true,
            "--properties" => properties = true,
            "--badges" => badges = true,
            "--breadcrumb" => breadcrumb = true,
            "--font" => font = true,
            "--wizard" => wizard = true,
            "--barcodes" => barcode = true,
            "--spellcheck" => spell = true,
            "--gauges" => gauge = true,
            "--data-form" => data_form = true,
            "--tree" => tree = true,
            "--grid-workspace" => grid_workspace = true,
            "--docking" => docking = true,
            "--documents" => {
                documents = true;
                docking = true;
            }
            "--float-pane" => {
                float_pane = Some(arguments.next().ok_or("--float-pane requires a pane ID")?)
            }
            "--layout-file" => {
                layout_file = Some(std::path::PathBuf::from(
                    arguments.next().ok_or("--layout-file requires a path")?,
                ));
            }
            "--smoke-test" => smoke_test = true,
            "--diagnostics" => diagnostics = true,
            "--ribbon-minimized" => ribbon_minimized = true,
            "--native-title-bar" => native_title_bar = true,
            "--no-activate" => activate = false,
            "--help" | "-h" => {
                println!(
                    "--report-designer, --dashboard and --media open composition examples. --composition-preview selects preview/designer/playback; --media-file PATH opens an explicit local video."
                );
                println!(
                    "--gallery opens grouped visual samples, preview/acceptance and retained Ribbon/popup hosts."
                );
                println!(
                    "--image-editor opens a retained image editor and accepted preview with explicit PNG load/save."
                );
                println!(
                    "--charts-3d, --map and --diagram open spatial canvases with live data tables. --spatial-preview INDEX applies an example action before display."
                );
                println!(
                    "--scheduler and --gantt open calendar and task planning. --rich-editor opens formatted documents with explicit DOCX import/export. --spreadsheet, --pdf, --print-preview and --report open the document output examples."
                );
                println!(
                    "--planning-preview INDEX selects a scheduler view before display: 0 Day, 1 Week, 2 Month, 3 Timeline. Requires --scheduler."
                );
                println!(
                    "--data-editors opens DataEditorsWindow.xaml: transactional data, MemoEdit, editor buttons and PasswordBox.\n--workspace opens WorkspaceWindow.xaml: retained tabs, dialogs, customizable status, reusable styles/templates and typed queries."
                );
                println!(
                    "--dialog-directory PATH selects the initial directory for workspace file dialogs; no files are opened or written."
                );
                println!(
                    "--batch opens BatchWindow.xaml: panels, popup content, toolbars, choices, sliders, localization and asynchronous validation."
                );
                println!(
                    "xaml-form [--controls | --editors | --groups | --status | --data | --docking [--float-pane ID] [--layout-file PATH]] [--ribbon-minimized] [--native-title-bar] [--light | --dark] [--smoke-test] [--diagnostics] [--no-activate]"
                );
                println!(
                    "--controls opens ControlsWindow.xaml; the default is MainWindow.xaml with Ribbon."
                );
                println!(
                    "--visuals opens VisualsWindow.xaml with images, complex text and themes. --high-contrast-light / --high-contrast-dark select contrast themes; --compact / --touch select density. --recovery-smoke exercises device recovery and exits."
                );
                println!(
                    "--editors opens EditorsWindow.xaml with selection, typed values and data bindings."
                );
                println!(
                    "--groups opens GroupsWindow.xaml with GroupBox, Expander and TabbedGroup containers."
                );
                println!(
                    "--status opens StatusWindow.xaml with StatusBar, RibbonStatusBar, progress and zoom bindings."
                );
                println!(
                    "--data opens DataWindow.xaml with typed Rust collection and in-memory SQLite tables."
                );
                println!(
                    "--data-editors opens the transactional source and editor examples. --ribbon-workspace opens retained Ribbon editors, Backstage and explicit QAT preferences."
                );
                println!(
                    "--selection opens large virtual lists, checked/radio rows, tokens and reference lookups."
                );
                println!(
                    "--diagnostics logs input, values and validation for automated input tests."
                );
                println!(
                    "--layout-file selects the file for explicit Save/Restore commands; no automatic loading or saving."
                );
                return Ok(());
            }
            _ => return Err(format!("Unknown argument: {argument}")),
        }
    }
    if [
        composition_mode.is_some(),
        output_mode.is_some(),
        charts_3d,
        map,
        diagram,
        scheduler,
        gantt,
        rich_editor,
        image_editor,
        pivot,
        charts,
        query_builder,
        notifications,
        controls,
        docking,
        editors,
        groups,
        status,
        data,
        visuals,
        batch,
        workspace,
        data_editors,
        ribbon_workspace,
        selection,
        search,
        rating,
        color,
        navigation,
        tiles,
        gallery,
        properties,
        badges,
        breadcrumb,
        font,
        wizard,
        barcode,
        spell,
        gauge,
        data_form,
        tree,
        grid_workspace,
    ]
    .into_iter()
    .filter(|v| *v)
    .count()
        > 1
    {
        return Err(
            "--controls, --editors, --groups, --status, --data, --visuals, --batch and --docking are mutually exclusive".into(),
        );
    }
    if float_pane.is_some() && !docking {
        return Err("--float-pane requires --docking".into());
    }
    if spatial_preview.is_some() && !(charts_3d || map || diagram) {
        return Err("--spatial-preview requires --charts-3d, --map or --diagram".into());
    }
    if planning_preview.is_some() && !scheduler {
        return Err("--planning-preview requires --scheduler".into());
    }
    if dialog_directory.is_some() && !workspace {
        return Err("--dialog-directory requires --workspace".into());
    }
    if layout_file.is_some() && !docking {
        return Err("--layout-file requires --docking".into());
    }
    if composition_preview && composition_mode.is_none() {
        return Err("--composition-preview requires a composition example".into());
    }
    if media_file.is_some()
        && !matches!(
            composition_mode,
            Some(composition_app::CompositionMode::Media)
        )
    {
        return Err("--media-file requires --media".into());
    }
    let mut definition = if output_mode.is_some() || composition_mode.is_some() {
        generated_output::window_definition()
    } else if charts_3d || map || diagram {
        generated_spatial::window_definition()
    } else if scheduler || gantt {
        generated_planning::window_definition()
    } else if rich_editor {
        generated_rich_document::window_definition()
    } else if image_editor {
        generated_image_editor::window_definition()
    } else if charts {
        generated_charts::window_definition()
    } else if pivot {
        generated_pivot::window_definition()
    } else if query_builder {
        generated_query::window_definition()
    } else if properties {
        generated_properties::window_definition()
    } else if notifications {
        generated_notifications::window_definition()
    } else if documents {
        generated_documents::window_definition()
    } else if gallery {
        generated_gallery::window_definition()
    } else if spell {
        generated_spell::window_definition()
    } else if gauge {
        generated_gauge::window_definition()
    } else if barcode {
        generated_barcode::window_definition()
    } else if wizard {
        generated_wizard::window_definition()
    } else if navigation {
        generated_navigation::window_definition()
    } else if breadcrumb {
        generated_breadcrumb::window_definition()
    } else if grid_workspace {
        generated_grid_workspace::window_definition()
    } else if data_form {
        generated_data_form::window_definition()
    } else if tree {
        generated_tree::window_definition()
    } else if badges {
        generated_badges::window_definition()
    } else if font {
        generated_font::window_definition()
    } else if tiles {
        generated_tiles::window_definition()
    } else if color {
        generated_color::window_definition()
    } else if rating {
        generated_rating::window_definition()
    } else if search {
        generated_search::window_definition()
    } else if selection {
        generated_selection::window_definition()
    } else if ribbon_workspace {
        generated_ribbon_workspace::window_definition()
    } else if data_editors {
        generated_data_editors::window_definition()
    } else if workspace {
        generated_workspace::window_definition()
    } else if batch {
        generated_batch::window_definition()
    } else if visuals {
        generated_visuals::window_definition()
    } else if docking {
        generated_docking::window_definition()
    } else if editors {
        generated_editors::window_definition()
    } else if groups {
        generated_groups::window_definition()
    } else if status {
        generated_status::window_definition()
    } else if data {
        generated_data::window_definition()
    } else if controls {
        generated_controls::window_definition()
    } else {
        generated::window_definition()
    };
    if let Some(theme) = theme {
        definition.theme = theme;
    }
    if let Some(density) = density {
        definition.density = density;
    }
    if native_title_bar {
        definition.title_bar = rust_desktop_ui_xaml::TitleBar::Native;
    }
    if ribbon_minimized {
        fn minimize(element: &mut rust_desktop_ui_xaml::ElementDefinition) {
            use rust_desktop_ui_xaml::ElementKind;
            if let ElementKind::Ribbon(ribbon) = &mut element.kind {
                ribbon.is_minimized = true;
            }
            for child in element.children_mut() {
                minimize(child);
            }
        }
        minimize(&mut definition.content);
    }
    let options = RunOptions {
        width: definition.width,
        height: definition.height,
        min_size: (definition.title_bar == rust_desktop_ui_xaml::TitleBar::Ribbon)
            .then_some(rust_desktop_ui_core::Size::new(240.0, 240.0)),
        show_render_stats: diagnostics,
        smoke_test,
        smoke_device_recovery: recovery,
        activate,
    };
    if let Some(mode) = composition_mode {
        return run_composition(
            definition,
            options,
            diagnostics,
            mode,
            composition_preview,
            media_file,
        );
    }
    if let Some(mode) = output_mode {
        return run_output(definition, options, diagnostics, mode);
    }
    if docking {
        return run_docking(
            definition,
            options,
            diagnostics,
            documents,
            float_pane,
            layout_file,
        );
    }
    if charts_3d || map || diagram {
        let kind = if charts_3d {
            spatial_app::SpatialDemoKind::Chart3D
        } else if map {
            spatial_app::SpatialDemoKind::Map
        } else {
            spatial_app::SpatialDemoKind::Diagram
        };
        return run_spatial(definition, options, diagnostics, kind, spatial_preview);
    }
    if scheduler || gantt {
        return run_planning(definition, options, diagnostics, gantt, planning_preview);
    }
    if rich_editor {
        return run_demo(
            definition,
            options,
            diagnostics,
            rich_document_app::RichDocumentDemo::new,
        );
    }
    if image_editor {
        return run_demo(
            definition,
            options,
            diagnostics,
            image_editor_app::ImageDemo::new,
        );
    }
    if charts {
        return run_demo(
            definition,
            options,
            diagnostics,
            charts_model::ChartsDemo::new,
        );
    }
    if pivot {
        return run_demo(
            definition,
            options,
            diagnostics,
            pivot_model::PivotDemo::new,
        );
    }
    if query_builder {
        return run_demo(
            definition,
            options,
            diagnostics,
            query_model::QueryDemo::new,
        );
    }
    if notifications {
        return run_notifications(definition, options, diagnostics);
    }
    if workspace {
        return run_workspace(definition, options, diagnostics, dialog_directory);
    }
    let (context, sources) = if properties {
        (None, Vec::new())
    } else if gallery {
        (Some(gallery_model::model()?), Vec::new())
    } else if wizard {
        (Some(wizard_model::model()?), Vec::new())
    } else if navigation {
        (Some(navigation_model::model()?), Vec::new())
    } else if breadcrumb {
        (Some(breadcrumb_model::model()?), Vec::new())
    } else if grid_workspace {
        (Some(grid_workspace_model::model()?), Vec::new())
    } else if data_form {
        let (context, sources) = data_form_model::model()?;
        (Some(context), sources)
    } else if tree {
        (Some(tree_model::model()?), Vec::new())
    } else if badges {
        (Some(badge_model::model()?), Vec::new())
    } else if font {
        (Some(font_model::model()?), Vec::new())
    } else if tiles {
        (Some(tile_model::model()?), Vec::new())
    } else if color {
        (Some(color_model::model()?), Vec::new())
    } else if rating {
        (Some(rating_model::model()?), Vec::new())
    } else if search {
        (Some(search_model::model()?), Vec::new())
    } else if selection {
        (Some(selection_model::model()?), Vec::new())
    } else if ribbon_workspace {
        (Some(ribbon_workspace_model::model()?), Vec::new())
    } else if data_editors {
        let (context, sources) = data_editors_model::model()?;
        (Some(context), sources)
    } else if batch {
        (Some(batch_model::model()?), Vec::new())
    } else if data {
        let (context, sources) = data_model::model()?;
        (Some(context), sources)
    } else if editors || groups || status {
        (Some(forms_model::model()?), Vec::new())
    } else {
        (None, Vec::new())
    };
    run_plain_form(definition, options, diagnostics, context, sources)
}

// Keep application construction out of the argument dispatcher. In debug builds,
// mutually exclusive branches can otherwise reserve a stack slot for every large
// application value and Result at once, before any selected branch can run.
#[inline(never)]
fn run_output(
    definition: WindowDefinition,
    options: RunOptions,
    diagnostics: bool,
    mode: output_app::OutputMode,
) -> Result<(), String> {
    run(
        output_app::OutputDemo::new(definition, diagnostics, mode)?,
        options,
    )
}

#[inline(never)]
fn run_composition(
    definition: WindowDefinition,
    options: RunOptions,
    diagnostics: bool,
    mode: composition_app::CompositionMode,
    preview: bool,
    video: Option<std::path::PathBuf>,
) -> Result<(), String> {
    run(
        composition_app::CompositionDemo::new(definition, diagnostics, mode, preview, video)?,
        options,
    )
}

#[inline(never)]
fn run_demo<A: DesktopApp + 'static>(
    definition: WindowDefinition,
    options: RunOptions,
    diagnostics: bool,
    constructor: fn(WindowDefinition, bool) -> Result<A, String>,
) -> Result<(), String> {
    let app = constructor(definition, diagnostics)?;
    run(app, options)
}

#[inline(never)]
fn run_spatial(
    definition: WindowDefinition,
    options: RunOptions,
    diagnostics: bool,
    kind: spatial_app::SpatialDemoKind,
    preview: Option<u32>,
) -> Result<(), String> {
    let mut app = spatial_app::SpatialDemo::new(definition, diagnostics, kind)?;
    if let Some(index) = preview {
        app.preview_action(index)?;
    }
    run(app, options)
}

#[inline(never)]
fn run_planning(
    definition: WindowDefinition,
    options: RunOptions,
    diagnostics: bool,
    gantt: bool,
    preview: Option<u32>,
) -> Result<(), String> {
    let mut app = planning_app::PlanningDemo::new(definition, gantt, diagnostics)?;
    if let Some(index) = preview {
        app.preview_mode(index)?;
    }
    run(app, options)
}

#[inline(never)]
fn run_docking(
    definition: WindowDefinition,
    options: RunOptions,
    diagnostics: bool,
    documents: bool,
    float_pane: Option<String>,
    layout_file: Option<std::path::PathBuf>,
) -> Result<(), String> {
    let mut app =
        docking_app::DockApplication::new(definition, options, diagnostics, float_pane.as_deref())?;
    if documents {
        app.enable_documents()?;
    } else {
        let path = layout_file
            .map(Ok)
            .unwrap_or_else(docking_app::storage::default_path)?;
        app.set_layout_file(path);
    }
    rust_desktop_ui_platform_winit::run_multi(app)
}

#[inline(never)]
fn run_notifications(
    definition: WindowDefinition,
    options: RunOptions,
    diagnostics: bool,
) -> Result<(), String> {
    let app = notifications_app::NotificationsApplication::new(definition, options, diagnostics)?;
    rust_desktop_ui_platform_winit::run_multi(app)
}

#[inline(never)]
fn run_workspace(
    definition: WindowDefinition,
    options: RunOptions,
    diagnostics: bool,
    dialog_directory: Option<std::path::PathBuf>,
) -> Result<(), String> {
    let mut app = workspace_app::WorkspaceApplication::new(definition, options, diagnostics)?;
    app.set_dialog_directory(dialog_directory);
    rust_desktop_ui_platform_winit::run_multi(app)
}

#[inline(never)]
fn run_plain_form(
    definition: WindowDefinition,
    options: RunOptions,
    diagnostics: bool,
    context: Option<std::rc::Rc<std::cell::RefCell<rust_desktop_ui_controls::BindingContext>>>,
    sources: Vec<(String, data_model::SharedSource)>,
) -> Result<(), String> {
    let app = form::Form::new_with_sources(definition, diagnostics, context, sources)?;
    if diagnostics {
        println!(
            "named-editor=Message resolved={}",
            app.find_name("Message").is_some()
        );
        for name in [
            "MainRibbon",
            "Message",
            "Option",
            "Confirm",
            "Pin",
            "Disabled",
            "AlignLeft",
            "AlignCenter",
            "AlignRight",
            "CheckIcon",
            "CloseIcon",
            "AddIcon",
            "RemoveIcon",
            "ChevronDownIcon",
            "ChevronRightIcon",
            "SaveIcon",
            "DisabledIcon",
            "VRule",
            "HRule",
            "ActionMenu",
            "AcceptSplit",
        ] {
            println!(
                "named-control={name} resolved={}",
                app.find_name(name).is_some()
            );
        }
    }
    run(app, options)
}
