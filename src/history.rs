//! Port usage history tracking
//!
//! Stores snapshots of port activity in a SQLite database for historical analysis.

use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection};

use crate::platform;

const DB_NAME: &str = "ports_history.db";

/// Highest schema version this binary understands. Bump alongside the
/// migration ladder in `init_db`.
const CURRENT_SCHEMA_VERSION: u32 = 2;

/// Get the path to the history database
fn db_path() -> Result<PathBuf> {
    let data_dir = dirs::data_local_dir()
        .or_else(dirs::home_dir)
        .context("Could not determine data directory")?;

    let ports_dir = data_dir.join("ports");
    std::fs::create_dir_all(&ports_dir)?;

    Ok(ports_dir.join(DB_NAME))
}

/// Initialize the database schema.
///
/// Schema-version sanity check: refuses to operate on a DB whose
/// `user_version` is greater than this binary supports. This is
/// forward-only — it only fires when a *future* binary writes a
/// higher version and the current binary then reads it. Today's
/// downgrade path (newer binary → older binary on the same DB) is
/// **not** protected by this check, because the older binary
/// predates the check. Real downgrade protection lives in the
/// separate `review-schema-downgrade-handling` spec.
fn init_db(conn: &Connection) -> Result<()> {
    // Read as i64 first so a hand-mangled negative value surfaces
    // as a friendly error instead of rusqlite's opaque "invalid type".
    let version_raw: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version_raw < 0 {
        anyhow::bail!(
            "history DB has invalid user_version {version_raw}; \
             remove {} to reset.",
            db_path()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        );
    }
    let version = version_raw as u32;

    if version > CURRENT_SCHEMA_VERSION {
        anyhow::bail!(
            "history DB schema v{version} is newer than this binary supports \
             (v{CURRENT_SCHEMA_VERSION}). Upgrade portls or remove {} to reset.",
            db_path()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        );
    }

    if version < 1 {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                unix_ts INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS ports (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                snapshot_id INTEGER NOT NULL,
                port INTEGER NOT NULL,
                protocol TEXT NOT NULL,
                address TEXT NOT NULL,
                pid INTEGER,
                process_name TEXT,
                container TEXT,
                state TEXT,
                remote_addr TEXT,
                FOREIGN KEY (snapshot_id) REFERENCES snapshots(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_ports_snapshot ON ports(snapshot_id);
            CREATE INDEX IF NOT EXISTS idx_ports_port ON ports(port);
            CREATE INDEX IF NOT EXISTS idx_snapshots_unix_ts ON snapshots(unix_ts);
            ",
        )?;
        conn.execute_batch("PRAGMA user_version = 1;")?;
    }

    if version < 2 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            PRAGMA user_version = 2;",
        )?;
    }

    // Run on every open so a manually-deleted counter row is restored
    // before `increment_counter` tries to UPDATE it. Idempotent; cheap.
    conn.execute(
        "INSERT OR IGNORE INTO meta (key, value) VALUES ('auto_prune_counter', '0')",
        [],
    )?;

    Ok(())
}

