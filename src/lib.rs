//! # ports
//!
//! Modern cross-platform port inspector. A clean replacement for `ss`, `netstat`, and `lsof`.
//!
//! ## Features
//!
//! - List listening ports (TCP/UDP, IPv4/IPv6)
//! - Show established connections
//! - Filter by port number, process name, or protocol
//! - Kill processes by port or name
//! - Interactive selection mode
//! - Watch mode with live updates
//! - JSON output for scripting
//! - Shell completions (bash, zsh, fish)
//!
//! ## Platform Support
//!
//! - **Linux**: Native `/proc/net` parsing for TCP, TCP6, UDP, UDP6
//! - **macOS**: Uses `lsof` for connections, `listeners` crate for listening ports
//! - **Others**: Generic fallback via `listeners` crate
//!
//! ## Library API
//!
//! This crate ships a binary (`ports`); the library exists only so that
//! `main.rs` can stay a thin shim. The surface is intentionally minimal —
//! only [`Cli`] and [`run`] are exported — and is **not** part of the
//! crate's public API for semver purposes. Internal modules are
//! crate-private and may change shape or disappear in any release.
//! Depending on `portls` as a library is not a supported integration.

pub(crate) mod ancestry;
pub(crate) mod cli;
pub(crate) mod commands;
#[cfg(feature = "docker")]
pub(crate) mod docker;
pub(crate) mod filter;
pub(crate) mod framework;
#[cfg(feature = "history")]
pub(crate) mod history;
#[cfg(feature = "tui")]
pub(crate) mod interactive;
pub(crate) mod output;
pub(crate) mod platform;
pub(crate) mod project;
#[cfg(feature = "tui")]
pub(crate) mod top;
pub(crate) mod types;
#[cfg(feature = "watch")]
pub(crate) mod watch;

pub use cli::Cli;

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::CommandFactory;
use clap_complete::{generate, Shell};

pub fn run(cli: Cli) -> Result<()> {
    if cli.interactive {
        return run_interactive(&cli);
    }

    if cli.watch {
        return run_watch(&cli);
    }

    match &cli.command {
        Some(cli::Commands::List) => commands::list::execute(
            cli.json,
            cli.connections,
            cli.sort,
            cli.protocol,
            cli.why,
            cli.dev,
        ),
        Some(cli::Commands::Kill {
            target,
            force,
            all,
            connections,
        }) => commands::kill::execute(target, *force, *all, *connections),
        Some(cli::Commands::Why { target }) => commands::why::execute(target, cli.json),
        Some(cli::Commands::Top { connections }) => run_top(*connections, cli.dev),
        Some(cli::Commands::Completions { shell, print }) => {
            if *print {
                let mut cmd = Cli::command();
                if matches!(shell, Shell::Fish) {
                    print!("{}", build_fish_completions(&mut cmd));
                } else {
                    generate(*shell, &mut cmd, "ports", &mut io::stdout());
                }
            } else {
                let installed = install_completions(*shell)?;
                eprintln!("Installed completions to {}", installed.path.display());
                if !installed.hint.is_empty() {
                    eprintln!("{}", installed.hint);
                }
            }
            Ok(())
        }
        Some(cli::Commands::History { action }) => run_history(action, cli.json),
        None => match &cli.query {
            Some(query) => commands::query::execute(
                query,
                cli.json,
                cli.connections,
                cli.sort,
                cli.protocol,
                cli.regex,
                cli.why,
                cli.dev,
            ),
            None => commands::list::execute(
                cli.json,
                cli.connections,
                cli.sort,
                cli.protocol,
                cli.why,
                cli.dev,
            ),
        },
    }
}

#[cfg(feature = "tui")]
fn run_interactive(cli: &Cli) -> Result<()> {
    use types::PortInfo;

    let ports = if cli.connections {
        platform::get_connections()?
    } else {
        platform::get_listening_ports()?
    };

    let mut ports = PortInfo::filter_protocol(ports, cli.protocol);
    if cli.dev {
        filter::retain_dev_only(&mut ports);
    }

    if let Some(query) = &cli.query {
        ports = PortInfo::filter_by_query(ports, query, cli.regex)?;
    }

    PortInfo::sort_vec(&mut ports, cli.sort);

    let ancestry_map = if cli.why {
        let pids_with_names: Vec<(u32, &str)> = ports
            .iter()
            .map(|p| (p.pid, p.process_name.as_str()))
            .collect();
        Some(ancestry::get_ancestry_batch(&pids_with_names))
    } else {
        None
    };

    interactive::select_and_kill(&ports, ancestry_map.as_ref())
}

