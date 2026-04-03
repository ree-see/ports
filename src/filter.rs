//! Dev-process filtering for the `--dev` flag.
//!
//! Determines whether a port belongs to a developer-relevant
//! process by checking framework detection, container presence,
//! a known dev-binary allowlist, and platform-specific
//! blocklists of system daemons and desktop applications.

use std::collections::HashSet;
use std::sync::LazyLock;

use crate::types::PortInfo;

// ── Dev binary allowlist ───────────────────────────────

static DEV_BINARIES: LazyLock<HashSet<&str>> = LazyLock::new(|| {
    HashSet::from([
        "node",
        "python",
        "python3",
        "ruby",
        "java",
        "go",
        "cargo",
        "deno",
        "bun",
        "php",
        "uvicorn",
        "gunicorn",
        "flask",
        "rails",
        "npm",
        "npx",
        "yarn",
        "pnpm",
        "tsc",
        "tsx",
        "esbuild",
        "rollup",
        "turbo",
        "nx",
        "jest",
        "vitest",
        "mocha",
        "pytest",
        "cypress",
        "playwright",
        "rustc",
        "dotnet",
        "gradle",
        "mvn",
        "mix",
        "elixir",
    ])
});

// ── Platform blocklists ────────────────────────────────

#[cfg(target_os = "macos")]
static BLOCKLIST_PREFIX: &[&str] = &[
    // Desktop / consumer apps
    "spotify",
    "raycast",
    "tableplus",
    "postman",
    "linear helper",
    "controlcenter",
    "rapportd",
    "superhuman",
    "setappagent",
    "slack",
    "discord",
    "firefox",
    "chrome",
    "google chrome",
    "safari",
    "figma",
    "notion",
    "zoom",
    "teams",
    "code helper",
    "iterm2",
    "warp",
    "arc",
    // System daemons
    "loginwindow",
    "windowserver",
    "systemuiserver",
    "kernel_task",
    "launchd",
    "mdworker",
    "mds_stores",
    "cfprefsd",
    "coreaudiod",
    "corebrightnessd",
    "airportd",
    "bluetoothd",
    "sharingd",
    "usernoted",
    "notificationcenter",
    "cloudd",
    "nsurlsessiond",
    "trustd",
    "securityd",
    "opendirectoryd",
    "diskarbitrationd",
];

#[cfg(target_os = "linux")]
static BLOCKLIST_PREFIX: &[&str] = &[
    // System services
    "systemd",
    "dbus-daemon",
    "networkmanager",
    "avahi-daemon",
    "cupsd",
    "cron",
    "rsyslogd",
    "snapd",
    "polkitd",
    "accounts-daemon",
    "udisksd",
    "thermald",
    "irqbalance",
    "acpid",
    "atd",
    "gdm",
    "lightdm",
    "pipewire",
    "pulseaudio",
    "xdg-",
    "gvfs",
    "tracker-",
    "evolution-",
    "gnome-shell",
    "kwin",
    "plasmashell",
    "nautilus",
    "thunar",
    // Desktop apps
    "spotify",
    "slack",
    "discord",
    "firefox",
    "google-chrome",
    "chromium",
    "figma",
    "notion",
    "zoom",
    "teams",
    "telegram",
    "signal",
];

/// Exact-match blocklist for Linux only.
///
/// `code` is blocked exactly but not as a prefix, so
/// `code-server` (a dev tool) passes through.
#[cfg(target_os = "linux")]
static BLOCKLIST_EXACT: &[&str] = &["code"];

// ── Public API ─────────────────────────────────────────

/// Returns `true` when a port entry looks developer-relevant.
///
/// Priority order:
/// 1. Framework detected -> true
/// 2. Container present  -> true
/// 3. Dev-binary allowlist match -> true
/// 4. Platform blocklist match -> false
/// 5. Default -> true (conservative: don't hide unknowns)
pub fn is_dev_process(info: &PortInfo) -> bool {
    if info.framework.is_some() {
        return true;
    }
    if info.container.is_some() {
        return true;
    }

    let name = info.process_name.to_lowercase();

    if DEV_BINARIES.contains(name.as_str()) {
        return true;
    }

    if is_blocklisted(&name) {
        return false;
    }

    true
}

/// Filter a vec in-place, keeping only dev-relevant ports.
pub fn retain_dev_only(ports: &mut Vec<PortInfo>) {
    ports.retain(is_dev_process);
}

