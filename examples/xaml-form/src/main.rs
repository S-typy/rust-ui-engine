//! A native form whose structure and initial properties are compiled from XAML.
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
mod grid_workspace_model;
#[cfg(test)]
mod group_adapter_tests;
#[cfg(test)]
mod layout_adapter_tests;
mod navigation_diagnostics;
mod navigation_model;
mod popup_diagnostics;
mod rating_model;
#[cfg(test)]
mod ribbon_tests;
mod ribbon_workspace_model;
mod search_model;
mod selection_model;
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

use rust_desktop_ui_platform_winit::{RunOptions, run};
use rust_desktop_ui_xaml::Theme;
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
    let mut badges = false;
    let mut breadcrumb = false;
    let mut font = false;
    let mut wizard = false;
    let mut barcode = false;
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
            "--badges" => badges = true,
            "--breadcrumb" => breadcrumb = true,
            "--font" => font = true,
            "--wizard" => wizard = true,
            "--barcodes" => barcode = true,
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
                    "--gallery opens grouped visual samples, preview/acceptance and retained Ribbon/popup hosts."
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
        badges,
        breadcrumb,
        font,
        wizard,
        barcode,
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
    if dialog_directory.is_some() && !workspace {
        return Err("--dialog-directory requires --workspace".into());
    }
    if layout_file.is_some() && !docking {
        return Err("--layout-file requires --docking".into());
    }
    let mut definition = if notifications {
        generated_notifications::window_definition()
    } else if documents {
        generated_documents::window_definition()
    } else if gallery {
        generated_gallery::window_definition()
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
    if docking {
        let mut app = docking_app::DockApplication::new(
            definition,
            options,
            diagnostics,
            float_pane.as_deref(),
        )?;
        if documents {
            app.enable_documents()?;
        } else {
            let path = layout_file
                .map(Ok)
                .unwrap_or_else(docking_app::storage::default_path)?;
            app.set_layout_file(path);
        }
        return rust_desktop_ui_platform_winit::run_multi(app);
    }
    if notifications {
        return rust_desktop_ui_platform_winit::run_multi(
            notifications_app::NotificationsApplication::new(definition, options, diagnostics)?,
        );
    }
    if workspace {
        let mut application =
            workspace_app::WorkspaceApplication::new(definition, options, diagnostics)?;
        application.set_dialog_directory(dialog_directory);
        return rust_desktop_ui_platform_winit::run_multi(application);
    }
    let app = if gallery {
        form::Form::new_with_model(definition, diagnostics, Some(gallery_model::model()?))?
    } else if wizard {
        form::Form::new_with_model(definition, diagnostics, Some(wizard_model::model()?))?
    } else if navigation {
        form::Form::new_with_model(definition, diagnostics, Some(navigation_model::model()?))?
    } else if breadcrumb {
        form::Form::new_with_model(definition, diagnostics, Some(breadcrumb_model::model()?))?
    } else if grid_workspace {
        form::Form::new_with_model(
            definition,
            diagnostics,
            Some(grid_workspace_model::model()?),
        )?
    } else if data_form {
        let (context, sources) = data_form_model::model()?;
        form::Form::new_with_sources(definition, diagnostics, Some(context), sources)?
    } else if tree {
        form::Form::new_with_model(definition, diagnostics, Some(tree_model::model()?))?
    } else if badges {
        form::Form::new_with_model(definition, diagnostics, Some(badge_model::model()?))?
    } else if font {
        form::Form::new_with_model(definition, diagnostics, Some(font_model::model()?))?
    } else if tiles {
        form::Form::new_with_model(definition, diagnostics, Some(tile_model::model()?))?
    } else if color {
        form::Form::new_with_model(definition, diagnostics, Some(color_model::model()?))?
    } else if rating {
        form::Form::new_with_model(definition, diagnostics, Some(rating_model::model()?))?
    } else if search {
        form::Form::new_with_model(definition, diagnostics, Some(search_model::model()?))?
    } else if selection {
        form::Form::new_with_model(definition, diagnostics, Some(selection_model::model()?))?
    } else if ribbon_workspace {
        form::Form::new_with_model(
            definition,
            diagnostics,
            Some(ribbon_workspace_model::model()?),
        )?
    } else if data_editors {
        let (context, sources) = data_editors_model::model()?;
        form::Form::new_with_sources(definition, diagnostics, Some(context), sources)?
    } else if batch {
        form::Form::new_with_model(definition, diagnostics, Some(batch_model::model()?))?
    } else if data {
        let (context, sources) = data_model::model()?;
        form::Form::new_with_sources(definition, diagnostics, Some(context), sources)?
    } else if editors || groups || status {
        form::Form::new_with_model(definition, diagnostics, Some(forms_model::model()?))?
    } else {
        form::Form::new(definition, diagnostics)?
    };
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
