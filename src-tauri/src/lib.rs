//! DiskPilot engine.
//!
//! Module boundaries mirror the safety architecture:
//! scanner (read-only) -> classifier (pure) -> [recommendations -> planner ->
//! executor, Phase 2] with `safety` as the single gate for any path that comes
//! from the UI.

pub mod classifier;
pub mod commands;
pub mod disk;
pub mod error;
pub mod explorer;
pub mod history;
pub mod model;
pub mod safety;
pub mod scanner;
pub mod state;

use tauri::Manager;

use state::{AppState, ScanSlot};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let home = dirs::home_dir().ok_or("Could not determine the home folder")?;
            let history = app.path().app_data_dir().ok().and_then(|dir| {
                std::fs::create_dir_all(&dir).ok()?;
                history::History::open(&dir.join("diskpilot.sqlite3"))
                    .map_err(|e| eprintln!("diskpilot: history unavailable: {e}"))
                    .ok()
            });
            app.manage(AppState {
                home,
                scan: std::sync::Mutex::new(ScanSlot::default()),
                history,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_environment,
            commands::get_disk_info,
            commands::start_scan,
            commands::cancel_scan,
            commands::is_scan_running,
            commands::get_scan_summary,
            commands::list_directory,
            commands::get_item_details,
            commands::reveal_in_finder,
            commands::get_scan_history,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DiskPilot");
}
