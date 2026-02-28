//! Integration tests for the `ports history` subcommands.
//!
//! These tests use a temporary directory for the history database to avoid
//! polluting the user's actual history data.

use std::process::Command;
use tempfile::TempDir;

/// Run a ports command with a custom HOME to isolate the test database
fn run_ports_with_temp_home(args: &[&str], temp_home: &TempDir) -> std::process::Output {
    Command::new("cargo")
        .args(["run", "--"])
        .args(args)
        .env("HOME", temp_home.path())
        .env("XDG_DATA_HOME", temp_home.path().join(".local/share"))
        .output()
        .expect("Failed to execute command")
}

/// Run ports command and return (success, stdout, stderr)
fn run_and_capture(args: &[&str], temp_home: &TempDir) -> (bool, String, String) {
    let output = run_ports_with_temp_home(args, temp_home);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), stdout, stderr)
}

// ============================================================================
// history record
// ============================================================================

#[test]
fn test_history_record_basic() {
    let temp_home = TempDir::new().expect("Failed to create temp dir");

    let (success, stdout, stderr) = run_and_capture(&["history", "record"], &temp_home);

    // Should succeed even on first run (creates the database)
    assert!(success, "history record failed: {}", stderr);
    assert!(
        stdout.contains("Recorded") || stdout.contains("ports"),
        "Expected confirmation message, got: {}",
        stdout
    );
}

#[test]
fn test_history_record_with_connections() {
    let temp_home = TempDir::new().expect("Failed to create temp dir");

    let (success, stdout, stderr) =
        run_and_capture(&["history", "record", "--connections"], &temp_home);

    assert!(success, "history record --connections failed: {}", stderr);
    assert!(
        stdout.contains("Recorded"),
        "Expected confirmation, got: {}",
        stdout
    );
}

#[test]
fn test_history_record_json_output() {
    let temp_home = TempDir::new().expect("Failed to create temp dir");

    let (success, stdout, stderr) = run_and_capture(&["--json", "history", "record"], &temp_home);

    assert!(success, "history record --json failed: {}", stderr);

    // Should be valid JSON
    let json: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(json.is_ok(), "Expected valid JSON, got: {}", stdout);

    let json = json.unwrap();
    assert!(
        json.get("snapshot_id").is_some(),
        "Expected snapshot_id in JSON"
    );
    assert!(
        json.get("port_count").is_some(),
        "Expected port_count in JSON"
    );
    assert!(
        json.get("timestamp").is_some(),
        "Expected timestamp in JSON"
    );
}

// ============================================================================
// history show
// ============================================================================

#[test]
fn test_history_show_empty() {
    let temp_home = TempDir::new().expect("Failed to create temp dir");

    // Query without recording first - should handle gracefully
    let (success, stdout, _stderr) = run_and_capture(&["history", "show"], &temp_home);

    assert!(success, "history show should succeed even with no data");
    assert!(
        stdout.contains("No history") || stdout.is_empty() || stdout.contains("record"),
        "Expected empty/helpful message, got: {}",
        stdout
    );
}

#[test]
fn test_history_show_after_record() {
    let temp_home = TempDir::new().expect("Failed to create temp dir");

    // Record first
    let (success, _, stderr) = run_and_capture(&["history", "record"], &temp_home);
    assert!(success, "record failed: {}", stderr);

    // Now query
    let (success, stdout, stderr) = run_and_capture(&["history", "show"], &temp_home);
    assert!(success, "history show failed: {}", stderr);

    // Should have some output (table or "no history" if no ports active)
    assert!(
        !stdout.is_empty() || stdout.contains("No history"),
        "Expected some output: {}",
        stdout
    );
}

#[test]
fn test_history_show_json_output() {
    let temp_home = TempDir::new().expect("Failed to create temp dir");

    // Record first
    let (success, _, _) = run_and_capture(&["history", "record"], &temp_home);
    assert!(success, "record failed");

    // Query with JSON
    let (success, stdout, stderr) = run_and_capture(&["--json", "history", "show"], &temp_home);
    assert!(success, "history show --json failed: {}", stderr);

    // Should be valid JSON (array)
    let json: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(json.is_ok(), "Expected valid JSON array, got: {}", stdout);
    assert!(json.unwrap().is_array(), "Expected JSON array");
}

#[test]
fn test_history_show_with_port_filter() {
    let temp_home = TempDir::new().expect("Failed to create temp dir");

    // Record
    let _ = run_and_capture(&["history", "record"], &temp_home);

    // Query specific port (22 is common for SSH)
    let (success, _stdout, stderr) =
        run_and_capture(&["history", "show", "--port", "22"], &temp_home);
    assert!(success, "history show --port failed: {}", stderr);
}

