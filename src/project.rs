//! Project root detection utility.
//!
//! Walks up from a given directory looking for marker
//! files that indicate a project root (e.g. Cargo.toml,
//! package.json). Results are cached to avoid repeated
//! filesystem traversal.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

/// Marker files that indicate a project root.
const MARKERS: &[&str] = &[
    "package.json",
    "Cargo.toml",
    "go.mod",
    "pyproject.toml",
    "Gemfile",
    "pom.xml",
    "build.gradle",
];

/// Maximum ancestor levels to walk before giving up.
const MAX_DEPTH: usize = 15;

static PROJECT_ROOT_CACHE: LazyLock<Mutex<HashMap<PathBuf, Option<PathBuf>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Find the project root by walking up from `cwd`.
///
/// Checks each ancestor (up to [`MAX_DEPTH`] levels)
/// for any of the known [`MARKERS`]. Returns the
/// directory containing the first (nearest) marker
/// found, or `None` if no marker is found within the
/// depth limit.
pub fn find_project_root(cwd: &Path) -> Option<PathBuf> {
    let canonical = cwd.canonicalize().ok()?;

    let cache = PROJECT_ROOT_CACHE.lock().unwrap();
    if let Some(cached) = cache.get(&canonical) {
        return cached.clone();
    }
    drop(cache);

    let result = walk_ancestors(&canonical);

    let mut cache = PROJECT_ROOT_CACHE.lock().unwrap();
    cache.insert(canonical, result.clone());
    result
}

/// Clear the project root cache.
///
/// Called at the start of each watch/top refresh cycle so
/// that project root changes are picked up.
#[allow(dead_code)] // only used by watch/top features
pub fn clear_cache() {
    PROJECT_ROOT_CACHE.lock().unwrap().clear();
}

fn walk_ancestors(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    for _ in 0..MAX_DEPTH {
        let dir = current?;
        for marker in MARKERS {
            if dir.join(marker).exists() {
                return Some(dir.to_path_buf());
            }
        }
        current = dir.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_finds_cargo_toml() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let sub2 = root.join("sub1").join("sub2");
        fs::create_dir_all(&sub2).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]").unwrap();

        let result = find_project_root(&sub2);
        assert_eq!(result.unwrap(), root.canonicalize().unwrap());
    }

    #[test]
    fn test_finds_package_json() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let sub2 = root.join("sub1").join("sub2");
        fs::create_dir_all(&sub2).unwrap();
        fs::write(root.join("package.json"), "{}").unwrap();

        let result = find_project_root(&sub2);
        assert_eq!(result.unwrap(), root.canonicalize().unwrap());
    }

    #[test]
    fn test_returns_none_without_markers() {
        clear_cache();
        // Create 16 levels so the depth limit kicks in
        // before we escape the temp dir. This avoids
        // depending on the host filesystem having no
        // marker files above the temp directory.
        let tmp = TempDir::new().unwrap();
        let mut deep = tmp.path().to_path_buf();
        for i in 0..16 {
            deep = deep.join(format!("d{i}"));
        }
        fs::create_dir_all(&deep).unwrap();

        let result = find_project_root(&deep);
        assert!(result.is_none());
    }

    #[test]
    fn test_depth_limit() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Build 17 levels of nesting
        let mut deep = root.to_path_buf();
        for i in 0..17 {
            deep = deep.join(format!("d{i}"));
        }
        fs::create_dir_all(&deep).unwrap();

        // Place marker at root (>15 levels away)
        fs::write(root.join("Cargo.toml"), "[package]").unwrap();

        let result = find_project_root(&deep);
        assert!(
            result.is_none(),
            "should not find marker beyond \
             15 ancestor levels"
        );
    }

    #[test]
    fn test_prefers_nearest_marker() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let sub1 = root.join("sub1");
        let sub2 = sub1.join("sub2");
        fs::create_dir_all(&sub2).unwrap();

        // Marker further up
        fs::write(root.join("Cargo.toml"), "[package]").unwrap();
        // Marker closer
        fs::write(sub1.join("package.json"), "{}").unwrap();

        let result = find_project_root(&sub2);
        assert_eq!(
            result.unwrap(),
            sub1.canonicalize().unwrap(),
            "should return nearest marker dir"
        );
    }
}
