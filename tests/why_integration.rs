use std::process::Command;

#[test]
fn test_why_help_shows_usage() {
    let output = Command::new("cargo")
        .args(["run", "--", "why", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ancestry"));
    assert!(stdout.contains("TARGET"));
}

#[test]
fn test_why_nonexistent_target_graceful() {
    let output = Command::new("cargo")
        .args(["run", "--", "why", "nonexistent_xyz_98765"])
        .output()
        .expect("Failed to execute command");

    // Should succeed (exit 0) but print "No process found" to stderr.
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No process found"),
        "Expected 'No process found' in stderr, got: {}",
        stderr
    );
}

#[test]
fn test_why_json_nonexistent_returns_empty_array() {
    let output = Command::new("cargo")
        .args(["run", "--", "why", "--json", "nonexistent_xyz_98765"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "[]");
}

#[test]
fn test_why_flag_on_list() {
    let output = Command::new("cargo")
        .args(["run", "--", "--why"])
        .output()
        .expect("Failed to execute command");

    // Should succeed (may have no ports, but no error).
    assert!(output.status.success());
}

#[test]
fn test_why_flag_on_list_json() {
    let output = Command::new("cargo")
        .args(["run", "--", "--why", "--json"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should be valid JSON (array).
    assert!(
        stdout.trim().starts_with('['),
        "Expected JSON array, got: {}",
        stdout
    );
}

#[test]
fn test_why_watch_rejected_for_why_subcommand() {
    let output = Command::new("cargo")
        .args(["run", "--", "--watch", "why", "8080"])
        .output()
        .expect("Failed to execute command");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Cannot use --watch with why"),
        "Expected watch rejection, got: {}",
        stderr
    );
}
