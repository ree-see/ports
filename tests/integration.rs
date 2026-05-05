use std::process::Command;
use tempfile::TempDir;

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

// HOME is set to a tempdir explicitly. Do not unset HOME — `dirs::home_dir`
// falls through to `getpwuid_r` when HOME is empty/unset and would write
// to the developer's real home directory.
fn ports_completions_cmd(temp_home: &TempDir, args: &[&str]) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.args(["run", "--quiet", "--"]);
    cmd.args(args);
    cmd.env("HOME", temp_home.path());
    cmd.env_remove("XDG_CONFIG_HOME");
    cmd.env_remove("XDG_DATA_HOME");
    cmd
}

#[test]
fn installs_fish_completions_to_xdg_config() {
    let temp = TempDir::new().expect("tempdir");
    let output = ports_completions_cmd(&temp, &["completions", "fish"])
        .output()
        .expect("run completions fish");

    assert!(output.status.success(), "exit not 0: {output:?}");

    let installed = temp.path().join(".config/fish/completions/ports.fish");
    assert!(installed.exists(), "expected file at {installed:?}");

    let body = std::fs::read_to_string(&installed).unwrap();
    assert!(
        body.starts_with("complete -c ports -f\n"),
        "installed file missing fish file-suppression prefix"
    );
}

#[test]
fn regenerate_overwrites_existing_completions() {
    let temp = TempDir::new().expect("tempdir");
    let installed = temp.path().join(".config/fish/completions/ports.fish");

    for _ in 0..2 {
        let output = ports_completions_cmd(&temp, &["completions", "fish"])
            .output()
            .expect("run completions fish");
        assert!(output.status.success());
    }

    assert!(installed.exists());
    let body = std::fs::read_to_string(&installed).unwrap();
    assert!(body.starts_with("complete -c ports -f\n"));
}

#[test]
fn unsupported_shell_errors_with_print_hint() {
    let temp = TempDir::new().expect("tempdir");
    let output = ports_completions_cmd(&temp, &["completions", "powershell"])
        .output()
        .expect("run completions powershell");

    assert!(!output.status.success(), "expected non-zero exit");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not supported") && stderr.contains("--print"),
        "stderr should explain the unsupported shell and point to --print: {stderr}"
    );
}

#[test]
fn print_flag_emits_stdout_without_writing_file() {
    let temp = TempDir::new().expect("tempdir");
    let output = ports_completions_cmd(&temp, &["completions", "fish", "--print"])
        .output()
        .expect("run completions fish --print");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("complete -c ports -f\n"),
        "stdout should be the fish completion content"
    );

    let installed = temp.path().join(".config/fish/completions/ports.fish");
    assert!(
        !installed.exists(),
        "--print should not create the install file"
    );
}
