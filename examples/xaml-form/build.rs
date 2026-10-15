fn main() {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    for (source, generated) in [
        ("MainWindow.xaml", "main_window.rs"),
        ("ControlsWindow.xaml", "controls_window.rs"),
        ("DockWindow.xaml", "dock_window.rs"),
    ] {
        println!("cargo:rerun-if-changed={source}");
        if let Err(error) = rust_desktop_ui_xaml::compile_file(source, out.join(generated)) {
            panic!("{error}");
        }
    }
}
