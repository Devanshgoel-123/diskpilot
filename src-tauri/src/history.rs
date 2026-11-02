//! Scan history persisted in a local SQLite database.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};

use crate::model::{ScanRecord, ScanStatus};

pub struct History {
    conn: Mutex<Connection>,
}

const MIGRATIONS: &str = "
CREATE TABLE IF NOT EXISTS scan_runs (
    id            TEXT PRIMARY KEY,
    root          TEXT NOT NULL,
    started_at_ms INTEGER NOT NULL,
    finished_at_ms INTEGER NOT NULL,
    duration_ms   INTEGER NOT NULL,
    total_bytes   INTEGER NOT NULL,
    file_count    INTEGER NOT NULL,
    dir_count     INTEGER NOT NULL,
    error_count   INTEGER NOT NULL,
    status        TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS scan_runs_started ON scan_runs(started_at_ms DESC);
";

fn to_i64(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

fn to_u64(v: i64) -> u64 {
    u64::try_from(v).unwrap_or(0)
}

impl History {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Self> {
        conn.execute_batch(MIGRATIONS)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn record(&self, r: &ScanRecord) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT OR REPLACE INTO scan_runs
             (id, root, started_at_ms, finished_at_ms, duration_ms, total_bytes, file_count, dir_count, error_count, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                r.id,
                r.root,
                r.started_at_ms,
                r.finished_at_ms,
                to_i64(r.duration_ms),
                to_i64(r.total_bytes),
                to_i64(r.file_count),
                to_i64(r.dir_count),
                to_i64(r.error_count),
                r.status.as_str(),
            ],
        )?;
        Ok(())
    }

    pub fn recent(&self, limit: u32) -> rusqlite::Result<Vec<ScanRecord>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, root, started_at_ms, finished_at_ms, duration_ms, total_bytes, file_count, dir_count, error_count, status
             FROM scan_runs ORDER BY started_at_ms DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(ScanRecord {
                id: row.get(0)?,
                root: row.get(1)?,
                started_at_ms: row.get(2)?,
                finished_at_ms: row.get(3)?,
                duration_ms: to_u64(row.get(4)?),
                total_bytes: to_u64(row.get(5)?),
                file_count: to_u64(row.get(6)?),
                dir_count: to_u64(row.get(7)?),
                error_count: to_u64(row.get(8)?),
                status: ScanStatus::parse(&row.get::<_, String>(9)?),
            })
        })?;
        rows.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, started: i64) -> ScanRecord {
        ScanRecord {
            id: id.into(),
            root: "/tmp/x".into(),
            started_at_ms: started,
            finished_at_ms: started + 10,
            duration_ms: 10,
            total_bytes: 4096,
            file_count: 3,
            dir_count: 1,
            error_count: 0,
            status: ScanStatus::Completed,
        }
    }

    #[test]
    fn records_and_lists_newest_first() {
        let h = History::in_memory().unwrap();
        h.record(&record("a", 100)).unwrap();
        h.record(&record("b", 200)).unwrap();
        let rows = h.recent(10).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["b", "a"]
        );
        assert_eq!(rows[0].status, ScanStatus::Completed);
        assert_eq!(h.recent(1).unwrap().len(), 1);
    }
}
