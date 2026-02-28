//! Tiered source detection algorithm.
//!
//! Priority order (highest wins):
//!   Tier 1: Container (cgroup-based) → Docker
//!   Tier 2: Init system (cgroup/metadata) → Systemd, Launchd
//!   Tier 3: Supervisors (chain name match, top-down) → Pm2, Supervisord, Gunicorn, Runit, S6
//!   Tier 4: Multiplexers (chain name match) → Tmux, Screen, Nohup
//!   Tier 5: Cron (chain name match) → Cron
//!   Tier 6: Shell (direct parent only) → Shell
//!   Default: Unknown

use super::{Ancestor, SourceType};

/// Known shell binary names.
const SHELLS: &[&str] = &["bash", "sh", "zsh", "fish", "tcsh", "dash", "ksh", "csh"];

/// Tier-3 supervisor names mapped to their SourceType.
const SUPERVISORS: &[(&str, SourceType)] = &[
    ("pm2", SourceType::Pm2),
    ("supervisord", SourceType::Supervisord),
    ("supervisor", SourceType::Supervisord),
    ("gunicorn", SourceType::Gunicorn),
    ("runsv", SourceType::Runit),
    ("runsvdir", SourceType::Runit),
    ("s6-svscan", SourceType::S6),
    ("s6-supervise", SourceType::S6),
];

/// Tier-4 multiplexer names.
const MULTIPLEXERS: &[(&str, SourceType)] = &[
    ("tmux", SourceType::Tmux),
    ("tmux: server", SourceType::Tmux),
    ("screen", SourceType::Screen),
    ("nohup", SourceType::Nohup),
];

/// Cron-related process names.
const CRON_NAMES: &[&str] = &["cron", "crond", "anacron"];

