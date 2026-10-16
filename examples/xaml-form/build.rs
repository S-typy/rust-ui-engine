fn main() {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    for (source, generated) in [
        ("MainWindow.xaml", "main_window.rs"),
        ("ControlsWindow.xaml", "controls_window.rs"),
        ("DockWindow.xaml", "dock_window.rs"),
        ("EditorsWindow.xaml", "editors_window.rs"),
        ("GroupsWindow.xaml", "groups_window.rs"),
        ("StatusWindow.xaml", "status_window.rs"),
        ("DataWindow.xaml", "data_window.rs"),
        ("DataEditorsWindow.xaml", "data_editors_window.rs"),
        ("RibbonWorkspaceWindow.xaml", "ribbon_workspace_window.rs"),
        ("VisualsWindow.xaml", "visuals_window.rs"),
        ("BatchWindow.xaml", "batch_window.rs"),
        ("WorkspaceWindow.xaml", "workspace_window.rs"),
        ("DialogWindow.xaml", "dialog_window.rs"),
    ] {
        println!("cargo:rerun-if-changed={source}");
        if let Err(error) = rust_desktop_ui_xaml::compile_file(source, out.join(generated)) {
            panic!("{error}");
        }
    }
}