// ── Blocklist helper ───────────────────────────────────

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn is_blocklisted(name: &str) -> bool {
    for prefix in BLOCKLIST_PREFIX {
        if name.starts_with(prefix) {
            return true;
        }
    }

    #[cfg(target_os = "linux")]
    for exact in BLOCKLIST_EXACT {
        if name == *exact {
            return true;
        }
    }

    false
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn is_blocklisted(_name: &str) -> bool {
    false
}

// ── Tests ──────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Protocol;

    fn make_port_info(process_name: &str) -> PortInfo {
        PortInfo {
            port: 8080,
            protocol: Protocol::Tcp,
            pid: 1234,
            process_name: process_name.to_string(),
            address: "127.0.0.1:8080".to_string(),
            remote_address: None,
            container: None,
            service_name: None,
            command_line: None,
            cwd: None,
            framework: None,
        }
    }

    fn with_framework(mut info: PortInfo, fw: &str) -> PortInfo {
        info.framework = Some(fw.to_string());
        info
    }

    fn with_container(mut info: PortInfo, ct: &str) -> PortInfo {
        info.container = Some(ct.to_string());
        info
    }

    // ── Framework / container priority ─────────────

    #[test]
    fn test_framework_detected_is_dev() {
        let info = with_framework(make_port_info("unknown"), "Next.js");
        assert!(is_dev_process(&info));
    }

    #[test]
    fn test_container_is_dev() {
        let info = with_container(make_port_info("docker-proxy"), "postgres");
        assert!(is_dev_process(&info));
    }

    // ── Allowlist ──────────────────────────────────

    #[test]
    fn test_allowlist_exact_match() {
        for name in &["node", "python3", "cargo"] {
            assert!(
                is_dev_process(&make_port_info(name)),
                "{name} should be in the dev allowlist"
            );
        }
    }

    #[test]
    fn test_allowlist_case_insensitive() {
        assert!(is_dev_process(&make_port_info("Node")));
        assert!(is_dev_process(&make_port_info("PYTHON3")));
    }

    // ── Conservative default ───────────────────────

    #[test]
    fn test_unknown_process_passes() {
        assert!(is_dev_process(&make_port_info("my-custom-app")));
    }

    #[test]
    fn test_allowlist_overrides_nothing() {
        // "node" with no framework/container still passes
        let info = make_port_info("node");
        assert!(info.framework.is_none());
        assert!(info.container.is_none());
        assert!(is_dev_process(&info));
    }

    // ── retain_dev_only ────────────────────────────

    #[test]
    fn test_retain_dev_only() {
        let mut ports = vec![
            make_port_info("node"),
            make_port_info("my-custom-app"),
            with_framework(make_port_info("unknown"), "Vite"),
            with_container(make_port_info("docker-proxy"), "redis"),
        ];

        // On macOS/Linux a blocklisted entry would be removed;
        // on other platforms everything passes. We add a
        // platform-gated entry so the test is meaningful
        // everywhere.
        #[cfg(target_os = "macos")]
        ports.push(make_port_info("loginwindow"));
        #[cfg(target_os = "linux")]
        ports.push(make_port_info("systemd"));

        retain_dev_only(&mut ports);

        // The first four should always survive.
        assert!(ports.iter().any(|p| p.process_name == "node"));
        assert!(ports.iter().any(|p| p.process_name == "my-custom-app"));
        assert!(ports.iter().any(|p| p.framework.is_some()));
        assert!(ports.iter().any(|p| p.container.is_some()));

        // Platform blocklisted entries should be gone.
        #[cfg(target_os = "macos")]
        assert!(!ports.iter().any(|p| p.process_name == "loginwindow"));
        #[cfg(target_os = "linux")]
        assert!(!ports.iter().any(|p| p.process_name == "systemd"));
    }

    // ── macOS blocklist ────────────────────────────

    #[cfg(target_os = "macos")]
    #[test]
    fn test_blocklist_macos_daemon() {
        assert!(!is_dev_process(&make_port_info("loginwindow")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_blocklist_macos_desktop() {
        assert!(!is_dev_process(&make_port_info("spotify")));
        assert!(!is_dev_process(&make_port_info("slack")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_blocklist_macos_prefix() {
        // "rapportd" matches the "rapportd" prefix exactly.
        assert!(!is_dev_process(&make_port_info("rapportd")));
        // "mdworker_shared" starts with "mdworker".
        assert!(!is_dev_process(&make_port_info("mdworker_shared")));
    }

    // ── Linux blocklist ────────────────────────────

    #[cfg(target_os = "linux")]
    #[test]
    fn test_blocklist_linux_systemd() {
        assert!(!is_dev_process(&make_port_info("systemd-resolved")));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_blocklist_linux_desktop() {
        assert!(!is_dev_process(&make_port_info("firefox")));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_blocklist_linux_code_exact() {
        // "code" is blocked exactly.
        assert!(!is_dev_process(&make_port_info("code")));
        // "code-server" must NOT be blocked (prefix mismatch).
        assert!(is_dev_process(&make_port_info("code-server")));
    }
}
