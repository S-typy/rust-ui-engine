//! Offline component gallery and application-owned virtual book data source.
mod accessibility;
mod books;
mod gallery;
mod ribbon_model;
#[cfg(test)]
mod tests;

use rust_desktop_ui_platform_winit::{RunOptions, run};
use std::{path::PathBuf, process::ExitCode};

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
    let mut rows = 100_000;
    let mut state_file = PathBuf::from("target/gallery-columns.txt");
    let mut options = RunOptions::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--rows" => {
                rows = args
                    .next()
                    .ok_or("--rows requires a count")?
                    .parse()
                    .map_err(|_| "Invalid row count")?
            }
            "--state-file" => {
                state_file = PathBuf::from(args.next().ok_or("--state-file requires a path")?)
            }
            "--smoke-test" => options.smoke_test = true,
            "--help" | "-h" => {
                println!("controls-gallery [--rows 0..1000000] [--state-file PATH] [--smoke-test]");
                return Ok(());
            }
            _ => return Err(format!("Unknown argument: {arg}")),
        }
    }
    let app = gallery::Gallery::new(rows, state_file)?;
    run(app, options)
}