#[test]
fn test_history_show_with_process_filter() {
    let temp_home = TempDir::new().expect("Failed to create temp dir");

    // Record
    let _ = run_and_capture(&["history", "record"], &temp_home);

    // Query by process name
    let (success, _stdout, stderr) =
        run_and_capture(&["history", "show", "-P", "sshd"], &temp_home);
    assert!(success, "history show -P failed: {}", stderr);
}

#[test]
fn test_history_show_with_hours_limit() {
    let temp_home = TempDir::new().expect("Failed to create temp dir");

    // Record
    let _ = run_and_capture(&["history", "record"], &temp_home);

    // Query with custom hours
    let (success, _stdout, stderr) = run_and_capture(&["history", "show", "-H", "48"], &temp_home);
    assert!(success, "history show -H failed: {}", stderr);
}

#[test]
fn test_history_show_with_limit() {
    let temp_home = TempDir::new().expect("Failed to create temp dir");

    // Record
    let _ = run_and_capture(&["history", "record"], &temp_home);

    // Query with row limit
    let (success, _stdout, stderr) =
        run_and_capture(&["history", "show", "--limit", "5"], &temp_home);
    assert!(success, "history show --limit failed: {}", stderr);
}

// ============================================================================
// history timeline
// ============================================================================

#[test]
fn test_history_timeline_basic() {
    let temp_home = TempDir::new().expect("Failed to create temp dir");

    // Record first
    let _ = run_and_capture(&["history", "record"], &temp_home);

    // Query timeline for port 22
    let (success, stdout, stderr) = run_and_capture(&["history", "timeline", "22"], &temp_home);
    assert!(success, "history timeline failed: {}", stderr);

    // Should have output (timeline or "no history")
    assert!(
        stdout.contains("Timeline") || stdout.contains("No history") || stdout.is_empty(),
        "Expected timeline output, got: {}",
        stdout
    );
}

#[test]
fn test_history_timeline_json_output() {
    let temp_home = TempDir::new().expect("Failed to create temp dir");

    // Record first
    let _ = run_and_capture(&["history", "record"], &temp_home);

    // Query with JSON
    let (success, stdout, stderr) =
        run_and_capture(&["--json", "history", "timeline", "22"], &temp_home);
    assert!(success, "history timeline --json failed: {}", stderr);

    // Should be valid JSON array
    let json: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(json.is_ok(), "Expected valid JSON, got: {}", stdout);
}

#[test]
fn test_history_timeline_with_hours() {
    let temp_home = TempDir::new().expect("Failed to create temp dir");

    // Record first
    let _ = run_and_capture(&["history", "record"], &temp_home);

    // Query with custom hours
    let (success, _stdout, stderr) =
        run_and_capture(&["history", "timeline", "80", "-H", "168"], &temp_home);
    assert!(success, "history timeline -H failed: {}", stderr);
}

// ============================================================================
// history stats
// ============================================================================

#[test]
fn test_history_stats_empty() {
    let temp_home = TempDir::new().expect("Failed to create temp dir");

    // Stats on empty database
    let (success, stdout, stderr) = run_and_capture(&["history", "stats"], &temp_home);
    assert!(success, "history stats failed: {}", stderr);

    // Should show zero stats
    assert!(
        stdout.contains("Statistics") || stdout.contains("Snapshots"),
        "Expected stats output, got: {}",
        stdout
    );
}

#[test]
fn test_history_stats_after_records() {
    let temp_home = TempDir::new().expect("Failed to create temp dir");

    // Record multiple snapshots
    let _ = run_and_capture(&["history", "record"], &temp_home);
    let _ = run_and_capture(&["history", "record"], &temp_home);

    // Get stats
    let (success, stdout, stderr) = run_and_capture(&["history", "stats"], &temp_home);
    assert!(success, "history stats failed: {}", stderr);

    // Should show non-zero stats
    assert!(
        stdout.contains("Snapshots"),
        "Expected Snapshots in output: {}",
        stdout
    );
}

#[test]
fn test_history_stats_json_output() {
    let temp_home = TempDir::new().expect("Failed to create temp dir");

    // Record first
    let _ = run_and_capture(&["history", "record"], &temp_home);

    // Get stats as JSON
    let (success, stdout, stderr) = run_and_capture(&["--json", "history", "stats"], &temp_home);
    assert!(success, "history stats --json failed: {}", stderr);

    // Should be valid JSON with expected fields
    let json: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(json.is_ok(), "Expected valid JSON, got: {}", stdout);

    let json = json.unwrap();
    assert!(
        json.get("snapshot_count").is_some(),
        "Expected snapshot_count"
    );
    assert!(
        json.get("total_entries").is_some(),
        "Expected total_entries"
    );
    assert!(json.get("unique_ports").is_some(), "Expected unique_ports");
    assert!(
        json.get("db_size_bytes").is_some(),
        "Expected db_size_bytes"
    );
    assert!(json.get("top_ports").is_some(), "Expected top_ports");
}

