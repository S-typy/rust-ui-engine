//! A native form whose structure and initial properties are compiled from XAML.
mod accessibility;
#[cfg(test)]
mod batch_binding_tests;
mod batch_diagnostics;
mod batch_model;
#[cfg(test)]
mod binding_adapter_tests;
#[cfg(test)]
mod data_adapter_tests;
mod data_model;
#[cfg(test)]
mod density_adapter_tests;
mod docking_accessibility;
mod docking_app;
mod form;
mod form_chrome;
mod forms_model;
#[cfg(test)]
mod forms_tests;
#[cfg(test)]
mod foundation_accessibility_tests;
#[cfg(test)]
mod group_adapter_tests;
#[cfg(test)]
mod layout_adapter_tests;
mod popup_diagnostics;
#[cfg(test)]
mod ribbon_tests;
#[cfg(test)]
mod status_adapter_tests;
#[cfg(test)]
mod tests;
mod view;
mod view_ribbon;
mod visual_model;
#[cfg(test)]
mod visual_tests;

mod generated_visuals {
    include!(concat!(env!("OUT_DIR"), "/visuals_window.rs"));
}
mod generated_batch {
    include!(concat!(env!("OUT_DIR"), "/batch_window.rs"));
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
    let mut controls = false;
    let mut editors = false;
    let mut groups = false;
    let mut status = false;
    let mut data = false;
    let mut visuals = false;
    let mut batch = false;
    let mut density = None;
    let mut recovery = false;
    let mut docking = false;
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
            "--recovery-smoke" => {
                recovery = true;
                smoke_test = true;
            }
            "--controls" => controls = true,
            "--editors" => editors = true,
            "--groups" => groups = true,
            "--status" => status = true,
            "--data" => data = true,
            "--docking" => docking = true,
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
        controls, docking, editors, groups, status, data, visuals, batch,
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
    if layout_file.is_some() && !docking {
        return Err("--layout-file requires --docking".into());
    }
    let mut definition = if batch {
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
        let path = layout_file
            .map(Ok)
            .unwrap_or_else(docking_app::storage::default_path)?;
        app.set_layout_file(path);
        return rust_desktop_ui_platform_winit::run_multi(app);
    }
    let app = if batch {
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
