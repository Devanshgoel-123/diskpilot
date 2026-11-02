//! DiskPilot engine.
//!
//! Module boundaries mirror the safety architecture:
//! scanner (read-only) -> classifier (pure) -> recommend -> cleanup::planner ->
//! cleanup::executor, with `safety` as the single gate for every path that is
//! read from the UI or removed. Docker changes go through `docker::actions`.

pub mod classifier;
pub mod cleanup;
pub mod commands;
pub mod devtools;
pub mod disk;
pub mod docker;
pub mod error;
pub mod explorer;
pub mod history;
pub mod model;
pub mod recommend;
pub mod safety;
pub mod scanner;
pub mod state;
#[cfg(test)]
pub mod test_support;

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
                plans: std::sync::Mutex::new(std::collections::HashMap::new()),
                history: history.map(std::sync::Arc::new),
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
            commands::get_recommendations,
            commands::create_cleanup_plan,
            commands::discard_cleanup_plan,
            commands::execute_cleanup_plan,
            commands::get_cleanup_history,
            commands::analyze_dev_tools,
            commands::get_dev_tools_report,
            commands::docker_overview,
            commands::docker_preview,
            commands::docker_execute,
            commands::docker_inspect,
            commands::docker_diagnostics,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DiskPilot");
}
