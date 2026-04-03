use std::process::Command;

/// On Linux and macOS, `--json` output should include
/// `command_line` and `cwd` for at least one port entry.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn test_json_includes_process_details() {
    let output = Command::new("cargo")
        .args(["run", "--", "--json"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let data: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).expect("JSON output should be a valid array");

    if data.is_empty() {
        // No listening ports -- nothing to verify.
        return;
    }

    let has_cmd = data.iter().any(|p| p.get("command_line").is_some());
    let has_cwd = data.iter().any(|p| p.get("cwd").is_some());

    assert!(has_cmd, "Expected at least one port with command_line");
    assert!(has_cwd, "Expected at least one port with cwd");
}

#[test]
fn test_cli_help_runs() {
    let output = Command::new("cargo")
        .args(["run", "--", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ports"));
}

#[test]
fn test_cli_version_runs() {
    let output = Command::new("cargo")
        .args(["run", "--", "--version"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}
