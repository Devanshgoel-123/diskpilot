//! Menu bar item: free space on the startup disk at a glance, and shortcuts
//! into the app. It only reads the disk's capacity; every action opens the
//! window, where the usual preview and confirmation apply.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::disk;

/// Asks the window to open a page or the Empty Trash window.
pub const EVENT_TRAY_NAVIGATE: &str = "tray://navigate";
const REFRESH: Duration = Duration::from_secs(30);

/// Decimal units, like Finder and the rest of the app.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() {
        value /= 1000.0;
        unit += 1;
    }
    let digits = if value >= 100.0 { 0 } else { 1 };
    format!("{value:.digits$} {}", UNITS[unit - 1])
}

fn labels(home: &Path) -> (String, String) {
    match disk::disk_info(home) {
        Ok(d) => (
            format_bytes(d.available_bytes),
            format!(
                "{} available of {}",
                format_bytes(d.available_bytes),
                format_bytes(d.total_bytes)
            ),
        ),
        Err(_) => ("—".into(), "Disk capacity unavailable".into()),
    }
}

pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn install<R: Runtime>(app: &AppHandle<R>, home: PathBuf) -> tauri::Result<()> {
    let (title, detail) = labels(&home);
    let capacity = MenuItem::with_id(app, "capacity", detail, false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open DiskPilot", true, None::<&str>)?;
    let cleanup = MenuItem::with_id(
        app,
        "recommendations",
        "Review Cleanup…",
        true,
        None::<&str>,
    )?;
    let duplicates = MenuItem::with_id(app, "duplicates", "Find Duplicates…", true, None::<&str>)?;
    let trash = MenuItem::with_id(app, "trash", "Empty Trash…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit DiskPilot", true, Some("CmdOrCtrl+Q"))?;
    let menu = Menu::with_items(
        app,
        &[
            &capacity,
            &PredefinedMenuItem::separator(app)?,
            &open,
            &cleanup,
            &duplicates,
            &trash,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let tray = TrayIconBuilder::with_id("diskpilot")
        .icon(Image::from_bytes(include_bytes!(
            "../icons/tray-template.png"
        ))?)
        .icon_as_template(true)
        .title(&title)
        .tooltip("DiskPilot: available space")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main_window(app),
            "quit" => app.exit(0),
            page @ ("recommendations" | "duplicates" | "trash") => {
                show_main_window(app);
                let _ = app.emit(EVENT_TRAY_NAVIGATE, page);
            }
            _ => {}
        })
        .build(app)?;

    // statfs is a single cheap system call; refresh it in the background.
    std::thread::spawn(move || loop {
        std::thread::sleep(REFRESH);
        let (title, detail) = labels(&home);
        if tray.set_title(Some(&title)).is_err() {
            break; // The app is shutting down.
        }
        let _ = capacity.set_text(detail);
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_the_interface() {
        assert_eq!(format_bytes(999), "999 B");
        assert_eq!(format_bytes(1_500), "1.5 KB");
        assert_eq!(format_bytes(115_400_000_000), "115 GB");
        assert_eq!(format_bytes(23_160_000_000), "23.2 GB");
    }
}
