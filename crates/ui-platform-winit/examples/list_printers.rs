//! Enumerate configured printers. No printer is opened and no job is submitted.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for name in rust_desktop_ui_platform_winit::printing::printers()? {
        println!("{name}");
    }
    Ok(())
}
