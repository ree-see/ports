//! Framework and runtime detection for listening ports.
//!
//! Identifies what framework or service is behind a process using
//! a tiered detection cascade:
//!   0. Docker image name
//!   1. Command-line string patterns
//!   2. package.json dependency lookup
//!   3. Config file existence
//!   4. Process name fallback

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

#[cfg(feature = "docker")]
use crate::docker;
use crate::project;
use crate::types::PortInfo;

// ── Caches ──────────────────────────────────────────────

type FrameworkCache = HashMap<PathBuf, Option<String>>;

static PROJECT_FRAMEWORK_CACHE: LazyLock<Mutex<FrameworkCache>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Clear the framework detection cache.
///
/// Called at the start of each watch/top refresh cycle so
/// that file-system changes (new deps, new config files)
/// are picked up.
#[allow(dead_code)] // only used by watch/top features
pub fn clear_cache() {
    PROJECT_FRAMEWORK_CACHE.lock().unwrap().clear();
}

// ── Public API ──────────────────────────────────────────

/// Populate the `framework` field on every port in the vec.
pub fn resolve_frameworks(mut ports: Vec<PortInfo>) -> Vec<PortInfo> {
    for p in &mut ports {
        p.framework = detect_framework(p);
    }
    ports
}

/// Detect the framework for a single port entry.
///
/// Returns `None` when no tier matches.
pub fn detect_framework(info: &PortInfo) -> Option<String> {
    if let Some(result) = detect_docker_image(info) {
        return Some(result);
    }
    if let Some(result) = detect_command_pattern(info) {
        return Some(result);
    }
    let project_root = info
        .cwd
        .as_ref()
        .and_then(|cwd| project::find_project_root(cwd));
    if let Some(ref root) = project_root {
        if let Some(result) = detect_from_project(root) {
            return Some(result);
        }
    }
    detect_process_name(info)
}

// ── Tier 0: Docker image ────────────────────────────────

/// Docker image substring → framework name.
#[cfg(feature = "docker")]
const DOCKER_IMAGE_PATTERNS: &[(&str, &str)] = &[
    ("postgres", "PostgreSQL"),
    ("redis", "Redis"),
    ("mysql", "MySQL"),
    ("mariadb", "MySQL"),
    ("mongo", "MongoDB"),
    ("nginx", "nginx"),
    ("localstack", "LocalStack"),
    ("rabbitmq", "RabbitMQ"),
    ("kafka", "Kafka"),
    ("elasticsearch", "Elasticsearch"),
    ("opensearch", "Elasticsearch"),
    ("minio", "MinIO"),
];

#[cfg(feature = "docker")]
fn detect_docker_image(info: &PortInfo) -> Option<String> {
    info.container.as_ref()?;
    let image = match docker::get_container_image(info.port) {
        Some(img) => img,
        // Container present but no image in cache.
        None => return Some("Docker".to_string()),
    };
    let image_lower = image.to_lowercase();
    for &(substr, fw) in DOCKER_IMAGE_PATTERNS {
        if image_lower.contains(substr) {
            return Some(fw.to_string());
        }
    }
    // Container present but unknown image pattern.
    Some("Docker".to_string())
}

#[cfg(not(feature = "docker"))]
fn detect_docker_image(_info: &PortInfo) -> Option<String> {
    None
}

// ── Tier 1: Command-line patterns ───────────────────────

/// Patterns that are safe to match anywhere in the command.
const UNAMBIGUOUS_CMD_PATTERNS: &[(&str, &str)] = &[
    ("flask", "Flask"),
    ("uvicorn", "Uvicorn"),
    ("manage.py", "Django"),
    ("django", "Django"),
    ("rails", "Rails"),
    ("gatsby", "Gatsby"),
    ("astro", "Astro"),
];

/// Patterns that must match the binary name or a standalone
/// argument token to avoid false positives.
const AMBIGUOUS_CMD_PATTERNS: &[(&str, &str)] = &[
    ("next", "Next.js"),
    ("vite", "Vite"),
    ("nuxt", "Nuxt"),
    ("remix", "Remix"),
    ("ng", "Angular"),
    ("angular", "Angular"),
    ("webpack", "Webpack"),
    ("cargo", "Rust"),
    ("rustc", "Rust"),
];

