//! A native form whose structure and initial properties are compiled from XAML.
mod accessibility;
#[cfg(test)]
mod binding_adapter_tests;
mod docking_accessibility;
mod docking_app;
mod form;
mod form_chrome;
mod forms_model;
#[cfg(test)]
mod forms_tests;
#[cfg(test)]
mod group_adapter_tests;
#[cfg(test)]
mod layout_adapter_tests;
mod popup_diagnostics;
#[cfg(test)]
mod ribbon_tests;
#[cfg(test)]
mod tests;
mod view;
mod view_ribbon;

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
    let mut docking = false;
    let mut float_pane = None;
    let mut layout_file = None;
    let mut smoke_test = false;
    let mut diagnostics = false;
    let mut ribbon_minimized = false;
    let mut native_title_bar = false;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--dark" => theme = Some(Theme::Dark),
            "--light" => theme = Some(Theme::Light),
            "--controls" => controls = true,
            "--editors" => editors = true,
            "--groups" => groups = true,
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
            "--help" | "-h" => {
                println!(
                    "xaml-form [--controls | --editors | --groups | --docking [--float-pane ID] [--layout-file PATH]] [--ribbon-minimized] [--native-title-bar] [--light | --dark] [--smoke-test] [--diagnostics]"
                );
                println!(
                    "--controls opens ControlsWindow.xaml; the default is MainWindow.xaml with Ribbon."
                );
                println!(
                    "--editors opens EditorsWindow.xaml with selection, typed values and data bindings."
                );
                println!(
                    "--groups opens GroupsWindow.xaml with GroupBox, Expander and TabbedGroup containers."
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
    if [controls, docking, editors, groups]
        .into_iter()
        .filter(|v| *v)
        .count()
        > 1
    {
        return Err("--controls, --editors, --groups and --docking are mutually exclusive".into());
    }
    if float_pane.is_some() && !docking {
        return Err("--float-pane requires --docking".into());
    }
    if layout_file.is_some() && !docking {
        return Err("--layout-file requires --docking".into());
    }
    let mut definition = if docking {
        generated_docking::window_definition()
    } else if editors {
        generated_editors::window_definition()
    } else if groups {
        generated_groups::window_definition()
    } else if controls {
        generated_controls::window_definition()
    } else {
        generated::window_definition()
    };
    if let Some(theme) = theme {
        definition.theme = theme;
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
    let app = if editors || groups {
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
