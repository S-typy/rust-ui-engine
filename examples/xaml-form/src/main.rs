//! A native form whose structure and initial properties are compiled from XAML.
mod form;
#[cfg(test)]
mod tests;

mod generated {
    include!(concat!(env!("OUT_DIR"), "/main_window.rs"));
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
    let mut definition = generated::window_definition();
    let mut smoke_test = false;
    let mut diagnostics = false;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--dark" => definition.theme = Theme::Dark,
            "--light" => definition.theme = Theme::Light,
            "--smoke-test" => smoke_test = true,
            "--diagnostics" => diagnostics = true,
            "--help" | "-h" => {
                println!("xaml-form [--light | --dark] [--smoke-test] [--diagnostics]");
                println!("--diagnostics logs editor values for automated input tests.");
                return Ok(());
            }
            _ => return Err(format!("Unknown argument: {argument}")),
        }
    }
    let options = RunOptions {
        width: definition.width,
        height: definition.height,
        min_size: None,
        show_render_stats: diagnostics,
        smoke_test,
    };
    let editor_name = definition.edit_box.name.clone();
    let app = form::Form::new(definition, diagnostics)?;
    if diagnostics && let Some(name) = editor_name {
        println!(
            "named-editor={name} resolved={}",
            app.find_name(&name).is_some()
        );
    }
    run(app, options)
}