#[cfg(not(feature = "tui"))]
fn run_interactive(_cli: &Cli) -> Result<()> {
    anyhow::bail!(
        "this binary was built without the `tui` feature; \
         rebuild with default features or `cargo install portls --features tui`"
    )
}

#[cfg(feature = "watch")]
fn run_watch(cli: &Cli) -> Result<()> {
    let filter = match &cli.command {
        Some(cli::Commands::List) => None,
        Some(cli::Commands::Kill { .. }) => {
            anyhow::bail!("Cannot use --watch with kill command");
        }
        Some(cli::Commands::Completions { .. }) => {
            anyhow::bail!("Cannot use --watch with completions command");
        }
        Some(cli::Commands::Top { .. }) => {
            anyhow::bail!("Cannot use --watch with top command (top has its own refresh)");
        }
        Some(cli::Commands::Why { .. }) => {
            anyhow::bail!("Cannot use --watch with why command");
        }
        Some(cli::Commands::History { .. }) => {
            anyhow::bail!("Cannot use --watch with history command");
        }
        None => cli.query.clone(),
    };

    watch::run(watch::WatchOptions {
        interval: std::time::Duration::from_secs_f64(cli.interval),
        json: cli.json,
        filter,
        connections: cli.connections,
        sort: cli.sort,
        protocol: cli.protocol,
        use_regex: cli.regex,
        why: cli.why,
        dev: cli.dev,
    })
}

#[cfg(not(feature = "watch"))]
fn run_watch(_cli: &Cli) -> Result<()> {
    anyhow::bail!(
        "this binary was built without the `watch` feature; \
         rebuild with default features or `cargo install portls --features watch`"
    )
}

#[cfg(feature = "tui")]
fn run_top(connections: bool, dev: bool) -> Result<()> {
    top::run(connections, dev)
}

#[cfg(not(feature = "tui"))]
fn run_top(_connections: bool, _dev: bool) -> Result<()> {
    anyhow::bail!(
        "this binary was built without the `tui` feature; \
         the `top` subcommand requires it. Rebuild with default features \
         or `cargo install portls --features tui`"
    )
}

#[cfg(feature = "history")]
fn run_history(action: &cli::HistoryAction, json: bool) -> Result<()> {
    match action {
        cli::HistoryAction::Record { connections } => commands::history::record(*connections, json),
        cli::HistoryAction::Show {
            port,
            process,
            hours,
            limit,
        } => commands::history::show(*port, process.clone(), Some(*hours), *limit, json),
        cli::HistoryAction::Timeline { port, hours } => {
            commands::history::timeline(*port, *hours, json)
        }
        cli::HistoryAction::Stats => commands::history::stats(json),
        cli::HistoryAction::Clean { keep } => commands::history::cleanup(*keep, json),
        cli::HistoryAction::Diff { ago } => commands::history::diff(*ago, json),
    }
}

#[cfg(not(feature = "history"))]
fn run_history(_action: &cli::HistoryAction, _json: bool) -> Result<()> {
    anyhow::bail!(
        "this binary was built without the `history` feature; \
         the `history` subcommand requires it. Rebuild with default features \
         or `cargo install portls --features history`"
    )
}

fn build_fish_completions(cmd: &mut clap::Command) -> String {
    let mut buf = Vec::new();
    generate(Shell::Fish, cmd, "ports", &mut buf);
    let body = String::from_utf8(buf).expect("clap_complete fish output is valid UTF-8");
    format!("complete -c ports -f\n{body}")
}

struct Installed {
    path: PathBuf,
    hint: String,
}

fn install_path_under(
    shell: Shell,
    home: &Path,
    xdg_config: Option<&Path>,
    xdg_data: Option<&Path>,
) -> Option<PathBuf> {
    match shell {
        Shell::Fish => {
            let base = xdg_config
                .map(Path::to_path_buf)
                .unwrap_or_else(|| home.join(".config"));
            Some(base.join("fish/completions/ports.fish"))
        }
        Shell::Bash => {
            let base = xdg_data
                .map(Path::to_path_buf)
                .unwrap_or_else(|| home.join(".local/share"));
            Some(base.join("bash-completion/completions/ports"))
        }
        Shell::Zsh => Some(home.join(".zsh/completions/_ports")),
        _ => None,
    }
}

fn install_path(shell: Shell) -> Result<PathBuf> {
    let home = dirs::home_dir().context("HOME is not set")?;
    let xdg_config = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute());
    let xdg_data = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute());
    install_path_under(shell, &home, xdg_config.as_deref(), xdg_data.as_deref()).with_context(
        || {
            format!(
                "auto-install of completions is not supported for {shell}. \
                 Use `ports completions {shell} --print` and redirect manually."
            )
        },
    )
}

