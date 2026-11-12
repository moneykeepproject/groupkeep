// Hide the console window on Windows release builds.
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use groupkeep_app::{controller, AppWindow};
use slint::ComponentHandle;

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let ctl = controller::Controller::setup(&ui);
    // Open a file passed on the command line (e.g. double-clicked .groupkeep).
    if let Some(path) = std::env::args().nth(1) {
        ctl.open_path(std::path::PathBuf::from(path));
    }
    ui.run()
}
