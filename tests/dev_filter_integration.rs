use std::process::Command;

#[test]
fn test_dev_flag_runs() {
    let output = Command::new("cargo")
        .args(["run", "--", "--dev"])
        .output()
        .expect("failed to execute");

    assert!(
        output.status.success(),
        "ports --dev failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_dev_flag_json() {
    let output = Command::new("cargo")
        .args(["run", "--", "--dev", "--json"])
        .output()
        .expect("failed to execute");

    assert!(
        output.status.success(),
        "ports --dev --json failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON output");
    let ports = parsed
        .get("ports")
        .expect("expected `ports` key in JSON wrapper");
    assert!(ports.is_array(), "expected `ports` to be an array");
}

#[test]
fn test_dev_flag_help() {
    let output = Command::new("cargo")
        .args(["run", "--", "--help"])
        .output()
        .expect("failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--dev"), "--dev not found in help output");
}