/// Open a connection to the history database
pub fn open_db() -> Result<Connection> {
    let path = db_path()?;
    let conn = Connection::open(&path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    init_db(&conn)?;
    Ok(conn)
}

/// Record the current port state as a snapshot
pub fn record_snapshot(include_connections: bool) -> Result<RecordResult> {
    let conn = open_db()?;
    let now = Utc::now();

    // Get current ports. History records all observations and does not
    // surface docker_status — per spec, the user's history view is about
    // ports over time, not daemon reachability at the moment of capture.
    let mut all_ports = platform::get_listening_ports()?.ports;
    if include_connections {
        all_ports.extend(platform::get_connections()?.ports);
    }
    // Insert snapshot
    conn.execute(
        "INSERT INTO snapshots (timestamp, unix_ts) VALUES (?1, ?2)",
        params![now.to_rfc3339(), now.timestamp()],
    )?;
    let snapshot_id = conn.last_insert_rowid();

    // Insert ports
    let mut stmt = conn.prepare(
        "INSERT INTO ports (snapshot_id, port, protocol, address, pid, process_name, container, state, remote_addr)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
    )?;

    for port in &all_ports {
        // Determine state based on whether this is a listening port or connection
        let state: Option<&str> = if port.remote_address.is_some() {
            Some("ESTABLISHED")
        } else {
            Some("LISTEN")
        };

        stmt.execute(params![
            snapshot_id,
            port.port as i32,
            port.protocol.to_string(),
            port.address,
            port.pid as i32,
            port.process_name,
            port.container,
            state,
            port.remote_address,
        ])?;
    }

    Ok(RecordResult {
        snapshot_id,
        port_count: all_ports.len(),
        timestamp: now,
    })
}

pub struct RecordResult {
    pub snapshot_id: i64,
    pub port_count: usize,
    pub timestamp: DateTime<Utc>,
}

/// Query options for history
pub struct HistoryQuery {
    pub port: Option<u16>,
    pub process: Option<String>,
    pub hours: Option<i64>,
    pub limit: usize,
}

impl Default for HistoryQuery {
    fn default() -> Self {
        Self {
            port: None,
            process: None,
            hours: Some(24),
            limit: 100,
        }
    }
}

/// A historical port entry
#[derive(Debug)]
pub struct HistoryEntry {
    pub timestamp: DateTime<Utc>,
    pub port: u16,
    pub protocol: String,
    pub address: String,
    pub pid: Option<u32>,
    pub process_name: String,
    pub container: Option<String>,
    pub state: Option<String>,
}

/// Get history matching the query
pub fn get_history(query: &HistoryQuery) -> Result<Vec<HistoryEntry>> {
    let conn = open_db()?;

    let mut sql = String::from(
        "SELECT s.timestamp, p.port, p.protocol, p.address, p.pid, p.process_name, p.container, p.state
         FROM ports p
         JOIN snapshots s ON p.snapshot_id = s.id
         WHERE 1=1"
    );

    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(port) = query.port {
        sql.push_str(" AND p.port = ?");
        params_vec.push(Box::new(port as i32));
    }

    if let Some(ref process) = query.process {
        sql.push_str(" AND p.process_name LIKE ?");
        params_vec.push(Box::new(format!("%{}%", process)));
    }

    if let Some(hours) = query.hours {
        let cutoff = Utc::now() - Duration::hours(hours);
        sql.push_str(" AND s.unix_ts >= ?");
        params_vec.push(Box::new(cutoff.timestamp()));
    }

    sql.push_str(" ORDER BY s.unix_ts DESC LIMIT ?");
    params_vec.push(Box::new(query.limit as i32));

    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_refs.as_slice(), |row| {
        let ts_str: String = row.get(0)?;
        let timestamp = DateTime::parse_from_rfc3339(&ts_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        Ok(HistoryEntry {
            timestamp,
            port: row.get::<_, i32>(1)? as u16,
            protocol: row.get(2)?,
            address: row.get(3)?,
            pid: row.get::<_, Option<i32>>(4)?.map(|p| p as u32),
            process_name: row.get(5)?,
            container: row.get(6)?,
            state: row.get(7)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Get summary statistics
pub fn get_stats() -> Result<HistoryStats> {
    let conn = open_db()?;

    let snapshot_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM snapshots", [], |row| row.get(0))?;

    let total_entries: i64 = conn.query_row("SELECT COUNT(*) FROM ports", [], |row| row.get(0))?;

    let oldest: Option<String> = conn
        .query_row("SELECT MIN(timestamp) FROM snapshots", [], |row| row.get(0))
        .ok();

    let newest: Option<String> = conn
        .query_row("SELECT MAX(timestamp) FROM snapshots", [], |row| row.get(0))
        .ok();

    let unique_ports: i64 =
        conn.query_row("SELECT COUNT(DISTINCT port) FROM ports", [], |row| {
            row.get(0)
        })?;

    let db_size = db_path()?.metadata().map(|m| m.len()).unwrap_or(0);

    Ok(HistoryStats {
        snapshot_count: snapshot_count as usize,
        total_entries: total_entries as usize,
        unique_ports: unique_ports as usize,
        oldest_snapshot: oldest.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        }),
        newest_snapshot: newest.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        }),
        db_size_bytes: db_size,
    })
}

pub struct HistoryStats {
    pub snapshot_count: usize,
    pub total_entries: usize,
    pub unique_ports: usize,
    pub oldest_snapshot: Option<DateTime<Utc>>,
    pub newest_snapshot: Option<DateTime<Utc>>,
    pub db_size_bytes: u64,
}

/// Clean up old history entries
pub fn cleanup(keep_hours: i64) -> Result<CleanupResult> {
    let conn = open_db()?;
    let cutoff = Utc::now() - Duration::hours(keep_hours);

    // Count what we're about to delete
    let snapshot_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM snapshots WHERE unix_ts < ?",
        params![cutoff.timestamp()],
        |row| row.get(0),
    )?;

    let entry_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ports WHERE snapshot_id IN (SELECT id FROM snapshots WHERE unix_ts < ?)",
        params![cutoff.timestamp()],
        |row| row.get(0),
    )?;

    // Delete old snapshots (cascades to ports)
    conn.execute(
        "DELETE FROM snapshots WHERE unix_ts < ?",
        params![cutoff.timestamp()],
    )?;

    // Vacuum to reclaim space
    conn.execute_batch("VACUUM;")?;

    Ok(CleanupResult {
        snapshots_deleted: snapshot_count as usize,
        entries_deleted: entry_count as usize,
    })
}