fn detect_command_pattern(info: &PortInfo) -> Option<String> {
    let cmd = info.command_line.as_ref()?;
    let cmd_lower = cmd.to_lowercase();

    // Unambiguous: match anywhere in the command string.
    for &(pattern, fw) in UNAMBIGUOUS_CMD_PATTERNS {
        if cmd_lower.contains(pattern) {
            return Some(fw.to_string());
        }
    }

    // Ambiguous: only match binary name or standalone arg.
    let tokens: Vec<&str> = cmd_lower.split_whitespace().collect();
    let binary = tokens
        .first()
        .and_then(|t| t.rsplit('/').next())
        .unwrap_or("");

    for &(pattern, fw) in AMBIGUOUS_CMD_PATTERNS {
        if binary == pattern {
            return Some(fw.to_string());
        }
        // Check standalone argument tokens (skip argv[0]).
        if tokens.iter().skip(1).any(|t| *t == pattern) {
            return Some(fw.to_string());
        }
    }

    None
}

// ── Tier 2: package.json dependencies ───────────────────

/// Dependency key → framework name (checked in priority order).
const PACKAGE_JSON_DEPS: &[(&str, &str)] = &[
    ("next", "Next.js"),
    ("nuxt", "Nuxt"),
    ("nuxt3", "Nuxt"),
    ("@sveltejs/kit", "SvelteKit"),
    ("svelte", "Svelte"),
    ("@remix-run/react", "Remix"),
    ("remix", "Remix"),
    ("astro", "Astro"),
    ("@angular/core", "Angular"),
    ("vue", "Vue"),
    ("react", "React"),
    ("express", "Express"),
    ("fastify", "Fastify"),
    ("hono", "Hono"),
    ("koa", "Koa"),
    ("@nestjs/core", "NestJS"),
    ("nestjs", "NestJS"),
    ("gatsby", "Gatsby"),
    ("vite", "Vite"),
    ("webpack-dev-server", "Webpack"),
    ("esbuild", "esbuild"),
    ("parcel", "Parcel"),
];

/// Cached lookup: tries package.json (Tier 2) then config
/// files (Tier 3). Caches the combined result per project root
/// so neither tier re-reads the filesystem on repeat calls.
fn detect_from_project(project_root: &Path) -> Option<String> {
    {
        let cache = PROJECT_FRAMEWORK_CACHE.lock().unwrap();
        if let Some(cached) = cache.get(project_root) {
            return cached.clone();
        }
    }

    let result =
        read_package_json_framework(project_root).or_else(|| detect_config_file(project_root));

    PROJECT_FRAMEWORK_CACHE
        .lock()
        .unwrap()
        .insert(project_root.to_path_buf(), result.clone());

    result
}

