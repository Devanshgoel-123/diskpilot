//! Work DiskPilot does on its own while it runs (in the window or the menu
//! bar): rescanning when results are old, and a weekly reminder. Both are off
//! until the person turns them on, and neither ever removes anything.
//!
//! The decisions are pure functions of the settings, the clock and the last
//! scan, so they are tested without waiting for real time to pass.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::history::CleanupActivity;
use crate::model::{ActionStatus, Recommendation, Tier};

const HOUR_MS: i64 = 60 * 60 * 1000;
const DAY_MS: i64 = 24 * HOUR_MS;

/// How often to rescan on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Rescan {
    #[default]
    Off,
    Daily,
    Weekly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Schedule {
    pub rescan: Rescan,
    /// A notification once a week with what is likely safe to clear.
    pub reminder: bool,
    /// When the last reminder was shown.
    pub last_reminder_ms: Option<i64>,
}

fn file(data_dir: &Path) -> PathBuf {
    data_dir.join("schedule.json")
}

/// The saved settings, or the defaults (everything off).
pub fn load(data_dir: &Path) -> Schedule {
    std::fs::read(file(data_dir))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(data_dir: &Path, s: &Schedule) -> std::io::Result<()> {
    let tmp = data_dir.join("schedule.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(s)?)?;
    std::fs::rename(tmp, file(data_dir))
}

// ---- Local time ----------------------------------------------------------------

/// A moment in local time: the local day number (days since 1970-01-01 in
/// local time), the weekday (0 = Sunday) and the hour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Local {
    pub day: i64,
    pub weekday: u32,
    pub hour: u32,
}

/// `ms` in this computer's time zone.
pub fn local(ms: i64) -> Local {
    let (offset_secs, weekday, hour) = crate::platform::local_time(ms.div_euclid(1000));
    Local {
        day: (ms + offset_secs * 1000).div_euclid(DAY_MS),
        weekday,
        hour,
    }
}

/// The local day number of the Monday that starts the week of `t`. Weeks are
/// counted in local days, so daylight saving changes do not shift them.
fn monday(t: Local) -> i64 {
    t.day - i64::from((t.weekday + 6) % 7)
}

// ---- Streaks (same rules as src/lib/cleanupStats.ts) ----------------------

/// Cleanups that count: files and Docker data that removed something.
/// Emptying the Trash is not counted again.
fn counts(a: &CleanupActivity) -> bool {
    a.kind != "trash"
        && a.status != ActionStatus::Failed
        && a.bytes_freed.saturating_add(a.bytes_trashed) > 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Streak {
    /// Weeks in a row, up to this week or last week, with a cleanup.
    pub weeks: u32,
    pub cleared_this_week: bool,
}

pub fn streak(activity: &[CleanupActivity], now: Local, local_of: impl Fn(i64) -> Local) -> Streak {
    let mut weeks: Vec<i64> = activity
        .iter()
        .filter(|a| counts(a))
        .map(|a| monday(local_of(a.finished_at_ms)))
        .collect();
    weeks.sort_unstable();
    weeks.dedup();
    let this_week = monday(now);
    let Some(&last) = weeks.last() else {
        return Streak {
            weeks: 0,
            cleared_this_week: false,
        };
    };
    let mut run = 1;
    for pair in weeks.windows(2).rev() {
        if pair[1] - pair[0] == 7 {
            run += 1;
        } else {
            break;
        }
    }
    let alive = this_week - last <= 7;
    Streak {
        weeks: if alive { run } else { 0 },
        cleared_this_week: last == this_week,
    }
}

// ---- Decisions -------------------------------------------------------------

/// Whether to start a rescan now: turned on, nothing running, and the last
/// results older than the chosen interval (or no results at all).
pub fn rescan_due(
    s: &Schedule,
    last_scan_finished_ms: Option<i64>,
    scanning: bool,
    now_ms: i64,
) -> bool {
    let every = match s.rescan {
        Rescan::Off => return false,
        Rescan::Daily => DAY_MS,
        Rescan::Weekly => 7 * DAY_MS,
    };
    // A little slack, so a scan that finished at 10:02 yesterday is due at 10:00.
    !scanning && last_scan_finished_ms.is_none_or(|t| now_ms - t >= every - HOUR_MS)
}

/// Whether to show the weekly reminder now: turned on, at least six and a
/// half days since the last one, and during the day (10:00 to 20:00), so it
/// never arrives at night.
pub fn reminder_due(s: &Schedule, now_ms: i64, now: Local) -> bool {
    s.reminder
        && (10..20).contains(&now.hour)
        && s.last_reminder_ms
            .is_none_or(|t| now_ms - t >= 6 * DAY_MS + 12 * HOUR_MS)
}

/// Bytes of likely-safe recommendations that can be acted on, as the
/// Overview's "Likely recoverable" counts them.
pub fn likely_safe_bytes(recs: &[Recommendation]) -> u64 {
    recs.iter()
        .filter(|r| r.tier == Tier::LikelySafe && !r.methods.is_empty())
        .map(|r| r.estimated_reclaim_bytes)
        .sum()
}

/// Below this, a reminder has nothing worth saying.
pub const MIN_REMINDER_BYTES: u64 = 500_000_000;

/// The reminder's title and body, or `None` when there is nothing worth a
/// notification.
pub fn reminder_text(likely_safe: u64, streak: Streak) -> Option<(String, String)> {
    if likely_safe < MIN_REMINDER_BYTES {
        return None;
    }
    let space = format!("{} is likely safe to clear.", format_bytes(likely_safe));
    let body = match (streak.weeks, streak.cleared_this_week) {
        (0, _) => format!("{space} Review it in DiskPilot."),
        (n, true) => format!(
            "{space} You already cleaned up this week: {n} {} in a row.",
            weeks(n)
        ),
        (n, false) => format!("{space} Clean up this week to keep your {n}-week streak."),
    };
    Some(("Time for a quick cleanup".into(), body))
}

fn weeks(n: u32) -> &'static str {
    if n == 1 {
        "week"
    } else {
        "weeks"
    }
}

/// Decimal units, like Finder and the app's own formatBytes.
fn format_bytes(b: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut unit = "B";
    for u in UNITS {
        if v < 1000.0 {
            break;
        }
        v /= 1000.0;
        unit = u;
    }
    if unit == "B" {
        format!("{b} B")
    } else {
        format!("{v:.1} {unit}")
    }
}

// ---- Running it --------------------------------------------------------------

/// The argument "Open at login" starts DiskPilot with, to stay in the menu bar.
pub const STARTED_AT_LOGIN: &str = "--hidden";

/// First check a minute after launch, then every ten minutes.
const FIRST_CHECK: std::time::Duration = std::time::Duration::from_secs(60);
const EVERY: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// Start the background checks. They run while DiskPilot runs, in the window
/// or in the menu bar.
pub fn spawn(app: tauri::AppHandle) {
    let _ = std::thread::Builder::new()
        .name("diskpilot-schedule".into())
        .spawn(move || {
            std::thread::sleep(FIRST_CHECK);
            loop {
                tick(&app);
                std::thread::sleep(EVERY);
            }
        });
}

fn tick(app: &tauri::AppHandle) {
    use tauri::Manager;
    let state = app.state::<crate::state::AppState>();
    let Some(data_dir) = state.data_dir.clone() else {
        return;
    };
    let mut s = load(&data_dir);
    let now_ms = crate::scanner::now_ms();

    let (last_finished, scanning, root) = {
        let slot = state.slot();
        let last = slot.last.as_ref();
        (
            last.map(|o| o.summary.finished_at_ms),
            slot.running.is_some(),
            last.map(|o| o.summary.root.clone()),
        )
    };
    if rescan_due(&s, last_finished, scanning, now_ms) {
        // The same read-only scan as the Rescan button, of the same folder.
        if let Err(e) = crate::commands::start_scan(app.clone(), state.clone(), root, None) {
            eprintln!("diskpilot: scheduled rescan did not start: {e}");
        }
    }

    if reminder_due(&s, now_ms, local(now_ms)) {
        if let Some((title, body)) = current_reminder(&state, now_ms) {
            if notify(app, &title, &body) {
                s.last_reminder_ms = Some(now_ms);
                let _ = save(&data_dir, &s);
            }
        }
    }
}

/// The reminder for the latest results, if there is something worth saying.
pub fn current_reminder(state: &crate::state::AppState, now_ms: i64) -> Option<(String, String)> {
    let recs = state.slot().all_recommendations();
    let activity = state
        .history
        .as_ref()
        .and_then(|h| h.cleanup_activity().ok())
        .unwrap_or_default();
    reminder_text(
        likely_safe_bytes(&recs),
        streak(&activity, local(now_ms), local),
    )
}

pub fn notify(app: &tauri::AppHandle, title: &str, body: &str) -> bool {
    use tauri_plugin_notification::NotificationExt;
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|e| eprintln!("diskpilot: notification not shown: {e}"))
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A local moment on an exact day (days since 1970-01-01) and weekday.
    fn at(day: i64, weekday: u32, hour: u32) -> Local {
        Local { day, weekday, hour }
    }

    /// Test activity whose `finished_at_ms` holds a local day number; the
    /// matching `local_of` turns it back into a Local, with 1970-01-01 a
    /// Thursday (weekday 4).
    fn act(day: i64) -> CleanupActivity {
        CleanupActivity {
            kind: "files".into(),
            finished_at_ms: day,
            status: ActionStatus::Succeeded,
            bytes_freed: 0,
            bytes_trashed: 1_000_000_000,
        }
    }
    fn local_of(day: i64) -> Local {
        at(day, u32::try_from((day + 4).rem_euclid(7)).unwrap(), 12)
    }

    // 2026-11-02 is a Monday: day 20759.
    const MON: i64 = 20_759;

    #[test]
    fn local_day_and_weekday_match_the_calendar() {
        // 2026-11-06 12:00 UTC is a Friday everywhere from UTC-11 to UTC+11.
        let t = local(1_793_966_400_000);
        assert_eq!(t.weekday, 5);
        assert_eq!(t.day, MON + 4);
        assert_eq!(local_of(MON).weekday, 1);
    }

    #[test]
    fn streaks_match_the_app() {
        let now = local_of(MON + 4); // Friday
        assert_eq!(
            streak(&[], now, local_of),
            Streak {
                weeks: 0,
                cleared_this_week: false
            }
        );
        // Three weeks in a row ending last week (a Wednesday, a Monday and a
        // Sunday): still alive this week.
        let three = [act(MON - 21 + 2), act(MON - 14), act(MON - 7 + 6)];
        assert_eq!(
            streak(&three, now, local_of),
            Streak {
                weeks: 3,
                cleared_this_week: false
            }
        );
        let mut four = three.to_vec();
        four.push(act(MON + 2));
        assert_eq!(
            streak(&four, now, local_of),
            Streak {
                weeks: 4,
                cleared_this_week: true
            }
        );
        // A gap of a whole week ends it.
        assert_eq!(streak(&[act(MON - 14)], now, local_of).weeks, 0);
        // Emptying the Trash and failures do not count.
        let mut trash = act(MON);
        trash.kind = "trash".into();
        let mut failed = act(MON);
        failed.status = ActionStatus::Failed;
        assert_eq!(streak(&[trash, failed], now, local_of).weeks, 0);
    }

    #[test]
    fn sunday_night_belongs_to_the_week_before() {
        let sunday = local_of(MON + 6);
        let next_monday = local_of(MON + 7);
        assert_eq!(monday(sunday), MON);
        assert_eq!(monday(next_monday), MON + 7);
        let s = streak(&[act(MON + 6), act(MON + 7)], next_monday, local_of);
        assert_eq!(
            s,
            Streak {
                weeks: 2,
                cleared_this_week: true
            }
        );
    }

    #[test]
    fn rescans_run_only_when_results_are_old() {
        let now = 100 * DAY_MS;
        let daily = Schedule {
            rescan: Rescan::Daily,
            ..Default::default()
        };
        assert!(!rescan_due(&Schedule::default(), None, false, now));
        assert!(rescan_due(&daily, None, false, now));
        assert!(rescan_due(&daily, Some(now - DAY_MS), false, now));
        assert!(rescan_due(
            &daily,
            Some(now - DAY_MS + 30 * 60 * 1000),
            false,
            now
        ));
        assert!(!rescan_due(&daily, Some(now - 3 * HOUR_MS), false, now));
        assert!(!rescan_due(&daily, None, true, now), "never while scanning");
        let weekly = Schedule {
            rescan: Rescan::Weekly,
            ..Default::default()
        };
        assert!(!rescan_due(&weekly, Some(now - 3 * DAY_MS), false, now));
        assert!(rescan_due(&weekly, Some(now - 7 * DAY_MS), false, now));
    }

    #[test]
    fn reminders_come_weekly_and_only_by_day() {
        let on = Schedule {
            reminder: true,
            ..Default::default()
        };
        let now = 100 * DAY_MS;
        assert!(!reminder_due(&Schedule::default(), now, at(100, 1, 11)));
        assert!(reminder_due(&on, now, at(100, 1, 11)));
        assert!(!reminder_due(&on, now, at(100, 1, 9)), "not before 10:00");
        assert!(!reminder_due(&on, now, at(100, 1, 20)), "not from 20:00");
        let shown = Schedule {
            last_reminder_ms: Some(now - 3 * DAY_MS),
            ..on.clone()
        };
        assert!(!reminder_due(&shown, now, at(100, 1, 11)));
        let week_ago = Schedule {
            last_reminder_ms: Some(now - 7 * DAY_MS),
            ..on
        };
        assert!(reminder_due(&week_ago, now, at(100, 1, 11)));
    }

    #[test]
    fn reminder_says_space_and_streak() {
        let s = |weeks, cleared| Streak {
            weeks,
            cleared_this_week: cleared,
        };
        assert_eq!(reminder_text(100_000_000, s(3, false)), None);
        let (_, body) = reminder_text(23_200_000_000, s(6, false)).unwrap();
        assert_eq!(
            body,
            "23.2 GB is likely safe to clear. Clean up this week to keep your 6-week streak."
        );
        let (_, body) = reminder_text(1_500_000_000, s(1, true)).unwrap();
        assert_eq!(
            body,
            "1.5 GB is likely safe to clear. You already cleaned up this week: 1 week in a row."
        );
        let (_, body) = reminder_text(800_000_000, s(0, false)).unwrap();
        assert_eq!(
            body,
            "800.0 MB is likely safe to clear. Review it in DiskPilot."
        );
    }

    #[test]
    fn counts_only_likely_safe_items_that_can_be_removed() {
        use crate::model::CleanupMethod;
        let rec = |tier, methods: Vec<CleanupMethod>, bytes| Recommendation {
            tier,
            methods,
            estimated_reclaim_bytes: bytes,
            ..crate::test_support::recommendation()
        };
        let recs = [
            rec(Tier::LikelySafe, vec![CleanupMethod::Trash], 10),
            rec(Tier::LikelySafe, vec![], 100),
            rec(Tier::ReviewFirst, vec![CleanupMethod::Trash], 1000),
        ];
        assert_eq!(likely_safe_bytes(&recs), 10);
    }

    #[test]
    fn settings_round_trip_and_default_to_off() {
        let dir = crate::test_support::sandbox();
        assert_eq!(load(dir.path()), Schedule::default());
        let s = Schedule {
            rescan: Rescan::Weekly,
            reminder: true,
            last_reminder_ms: Some(42),
        };
        save(dir.path(), &s).unwrap();
        assert_eq!(load(dir.path()), s);
        std::fs::write(dir.path().join("schedule.json"), "not json").unwrap();
        assert_eq!(load(dir.path()), Schedule::default());
    }
}
