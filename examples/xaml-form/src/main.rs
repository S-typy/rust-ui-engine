//! A native form whose structure and initial properties are compiled from XAML.
mod accessibility;
mod form;
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
    let mut smoke_test = false;
    let mut diagnostics = false;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--dark" => theme = Some(Theme::Dark),
            "--light" => theme = Some(Theme::Light),
            "--controls" => controls = true,
            "--smoke-test" => smoke_test = true,
            "--diagnostics" => diagnostics = true,
            "--help" | "-h" => {
                println!(
                    "xaml-form [--controls] [--light | --dark] [--smoke-test] [--diagnostics]"
                );
                println!(
                    "--controls opens ControlsWindow.xaml; the default is MainWindow.xaml with Ribbon."
                );
                println!(
                    "--diagnostics logs input, values and validation for automated input tests."
                );
                return Ok(());
            }
            _ => return Err(format!("Unknown argument: {argument}")),
        }
    }
    let mut definition = if controls {
        generated_controls::window_definition()
    } else {
        generated::window_definition()
    };
    if let Some(theme) = theme {
        definition.theme = theme;
    }
    let options = RunOptions {
        width: definition.width,
        height: definition.height,
        min_size: None,
        show_render_stats: diagnostics,
        smoke_test,
    };
    let app = form::Form::new(definition, diagnostics)?;
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