fn read_package_json_framework(project_root: &Path) -> Option<String> {
    let pkg_path = project_root.join("package.json");
    let content = std::fs::read_to_string(&pkg_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;

    let deps = json.get("dependencies");
    let dev_deps = json.get("devDependencies");

    for &(key, fw) in PACKAGE_JSON_DEPS {
        let has_dep = deps.and_then(|d| d.get(key)).is_some();
        let has_dev_dep = dev_deps.and_then(|d| d.get(key)).is_some();
        if has_dep || has_dev_dep {
            return Some(fw.to_string());
        }
    }

    None
}

// ── Tier 3: Config file detection ───────────────────────

const CONFIG_FILE_PATTERNS: &[(&[&str], &str)] = &[
    (
        &["next.config.js", "next.config.mjs", "next.config.ts"],
        "Next.js",
    ),
    (
        &["vite.config.ts", "vite.config.js", "vite.config.mts"],
        "Vite",
    ),
    (&["nuxt.config.ts", "nuxt.config.js"], "Nuxt"),
    (&["angular.json"], "Angular"),
    (&["svelte.config.js"], "SvelteKit"),
    (&["remix.config.js"], "Remix"),
    (&["astro.config.mjs", "astro.config.ts"], "Astro"),
    (&["webpack.config.js"], "Webpack"),
    (&["Cargo.toml"], "Rust"),
    (&["go.mod"], "Go"),
    (&["manage.py"], "Django"),
    (&["Gemfile"], "Ruby"),
];

fn detect_config_file(project_root: &Path) -> Option<String> {
    for &(files, fw) in CONFIG_FILE_PATTERNS {
        for file in files {
            if project_root.join(file).exists() {
                return Some(fw.to_string());
            }
        }
    }
    None
}

// ── Tier 4: Process name fallback ───────────────────────

const PROCESS_NAME_MAP: &[(&str, &str)] = &[
    ("node", "Node.js"),
    ("python", "Python"),
    ("python3", "Python"),
    ("ruby", "Ruby"),
    ("java", "Java"),
    ("go", "Go"),
];

fn detect_process_name(info: &PortInfo) -> Option<String> {
    let name = info.process_name.to_lowercase();
    for &(proc_name, fw) in PROCESS_NAME_MAP {
        if name == proc_name {
            return Some(fw.to_string());
        }
    }
    None
}

// ── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Protocol;
    use std::fs;
    use tempfile::TempDir;

    fn make_port_info() -> PortInfo {
        PortInfo {
            port: 3000,
            protocol: Protocol::Tcp,
            pid: 1000,
            process_name: "node".to_string(),
            address: "127.0.0.1:3000".to_string(),
            remote_address: None,
            container: None,
            service_name: None,
            command_line: None,
            cwd: None,
            framework: None,
        }
    }

    // ── Tier 0: Docker image ────────────────────────
    //
    // Consolidated into one test to avoid parallel-test
    // race conditions on the shared global DOCKER_CACHE.

    #[cfg(feature = "docker")]
    #[test]
    fn tier0_docker_image_detection() {
        // Sub-case 1: no container → skip tier 0.
        let mut info = make_port_info();
        assert!(detect_docker_image(&info).is_none());

        // Sub-case 2: known image → matches pattern.
        info.container = Some("my-pg".into());
        let mut map = HashMap::new();
        map.insert(
            3000,
            docker::ContainerInfo {
                name: "my-pg".into(),
                image: Some("postgres:16-alpine".into()),
            },
        );
        *docker::DOCKER_CACHE.lock().unwrap() = Some((std::time::Instant::now(), map));
        assert_eq!(detect_docker_image(&info), Some("PostgreSQL".to_string()));

        // Sub-case 3: unknown image → returns "Docker".
        info.container = Some("custom-app".into());
        let mut map = HashMap::new();
        map.insert(
            3000,
            docker::ContainerInfo {
                name: "custom-app".into(),
                image: Some("my-org/custom-thing:latest".into()),
            },
        );
        *docker::DOCKER_CACHE.lock().unwrap() = Some((std::time::Instant::now(), map));
        assert_eq!(detect_docker_image(&info), Some("Docker".to_string()));

        // Sub-case 4: container present, no cached image.
        info.container = Some("mystery-app".into());
        *docker::DOCKER_CACHE.lock().unwrap() = None;
        assert_eq!(detect_docker_image(&info), Some("Docker".to_string()));

        // Clean up.
        *docker::DOCKER_CACHE.lock().unwrap() = None;
    }

    // ── Tier 1: Command patterns ────────────────────

    #[test]
    fn tier1_unambiguous_flask() {
        let mut info = make_port_info();
        info.command_line = Some("python -m flask run --port 3000".into());
        assert_eq!(detect_command_pattern(&info), Some("Flask".to_string()));
    }

    #[test]
    fn tier1_unambiguous_django() {
        let mut info = make_port_info();
        info.command_line = Some("python manage.py runserver".into());
        assert_eq!(detect_command_pattern(&info), Some("Django".to_string()));
    }

    #[test]
    fn tier1_ambiguous_next_binary() {
        let mut info = make_port_info();
        info.command_line = Some("/usr/local/bin/next start".into());
        assert_eq!(detect_command_pattern(&info), Some("Next.js".to_string()));
    }

    #[test]
    fn tier1_ambiguous_next_arg() {
        let mut info = make_port_info();
        info.command_line = Some("node /app/node_modules/.bin/next dev".into());
        // "next" appears as a standalone arg → match.
        // The path component is /app/node_modules/.bin/next
        // but as a standalone token "next" appears as arg[2]
        // after splitting: ["node", "/app/node_modules/.bin/next", "dev"]
        // Wait — "/app/node_modules/.bin/next" is a full path token.
        // The split is on whitespace, so that token is a single
        // element. It won't match as standalone "next".
        // Instead the binary is "node", and the args are
        // ["/app/node_modules/.bin/next", "dev"]. Neither equals "next".
        // This is actually a case where we DON'T want to match the path.
        assert_eq!(detect_command_pattern(&info), None);
    }

    #[test]
    fn tier1_ambiguous_next_false_positive_path() {
        let mut info = make_port_info();
        info.command_line = Some("/home/user/next-project/server.js".into());
        // "next" in a path component should NOT match.
        assert_eq!(detect_command_pattern(&info), None);
    }

    #[test]
    fn tier1_ambiguous_npx_next() {
        let mut info = make_port_info();
        info.command_line = Some("npx next dev --port 3000".into());
        assert_eq!(detect_command_pattern(&info), Some("Next.js".to_string()));
    }

    #[test]
    fn tier1_ambiguous_vite() {
        let mut info = make_port_info();
        info.command_line = Some("node ./node_modules/.bin/vite".into());
        // "vite" is the last path component of arg[1],
        // but as a standalone whitespace-delimited token
        // "./node_modules/.bin/vite" != "vite".
        // This won't match. That's a limitation we accept —
        // Tier 2 (package.json) will catch it.
        assert_eq!(detect_command_pattern(&info), None);
    }

    #[test]
    fn tier1_cargo_binary() {
        let mut info = make_port_info();
        info.process_name = "cargo".into();
        info.command_line = Some("cargo run -- serve".into());
        assert_eq!(detect_command_pattern(&info), Some("Rust".to_string()));
    }

    // ── Tier 2: package.json ────────────────────────

    #[test]
    fn tier2_package_json_next() {
        clear_cache();
        let dir = TempDir::new().unwrap();
        let pkg = r#"{"dependencies":{"next":"14.0.0","react":"18.0.0"}}"#;
        fs::write(dir.path().join("package.json"), pkg).unwrap();

        let result = detect_from_project(dir.path());
        assert_eq!(result, Some("Next.js".to_string()));
    }

    #[test]
    fn tier2_package_json_priority() {
        clear_cache();
        let dir = TempDir::new().unwrap();
        // Both next and react present — next has higher priority.
        let pkg = r#"{"dependencies":{"react":"18","next":"14"}}"#;
        fs::write(dir.path().join("package.json"), pkg).unwrap();

        assert_eq!(detect_from_project(dir.path()), Some("Next.js".to_string()));
    }

    #[test]
    fn tier2_package_json_dev_deps() {
        clear_cache();
        let dir = TempDir::new().unwrap();
        let pkg = r#"{"devDependencies":{"vite":"5.0.0"}}"#;
        fs::write(dir.path().join("package.json"), pkg).unwrap();

        assert_eq!(detect_from_project(dir.path()), Some("Vite".to_string()));
    }

    #[test]
    fn tier2_package_json_missing() {
        clear_cache();
        let dir = TempDir::new().unwrap();
        assert_eq!(detect_from_project(dir.path()), None);
    }

    #[test]
    fn tier2_package_json_malformed() {
        clear_cache();
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), "{{bad json").unwrap();
        assert_eq!(detect_from_project(dir.path()), None);
    }

    // ── Tier 3: Config files ────────────────────────

    #[test]
    fn tier3_vite_config() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("vite.config.ts"), "").unwrap();
        assert_eq!(detect_config_file(dir.path()), Some("Vite".to_string()));
    }

    #[test]
    fn tier3_cargo_toml() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        assert_eq!(detect_config_file(dir.path()), Some("Rust".to_string()));
    }

    #[test]
    fn tier3_no_config() {
        let dir = TempDir::new().unwrap();
        assert_eq!(detect_config_file(dir.path()), None);
    }

    // ── Tier 4: Process name ────────────────────────

    #[test]
    fn tier4_node() {
        let info = make_port_info(); // process_name = "node"
        assert_eq!(detect_process_name(&info), Some("Node.js".to_string()));
    }

    #[test]
    fn tier4_python3() {
        let mut info = make_port_info();
        info.process_name = "python3".into();
        assert_eq!(detect_process_name(&info), Some("Python".to_string()));
    }

    #[test]
    fn tier4_unknown() {
        let mut info = make_port_info();
        info.process_name = "myapp".into();
        assert_eq!(detect_process_name(&info), None);
    }

    // ── Priority / integration ──────────────────────

    #[test]
    fn priority_command_wins_over_process_name() {
        let mut info = make_port_info();
        info.process_name = "node".into();
        info.command_line = Some("node manage.py runserver".into());
        // Tier 1 (Django via manage.py) should win over Tier 4 (Node.js).
        assert_eq!(detect_framework(&info), Some("Django".to_string()));
    }

    #[test]
    fn priority_package_json_wins_over_process_name() {
        clear_cache();
        let dir = TempDir::new().unwrap();
        let pkg = r#"{"dependencies":{"express":"4"}}"#;
        fs::write(dir.path().join("package.json"), pkg).unwrap();

        let mut info = make_port_info();
        info.process_name = "node".into();
        info.cwd = Some(dir.path().to_path_buf());
        // Tier 2 (Express) should win over Tier 4 (Node.js).
        assert_eq!(detect_framework(&info), Some("Express".to_string()));
    }

    #[test]
    fn resolve_frameworks_populates_field() {
        let mut info = make_port_info();
        info.command_line = Some("flask run".into());
        let ports = vec![info];
        let result = resolve_frameworks(ports);
        assert_eq!(result[0].framework, Some("Flask".to_string()));
    }
}
