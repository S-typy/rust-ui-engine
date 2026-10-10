fn main() {
    println!("cargo:rerun-if-changed=MainWindow.xaml");
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo OUT_DIR"))
        .join("main_window.rs");
    if let Err(error) = rust_desktop_ui_xaml::compile_file("MainWindow.xaml", &output) {
        panic!("{error}");
    }
}