pub struct CleanupResult {
    pub snapshots_deleted: usize,
    pub entries_deleted: usize,
}

/// Get the most frequently used ports
pub fn get_top_ports(limit: usize) -> Result<Vec<(u16, String, usize)>> {
    let conn = open_db()?;

    let mut stmt = conn.prepare(
        "SELECT port, protocol, COUNT(*) as cnt
         FROM ports
         GROUP BY port, protocol
         ORDER BY cnt DESC
         LIMIT ?",
    )?;

    let rows = stmt.query_map(params![limit as i32], |row| {
        Ok((
            row.get::<_, i32>(0)? as u16,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)? as usize,
        ))
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Get timeline of when a specific port was active
pub fn get_port_timeline(port: u16, hours: i64) -> Result<Vec<PortTimelineEntry>> {
    let conn = open_db()?;
    let cutoff = Utc::now() - Duration::hours(hours);

    let mut stmt = conn.prepare(
        "SELECT s.timestamp, p.protocol, p.process_name, p.container, p.state
         FROM ports p
         JOIN snapshots s ON p.snapshot_id = s.id
         WHERE p.port = ? AND s.unix_ts >= ?
         ORDER BY s.unix_ts ASC",
    )?;

    let rows = stmt.query_map(params![port as i32, cutoff.timestamp()], |row| {
        let ts_str: String = row.get(0)?;
        let timestamp = DateTime::parse_from_rfc3339(&ts_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        Ok(PortTimelineEntry {
            timestamp,
            protocol: row.get(1)?,
            process_name: row.get(2)?,
            container: row.get(3)?,
            state: row.get(4)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

#[derive(Debug)]
pub struct PortTimelineEntry {
    pub timestamp: DateTime<Utc>,
    pub protocol: String,
    pub process_name: String,
    pub container: Option<String>,
    pub state: Option<String>,
}

/// Action for a diff entry: port appeared or disappeared.
#[derive(Debug)]
pub enum DiffAction {
    Appeared,
    Disappeared,
}

/// A port that changed between two snapshots.
#[derive(Debug)]
pub struct DiffEntry {
    pub port: u16,
    pub protocol: String,
    pub process_name: String,
    pub action: DiffAction,
}

/// Compare the latest snapshot against one `snapshots_ago` snapshots earlier.
///
/// Returns ports that appeared (present in latest but not older) and disappeared
/// (present in older but not latest), ordered by action then port.
pub fn get_diff(snapshots_ago: usize) -> Result<Vec<DiffEntry>> {
    let conn = open_db()?;

    // Get the (snapshots_ago + 1) most recent snapshot IDs, ordered desc
    let mut stmt = conn.prepare("SELECT id FROM snapshots ORDER BY unix_ts DESC LIMIT ?")?;
    let ids: Vec<i64> = stmt
        .query_map(params![(snapshots_ago + 1) as i64], |r| r.get(0))?
        .collect::<Result<_, _>>()?;

    if ids.len() < 2 {
        return Ok(Vec::new()); // not enough history to diff
    }

    let latest_id = ids[0];
    let older_id = ids[snapshots_ago.min(ids.len() - 1)];

    // Ports in latest but not in older → Appeared
    let mut stmt = conn.prepare(
        "SELECT DISTINCT p.port, p.protocol, COALESCE(p.process_name, '') as process_name
         FROM ports p
         WHERE p.snapshot_id = ?1
           AND NOT EXISTS (
               SELECT 1 FROM ports o
               WHERE o.snapshot_id = ?2
                 AND o.port = p.port
                 AND o.protocol = p.protocol
           )
         ORDER BY p.port ASC",
    )?;
    let appeared: Vec<DiffEntry> = stmt
        .query_map(params![latest_id, older_id], |r| {
            Ok(DiffEntry {
                port: r.get::<_, i32>(0)? as u16,
                protocol: r.get(1)?,
                process_name: r.get(2)?,
                action: DiffAction::Appeared,
            })
        })?
        .collect::<Result<_, _>>()?;

    // Ports in older but not in latest → Disappeared
    let mut stmt = conn.prepare(
        "SELECT DISTINCT p.port, p.protocol, COALESCE(p.process_name, '') as process_name
         FROM ports p
         WHERE p.snapshot_id = ?1
           AND NOT EXISTS (
               SELECT 1 FROM ports n
               WHERE n.snapshot_id = ?2
                 AND n.port = p.port
                 AND n.protocol = p.protocol
           )
         ORDER BY p.port ASC",
    )?;
    let disappeared: Vec<DiffEntry> = stmt
        .query_map(params![older_id, latest_id], |r| {
            Ok(DiffEntry {
                port: r.get::<_, i32>(0)? as u16,
                protocol: r.get(1)?,
                process_name: r.get(2)?,
                action: DiffAction::Disappeared,
            })
        })?
        .collect::<Result<_, _>>()?;

    let mut entries = appeared;
    entries.extend(disappeared);
    Ok(entries)
}

/// Format bytes for display
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;

    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_in_memory() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable fk");
        conn
    }

    fn user_version(conn: &Connection) -> i64 {
        conn.query_row("PRAGMA user_version", [], |r| r.get(0))
            .expect("read user_version")
    }

    fn counter_value(conn: &Connection) -> String {
        conn.query_row(
            "SELECT value FROM meta WHERE key = 'auto_prune_counter'",
            [],
            |r| r.get(0),
        )
        .expect("read counter")
    }

    #[test]
    fn init_db_creates_meta_on_v2() {
        let conn = open_in_memory();
        init_db(&conn).expect("init_db ok");
        assert_eq!(user_version(&conn), 2);
        assert_eq!(counter_value(&conn), "0");
    }

    #[test]
    fn init_db_idempotent() {
        let conn = open_in_memory();
        init_db(&conn).expect("first init");
        init_db(&conn).expect("second init");
        assert_eq!(user_version(&conn), 2);
        assert_eq!(counter_value(&conn), "0");
    }

    #[test]
    fn init_db_v1_to_v2_migrates_in_place_preserving_data() {
        let conn = open_in_memory();
        // Build a v1-shape DB by hand.
        conn.execute_batch(
            "CREATE TABLE snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                unix_ts INTEGER NOT NULL
            );
            CREATE TABLE ports (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                snapshot_id INTEGER NOT NULL,
                port INTEGER NOT NULL,
                protocol TEXT NOT NULL,
                address TEXT NOT NULL,
                pid INTEGER,
                process_name TEXT,
                container TEXT,
                state TEXT,
                remote_addr TEXT,
                FOREIGN KEY (snapshot_id) REFERENCES snapshots(id) ON DELETE CASCADE
            );
            PRAGMA user_version = 1;",
        )
        .expect("v1 schema");
        // Seed one v1 snapshot + port row.
        conn.execute(
            "INSERT INTO snapshots (timestamp, unix_ts) VALUES ('2026-01-01T00:00:00Z', 1735689600)",
            [],
        )
        .expect("seed snapshot");
        let snap_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO ports (snapshot_id, port, protocol, address, pid, process_name, state)
             VALUES (?1, 8080, 'tcp', '0.0.0.0', 1234, 'nginx', 'LISTEN')",
            params![snap_id],
        )
        .expect("seed port");

        init_db(&conn).expect("v1->v2 migration");

        assert_eq!(user_version(&conn), 2);
        assert_eq!(counter_value(&conn), "0");
        // Pre-existing data must survive the migration.
        let snap_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))
            .expect("snapshots count");
        assert_eq!(snap_count, 1);
        let port_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM ports", [], |r| r.get(0))
            .expect("ports count");
        assert_eq!(port_count, 1);
        let proc_name: String = conn
            .query_row("SELECT process_name FROM ports", [], |r| r.get(0))
            .expect("port row");
        assert_eq!(proc_name, "nginx");
    }

    #[test]
    fn init_db_rejects_future_schema() {
        let conn = open_in_memory();
        conn.execute_batch("PRAGMA user_version = 99;")
            .expect("set future version");
        let err = init_db(&conn).expect_err("future schema must fail");
        let msg = format!("{err}");
        assert!(
            msg.contains("newer than"),
            "error should mention 'newer than': {msg}"
        );
    }

    #[test]
    fn init_db_rejects_negative_user_version() {
        let conn = open_in_memory();
        // PRAGMA user_version uses a 32-bit signed slot; -1 round-trips.
        conn.execute_batch("PRAGMA user_version = -1;")
            .expect("set negative");
        let err = init_db(&conn).expect_err("negative version must fail");
        let msg = format!("{err}");
        assert!(
            msg.contains("invalid"),
            "error should mention 'invalid': {msg}"
        );
    }

    #[test]
    fn init_db_restores_deleted_counter_row() {
        let conn = open_in_memory();
        init_db(&conn).expect("first init");
        conn.execute("DELETE FROM meta", [])
            .expect("manual delete of meta row");
        let missing: rusqlite::Result<String> = conn.query_row(
            "SELECT value FROM meta WHERE key = 'auto_prune_counter'",
            [],
            |r| r.get(0),
        );
        assert!(missing.is_err(), "counter row must be missing first");

        init_db(&conn).expect("second init restores row");
        assert_eq!(counter_value(&conn), "0");
    }
}