// ============================================================================
// history clean
// ============================================================================

#[test]
fn test_history_clean_empty() {
    let temp_home = TempDir::new().expect("Failed to create temp dir");

    // Clean on empty database
    let (success, stdout, stderr) = run_and_capture(&["history", "clean"], &temp_home);
    assert!(success, "history clean failed: {}", stderr);

    // Should report 0 deleted
    assert!(
        stdout.contains("0") || stdout.contains("Cleaned"),
        "Expected cleanup confirmation, got: {}",
        stdout
    );
}

#[test]
fn test_history_clean_with_keep() {
    let temp_home = TempDir::new().expect("Failed to create temp dir");

    // Record first
    let _ = run_and_capture(&["history", "record"], &temp_home);

    // Clean with custom keep period (keep 1 hour, should keep recent)
    let (success, stdout, stderr) =
        run_and_capture(&["history", "clean", "--keep", "1"], &temp_home);
    assert!(success, "history clean --keep failed: {}", stderr);

    // Should succeed
    assert!(
        stdout.contains("Cleaned"),
        "Expected confirmation: {}",
        stdout
    );
}

#[test]
fn test_history_clean_json_output() {
    let temp_home = TempDir::new().expect("Failed to create temp dir");

    // Record first
    let _ = run_and_capture(&["history", "record"], &temp_home);

    // Clean with JSON output
    let (success, stdout, stderr) = run_and_capture(&["--json", "history", "clean"], &temp_home);
    assert!(success, "history clean --json failed: {}", stderr);

    // Should be valid JSON
    let json: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(json.is_ok(), "Expected valid JSON, got: {}", stdout);

    let json = json.unwrap();
    assert!(
        json.get("snapshots_deleted").is_some(),
        "Expected snapshots_deleted"
    );
    assert!(
        json.get("entries_deleted").is_some(),
        "Expected entries_deleted"
    );
}

// ============================================================================
// Integration workflow tests
// ============================================================================

#[test]
fn test_full_history_workflow() {
    let temp_home = TempDir::new().expect("Failed to create temp dir");

    // 1. Record initial snapshot
    let (success, _, _) = run_and_capture(&["history", "record"], &temp_home);
    assert!(success, "initial record failed");

    // 2. Record with connections
    let (success, _, _) = run_and_capture(&["history", "record", "--connections"], &temp_home);
    assert!(success, "record with connections failed");

    // 3. Check stats shows 2 snapshots
    let (success, stdout, _) = run_and_capture(&["--json", "history", "stats"], &temp_home);
    assert!(success, "stats failed");

    let stats: serde_json::Value = serde_json::from_str(&stdout).expect("parse stats");
    let snapshot_count = stats["snapshot_count"].as_u64().unwrap_or(0);
    assert_eq!(
        snapshot_count, 2,
        "Expected 2 snapshots, got {}",
        snapshot_count
    );

    // 4. Query history
    let (success, _, _) = run_and_capture(&["history", "show"], &temp_home);
    assert!(success, "show failed");

    // 5. Clean (keeping all recent data)
    let (success, stdout, _) =
        run_and_capture(&["--json", "history", "clean", "--keep", "1"], &temp_home);
    assert!(success, "clean failed");

    let clean_result: serde_json::Value = serde_json::from_str(&stdout).expect("parse clean");
    assert!(
        clean_result.get("snapshots_deleted").is_some(),
        "Expected snapshots_deleted"
    );
}

#[test]
fn test_repeated_records_increment_snapshots() {
    let temp_home = TempDir::new().expect("Failed to create temp dir");

    // Record 3 times
    for i in 1..=3 {
        let (success, _, stderr) = run_and_capture(&["history", "record"], &temp_home);
        assert!(success, "record {} failed: {}", i, stderr);
    }

    // Check stats
    let (success, stdout, _) = run_and_capture(&["--json", "history", "stats"], &temp_home);
    assert!(success, "stats failed");

    let stats: serde_json::Value = serde_json::from_str(&stdout).expect("parse stats");
    let snapshot_count = stats["snapshot_count"].as_u64().unwrap_or(0);
    assert_eq!(snapshot_count, 3, "Expected 3 snapshots after 3 records");
}
