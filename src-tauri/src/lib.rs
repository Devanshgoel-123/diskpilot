//! DiskPilot engine.
//!
//! Module boundaries mirror the safety architecture:
//! scanner (read-only) -> classifier (pure) -> recommend -> cleanup::planner ->
//! cleanup::executor, with `safety` as the single gate for every path that is
//! read from the UI or removed. Docker changes go through `docker::actions`.

pub mod adjust;
pub mod apps;
pub mod assistant;
pub mod classifier;
pub mod cleanup;
pub mod commands;
pub mod devtools;
pub mod disk;
pub mod docker;
pub mod duplicates;
pub mod error;
pub mod explorer;
pub mod filekinds;
pub mod history;
pub mod model;
pub mod preview;
pub mod privacy;
pub mod recommend;
pub mod safety;
pub mod scanner;
pub mod schedule;
pub mod secrets;
pub mod snapshot;
pub mod state;
#[cfg(test)]
pub mod test_support;
pub mod trash;
pub mod tray;
pub mod tray_icon;

use tauri::Manager;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        // "Open at login" starts DiskPilot in the menu bar, without a window.
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![schedule::STARTED_AT_LOGIN]),
        ))
        .setup(|app| {
            let home = dirs::home_dir().ok_or("Could not determine the home folder")?;
            let data_dir = app
                .path()
                .app_data_dir()
                .ok()
                .filter(|dir| std::fs::create_dir_all(dir).is_ok());
            let history = data_dir.as_ref().and_then(|dir| {
                history::History::open(&dir.join("diskpilot.sqlite3"))
                    .map_err(|e| eprintln!("diskpilot: history unavailable: {e}"))
                    .ok()
            });
            tray::install(app.handle(), home.clone())?;
            if std::env::args().any(|a| a == schedule::STARTED_AT_LOGIN) {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            app.manage(AppState {
                home,
                scan: std::sync::Mutex::default(),
                restore: state::Restore::default(),
                plans: std::sync::Mutex::new(std::collections::HashMap::new()),
                history: history.map(std::sync::Arc::new),
                assistant: std::sync::Mutex::new(assistant::Conversation::default()),
                data_dir,
                trash: std::sync::Mutex::default(),
                duplicates: std::sync::Mutex::default(),
                docker_reviews: std::sync::Mutex::default(),
            });
            // Results from the previous launch, shown until the next scan. They
            // load in the background (about 150 ms for a home folder) so the
            // window opens without waiting; reading them waits instead.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let state = handle.state::<AppState>();
                let restored = state
                    .data_dir
                    .as_deref()
                    .and_then(snapshot::load)
                    .map(|outcome| commands::restored_slot(&state.home, outcome));
                if let Some(slot) = restored {
                    *state.scan.lock().unwrap_or_else(|e| e.into_inner()) = slot;
                }
                state.restore.finish();
            });
            schedule::spawn(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_environment,
            commands::get_disk_info,
            commands::get_schedule,
            commands::set_schedule,
            commands::send_test_reminder,
            commands::start_scan,
            commands::cancel_scan,
            commands::is_scan_running,
            commands::get_scan_summary,
            commands::get_file_kind_files,
            commands::get_saved_scan_info,
            commands::clear_saved_scan,
            commands::list_directory,
            commands::get_item_details,
            commands::reveal_in_finder,
            commands::open_owner_app,
            commands::file_thumbnail,
            commands::open_listed_file,
            commands::get_scan_history,
            commands::get_app_badges,
            commands::get_app_storage,
            commands::get_recommendations,
            commands::create_cleanup_plan,
            commands::discard_cleanup_plan,
            commands::execute_cleanup_plan,
            commands::get_cleanup_history,
            commands::get_cleanup_activity,
            commands::get_trash_summary,
            commands::preview_trash,
            commands::discard_trash_preview,
            commands::empty_trash,
            commands::cancel_empty_trash,
            commands::open_full_disk_access_settings,
            commands::get_full_disk_access,
            commands::check_ai_transcripts,
            commands::find_duplicates,
            commands::cancel_find_duplicates,
            commands::get_duplicates,
            commands::plan_duplicate_cleanup,
            commands::plan_file_kind_cleanup,
            commands::analyze_dev_tools,
            commands::get_dev_tools_report,
            commands::assistant_status,
            commands::assistant_set_key,
            commands::assistant_clear_key,
            commands::assistant_models,
            commands::assistant_ask,
            commands::assistant_reset,
            commands::docker_overview,
            commands::docker_preview,
            commands::docker_execute,
            commands::docker_inspect,
            commands::docker_diagnostics,
        ])
        // The menu bar item keeps running when the window is closed; it, the
        // Dock icon and Quit bring the window back or end the app.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building DiskPilot")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                tray::show_main_window(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}
