//! Git context detection from a process working directory.

use super::GitContext;

/// Detect git repo and branch from a process CWD path.
///
/// Returns `None` if cwd is a system directory (e.g. `/`), no `.git` is found,
/// or the path is not readable.
pub fn detect_git_context(cwd: &str) -> Option<GitContext> {
    // Skip system directories — daemons typically chdir("/").
    if cwd == "/" || cwd.starts_with("/usr") || cwd.starts_with("/var/run") {
        return None;
    }

    let mut dir = std::path::PathBuf::from(cwd);
    loop {
        let git_dir = dir.join(".git");
        if git_dir.exists() {
            let head_path = git_dir.join("HEAD");
            let branch = std::fs::read_to_string(&head_path).ok().map(|content| {
                let content = content.trim();
                if let Some(branch) = content.strip_prefix("ref: refs/heads/") {
                    branch.to_string()
                } else {
                    // Detached HEAD — show short hash.
                    content[..8.min(content.len())].to_string()
                }
            });

            // Use directory name as repo name.
            let repo_name = dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| dir.to_string_lossy().to_string());

            return Some(GitContext { repo_name, branch });
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// Read the current working directory of a process.
#[cfg(target_os = "linux")]
pub fn read_process_cwd(pid: u32) -> Option<String> {
    std::fs::read_link(format!("/proc/{}/cwd", pid))
        .ok()
        .map(|p| p.to_string_lossy().to_string())
}

/// Read the current working directory of a process via lsof.
#[cfg(target_os = "macos")]
pub fn read_process_cwd(pid: u32) -> Option<String> {
    let output = std::process::Command::new("lsof")
        .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix('n') {
            if !path.is_empty() {
                return Some(path.to_string());
            }
        }
    }
    None
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn read_process_cwd(_pid: u32) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skip_root_cwd() {
        assert!(detect_git_context("/").is_none());
    }

    #[test]
    fn test_skip_usr_cwd() {
        assert!(detect_git_context("/usr/bin").is_none());
    }

    #[test]
    fn test_skip_var_run_cwd() {
        assert!(detect_git_context("/var/run/myservice").is_none());
    }

    #[test]
    fn test_nonexistent_path() {
        assert!(detect_git_context("/tmp/nonexistent_path_xyz_12345").is_none());
    }
}
