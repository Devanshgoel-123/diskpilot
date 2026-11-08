//! Menu bar item (the notification area icon on Windows): free space on the
//! startup disk at a glance, and shortcuts into the app. It only reads the
//! disk's capacity; every action opens the window, where the usual preview
//! and confirmation apply.
//!
//! On Windows a left click opens the window and a right click shows the
//! menu, as tray icons do there. The tray has no title, so the free space is
//! in the tooltip.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::{disk, tray_icon};

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

/// Menu bar title, menu detail line, and the share of the disk in use.
fn labels(home: &Path) -> (String, String, f64) {
    match disk::disk_info(home) {
        Ok(d) => (
            format_bytes(d.available_bytes),
            format!(
                "{} available of {}",
                format_bytes(d.available_bytes),
                format_bytes(d.total_bytes)
            ),
            used_share(d.available_bytes, d.total_bytes),
        ),
        Err(_) => ("—".into(), "Disk capacity unavailable".into(), 0.0),
    }
}

/// Share of the disk that is not available, matching the "available" figure.
fn used_share(available: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    1.0 - available.min(total) as f64 / total as f64
}

/// Whole percent steps, so the icon is redrawn only when it would change.
fn icon_step(used: f64) -> u32 {
    (used * 100.0).round() as u32
}

#[cfg(not(windows))]
fn gauge(used: f64) -> Image<'static> {
    Image::new_owned(tray_icon::render(used), tray_icon::SIZE, tray_icon::SIZE)
}

/// Windows does not tint tray icons: white on a dark taskbar, near-black on
/// a light one. Checked on every refresh, so a theme change is followed.
#[cfg(windows)]
fn gauge(used: f64) -> Image<'static> {
    let rgb = if crate::winshell::light_taskbar() {
        [0x1c, 0x1c, 0x1c]
    } else {
        [0xff, 0xff, 0xff]
    };
    Image::new_owned(
        tray_icon::render_in(used, rgb),
        tray_icon::SIZE,
        tray_icon::SIZE,
    )
}

/// What the icon looks like, so it is redrawn only when that changes.
fn icon_key(used: f64) -> (u32, bool) {
    #[cfg(windows)]
    let light = crate::winshell::light_taskbar();
    #[cfg(not(windows))]
    let light = false;
    (icon_step(used), light)
}

fn tooltip(detail: &str) -> String {
    if cfg!(windows) {
        format!("DiskPilot: {detail}")
    } else {
        "DiskPilot: available space".into()
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
    let (title, detail, used) = labels(&home);
    let capacity = MenuItem::with_id(app, "capacity", &detail, false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open DiskPilot", true, None::<&str>)?;
    let cleanup = MenuItem::with_id(
        app,
        "recommendations",
        "Review Cleanup…",
        true,
        None::<&str>,
    )?;
    let duplicates = MenuItem::with_id(app, "duplicates", "Find Duplicates…", true, None::<&str>)?;
    let empty = if cfg!(windows) {
        "Empty Recycle Bin…"
    } else {
        "Empty Trash…"
    };
    let trash = MenuItem::with_id(app, "trash", empty, true, None::<&str>)?;
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
        .icon(gauge(used))
        .icon_as_template(true)
        .title(&title)
        .tooltip(tooltip(&detail))
        .menu(&menu)
        .show_menu_on_left_click(cfg!(not(windows)))
        .on_tray_icon_event(|tray, event| {
            // Windows: a left click opens the window; the menu is on the right button.
            if cfg!(windows) {
                if let tauri::tray::TrayIconEvent::Click {
                    button: tauri::tray::MouseButton::Left,
                    button_state: tauri::tray::MouseButtonState::Up,
                    ..
                } = event
                {
                    show_main_window(tray.app_handle());
                }
            }
        })
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
    let mut shown = icon_key(used);
    std::thread::spawn(move || loop {
        std::thread::sleep(REFRESH);
        let (title, detail, used) = labels(&home);
        if tray.set_title(Some(&title)).is_err() {
            break; // The app is shutting down.
        }
        if cfg!(windows) {
            let _ = tray.set_tooltip(Some(tooltip(&detail)));
        }
        let _ = capacity.set_text(detail);
        if icon_key(used) != shown {
            shown = icon_key(used);
            let _ = tray.set_icon(Some(gauge(used)));
            let _ = tray.set_icon_as_template(true);
        }
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

    #[test]
    fn used_share_follows_available_space() {
        assert_eq!(used_share(25, 100), 0.75);
        assert_eq!(used_share(0, 0), 0.0);
        assert_eq!(used_share(200, 100), 0.0);
        assert_eq!(icon_step(0.734), 73);
    }
}