fn install_completions(shell: Shell) -> Result<Installed> {
    let path = install_path(shell)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut cmd = Cli::command();
    let body = if matches!(shell, Shell::Fish) {
        build_fish_completions(&mut cmd)
    } else {
        let mut buf = Vec::new();
        generate(shell, &mut cmd, "ports", &mut buf);
        String::from_utf8(buf).expect("clap_complete output is valid UTF-8")
    };
    fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
    let hint = post_install_hint(shell, &path);
    Ok(Installed { path, hint })
}

fn post_install_hint(shell: Shell, path: &Path) -> String {
    let parent = path.parent().unwrap_or(path);
    match shell {
        Shell::Fish => format!("Restart your shell, or run: source {}", path.display()),
        Shell::Bash => format!(
            "Restart your shell to enable.\n\
             If `ports <TAB>` does not work, ensure bash-completion is installed \
             and that {} is on its lookup path. On macOS with Homebrew, you may \
             need to symlink to $(brew --prefix)/etc/bash_completion.d/ports.",
            parent.display()
        ),
        Shell::Zsh => format!(
            "If {} is not in your fpath, add to ~/.zshrc:\n\
             \x20 fpath=({} $fpath)\n\
             \x20 autoload -Uz compinit && compinit",
            parent.display(),
            parent.display(),
        ),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap_complete::Shell;
    use std::path::Path;

    #[test]
    fn fish_completions_suppress_files() {
        let mut cmd = Cli::command();
        let out = build_fish_completions(&mut cmd);
        assert!(
            out.starts_with("complete -c ports -f\n"),
            "fish completion must prepend `complete -c ports -f` so fish \
             doesn't fall back to file completion at the top level"
        );
        let body = out.trim_start_matches("complete -c ports -f\n");
        assert!(
            body.contains("kill"),
            "subcommand candidates must still appear after the prefix"
        );
    }

    #[test]
    fn install_path_under_fish_uses_dot_config() {
        let home = Path::new("/home/test");
        let path = install_path_under(Shell::Fish, home, None, None).unwrap();
        assert_eq!(
            path,
            Path::new("/home/test/.config/fish/completions/ports.fish")
        );
    }

    #[test]
    fn install_path_under_fish_respects_xdg_config_home() {
        let home = Path::new("/home/test");
        let xdg = Path::new("/custom/xdg");
        let path = install_path_under(Shell::Fish, home, Some(xdg), None).unwrap();
        assert_eq!(path, Path::new("/custom/xdg/fish/completions/ports.fish"));
    }

    #[test]
    fn install_path_under_fish_ignores_relative_xdg_config_home() {
        // install_path() filters non-absolute XDG values to None before
        // calling install_path_under; verify the under-fn falls back to
        // the home-relative default when xdg_config is None.
        let home = Path::new("/home/test");
        let path = install_path_under(Shell::Fish, home, None, None).unwrap();
        assert_eq!(
            path,
            Path::new("/home/test/.config/fish/completions/ports.fish")
        );
    }

    #[test]
    fn install_path_under_fish_ignores_empty_xdg_config_home() {
        let home = Path::new("/home/test");
        let path = install_path_under(Shell::Fish, home, None, None).unwrap();
        assert_eq!(
            path,
            Path::new("/home/test/.config/fish/completions/ports.fish")
        );
    }

    #[test]
    fn install_path_under_bash_uses_local_share() {
        let home = Path::new("/home/test");
        let path = install_path_under(Shell::Bash, home, None, None).unwrap();
        assert_eq!(
            path,
            Path::new("/home/test/.local/share/bash-completion/completions/ports")
        );
    }

    #[test]
    fn install_path_under_bash_respects_xdg_data_home() {
        let home = Path::new("/home/test");
        let xdg = Path::new("/custom/data");
        let path = install_path_under(Shell::Bash, home, None, Some(xdg)).unwrap();
        assert_eq!(
            path,
            Path::new("/custom/data/bash-completion/completions/ports")
        );
    }

    #[test]
    fn install_path_under_zsh_uses_dot_zsh() {
        let home = Path::new("/home/test");
        let path = install_path_under(Shell::Zsh, home, None, None).unwrap();
        assert_eq!(path, Path::new("/home/test/.zsh/completions/_ports"));
    }

    #[test]
    fn install_path_under_powershell_returns_none() {
        let home = Path::new("/home/test");
        assert!(install_path_under(Shell::PowerShell, home, None, None).is_none());
    }
}