/// Detect the source/supervisor for a process given its ancestry chain and
/// optional cgroup content.
///
/// The chain should be ordered from the target process (index 0) up toward
/// PID 1 (last element). `cgroup` is the raw content of `/proc/{pid}/cgroup`
/// on Linux (None on other platforms).
pub fn detect_source(chain: &[Ancestor], cgroup: Option<&str>) -> SourceType {
    // Tier 1: Container detection via cgroup.
    if let Some(cg) = cgroup {
        if cg.contains("/docker/")
            || cg.contains("/containerd/")
            || cg.contains("/kubepods/")
            || cg.contains("/podman-")
        {
            return SourceType::Docker;
        }
    }

    // Tier 2: Init system via cgroup metadata.
    if let Some(cg) = cgroup {
        if cg.contains(".service") {
            return SourceType::Systemd;
        }
    }

    // For tiers 3-6, walk the chain from TOP (nearest PID 1) to BOTTOM (target)
    // so the highest-level supervisor wins.
    let chain_top_down: Vec<&Ancestor> = chain.iter().rev().collect();

    // Tier 3: Known supervisors.
    for ancestor in &chain_top_down {
        let name_lower = ancestor.name.to_lowercase();
        for (supervisor_name, source_type) in SUPERVISORS {
            if name_lower == *supervisor_name {
                return source_type.clone();
            }
        }
    }

    // Tier 4: Multiplexers.
    for ancestor in &chain_top_down {
        let name_lower = ancestor.name.to_lowercase();
        for (mux_name, source_type) in MULTIPLEXERS {
            if name_lower == *mux_name || name_lower.starts_with(mux_name) {
                return source_type.clone();
            }
        }
    }

    // Tier 5: Cron.
    for ancestor in &chain_top_down {
        let name_lower = ancestor.name.to_lowercase();
        if CRON_NAMES.contains(&name_lower.as_str()) {
            return SourceType::Cron;
        }
    }

    // Tier 6: Shell — only if the direct parent is a shell.
    if chain.len() >= 2 {
        let direct_parent = &chain[1];
        let parent_lower = direct_parent.name.to_lowercase();
        if SHELLS.contains(&parent_lower.as_str()) {
            return SourceType::Shell;
        }
    }

    // On macOS, if chain terminates at launchd (PID 1), that's the source.
    if let Some(root) = chain.last() {
        if root.name == "launchd" {
            return SourceType::Launchd;
        }
    }

    SourceType::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_chain(names: &[(&str, u32)]) -> Vec<Ancestor> {
        let mut chain = Vec::new();
        for (i, (name, pid)) in names.iter().enumerate() {
            let ppid = if i + 1 < names.len() {
                names[i + 1].1
            } else {
                0
            };
            chain.push(Ancestor {
                pid: *pid,
                name: name.to_string(),
                ppid,
            });
        }
        chain
    }

    #[test]
    fn test_systemd_via_cgroup() {
        let chain = make_chain(&[("nginx", 500), ("bash", 100), ("systemd", 1)]);
        let cgroup = "0::/system.slice/nginx.service\n";
        assert_eq!(detect_source(&chain, Some(cgroup)), SourceType::Systemd);
    }

    #[test]
    fn test_systemd_bash_node_not_shell() {
        // Critical test: systemd -> bash -> node should be Systemd, not Shell
        let chain = make_chain(&[("node", 500), ("bash", 100), ("systemd", 1)]);
        let cgroup = "0::/system.slice/node-app.service\n";
        assert_eq!(detect_source(&chain, Some(cgroup)), SourceType::Systemd);
    }

    #[test]
    fn test_docker_via_cgroup() {
        let chain = make_chain(&[("node", 500), ("containerd-shim", 100), ("systemd", 1)]);
        let cgroup = "0::/docker/abc123def456\n";
        assert_eq!(detect_source(&chain, Some(cgroup)), SourceType::Docker);
    }

    #[test]
    fn test_docker_cgroup_beats_supervisor_name() {
        // Docker cgroup should win even if a supervisor name appears in chain
        let chain = make_chain(&[("gunicorn", 500), ("containerd-shim", 100), ("systemd", 1)]);
        let cgroup = "0::/docker/abc123\n";
        assert_eq!(detect_source(&chain, Some(cgroup)), SourceType::Docker);
    }

    #[test]
    fn test_pm2_supervisor() {
        let chain = make_chain(&[("node", 500), ("PM2", 100), ("systemd", 1)]);
        assert_eq!(detect_source(&chain, None), SourceType::Pm2);
    }

    #[test]
    fn test_supervisord() {
        let chain = make_chain(&[("myapp", 500), ("supervisord", 100), ("systemd", 1)]);
        assert_eq!(detect_source(&chain, None), SourceType::Supervisord);
    }

    #[test]
    fn test_gunicorn() {
        let chain = make_chain(&[
            ("worker", 501),
            ("gunicorn", 500),
            ("bash", 100),
            ("systemd", 1),
        ]);
        assert_eq!(detect_source(&chain, None), SourceType::Gunicorn);
    }

    #[test]
    fn test_tmux_multiplexer() {
        let chain = make_chain(&[
            ("node", 500),
            ("bash", 300),
            ("tmux: server", 200),
            ("systemd", 1),
        ]);
        assert_eq!(detect_source(&chain, None), SourceType::Tmux);
    }

    #[test]
    fn test_screen_multiplexer() {
        let chain = make_chain(&[
            ("python", 500),
            ("bash", 300),
            ("screen", 200),
            ("systemd", 1),
        ]);
        assert_eq!(detect_source(&chain, None), SourceType::Screen);
    }

    #[test]
    fn test_cron_detection() {
        let chain = make_chain(&[
            ("backup.sh", 500),
            ("sh", 400),
            ("cron", 100),
            ("systemd", 1),
        ]);
        assert_eq!(detect_source(&chain, None), SourceType::Cron);
    }

    #[test]
    fn test_shell_direct_parent() {
        let chain = make_chain(&[("node", 500), ("zsh", 100), ("init", 1)]);
        assert_eq!(detect_source(&chain, None), SourceType::Shell);
    }

    #[test]
    fn test_shell_not_detected_if_not_direct_parent() {
        // bash is grandparent, not direct parent — should be Unknown
        let chain = make_chain(&[("node", 500), ("wrapper", 200), ("bash", 100), ("init", 1)]);
        assert_eq!(detect_source(&chain, None), SourceType::Unknown);
    }

    #[test]
    fn test_launchd_fallback() {
        let chain = make_chain(&[("node", 500), ("launchd", 1)]);
        assert_eq!(detect_source(&chain, None), SourceType::Launchd);
    }

    #[test]
    fn test_empty_chain() {
        assert_eq!(detect_source(&[], None), SourceType::Unknown);
    }

    #[test]
    fn test_single_entry_chain() {
        let chain = make_chain(&[("node", 500)]);
        assert_eq!(detect_source(&chain, None), SourceType::Unknown);
    }

    #[test]
    fn test_runit_detection() {
        let chain = make_chain(&[
            ("myapp", 500),
            ("runsv", 200),
            ("runsvdir", 100),
            ("init", 1),
        ]);
        assert_eq!(detect_source(&chain, None), SourceType::Runit);
    }

    #[test]
    fn test_s6_detection() {
        let chain = make_chain(&[
            ("myapp", 500),
            ("s6-supervise", 200),
            ("s6-svscan", 100),
            ("init", 1),
        ]);
        assert_eq!(detect_source(&chain, None), SourceType::S6);
    }

    #[test]
    fn test_kubepods_cgroup() {
        let chain = make_chain(&[("app", 500)]);
        let cgroup = "0::/kubepods/burstable/pod123/container456\n";
        assert_eq!(detect_source(&chain, Some(cgroup)), SourceType::Docker);
    }
}
