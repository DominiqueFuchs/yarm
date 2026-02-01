use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

use crate::state::State;
use crate::term::{print_success, print_warning};

/// Directories to skip during recursive scanning
const SKIP_DIRS: &[&str] = &["node_modules", "target", "vendor", "__pycache__", ".build"];

/// Executes the scan command flow
pub fn run() -> Result<()> {
    let config = crate::config::load()?;
    let pools = config.pool_paths();

    if pools.is_empty() {
        anyhow::bail!(
            "No repository pools configured.\n\
             Add pools to ~/.config/yarm.toml:\n\n\
             [repositories]\n\
             pools = [\"~/projects\", \"~/work\"]"
        );
    }

    let exclude = build_exclude_set(&config.repositories.exclude)?;

    let spinner = crate::term::spinner("");

    let mut repos = Vec::new();
    let mut pool_count = 0;

    for pool in &pools {
        if !pool.is_dir() {
            spinner.suspend(|| {
                print_warning(format!("Pool directory not found: {}", pool.display()));
            });
            continue;
        }

        pool_count += 1;
        spinner.set_message(format!("Scanning {}...", pool.display()));

        let found = scan_directory(pool, &exclude, config.repositories.max_depth);
        repos.extend(found);
    }

    spinner.finish_and_clear();

    if pool_count == 0 {
        anyhow::bail!("None of the configured pool directories exist");
    }

    repos.sort();
    repos.dedup();

    let mut state = State {
        repositories: repos.clone(),
        ..State::default()
    };
    state.mark_scanned();
    crate::state::save(&state)?;

    println!();
    let repo_label = if repos.len() == 1 {
        "repository"
    } else {
        "repositories"
    };
    let pool_label = if pool_count == 1 { "pool" } else { "pools" };
    print_success(format!(
        "Found {} {repo_label} across {pool_count} {pool_label}",
        repos.len()
    ));

    Ok(())
}

/// Builds a `GlobSet` from the configured exclude patterns.
fn build_exclude_set(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .with_context(|| format!("Invalid exclude pattern: {pattern}"))?;
        builder.add(glob);
    }
    builder.build().context("Failed to build exclude set")
}

/// Recursively scans a directory for git repositories.
/// Returns the paths of directories containing a `.git` subdirectory.
/// When `max_depth` is `Some(n)`, only directories up to `n` levels below the root are visited.
/// Depth 0 means only the root itself is checked; `None` means unlimited.
fn scan_directory(root: &Path, exclude: &GlobSet, max_depth: Option<u32>) -> Vec<PathBuf> {
    let mut repos = Vec::new();
    let mut stack: Vec<(PathBuf, u32)> = vec![(root.to_path_buf(), 0)];

    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };

        let mut is_repo = false;
        let mut subdirs = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();

            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };

            // .git can be a directory (regular repo) or a file (submodule/worktree)
            if name == ".git" {
                is_repo = true;
                break;
            }

            if !path.is_dir() {
                continue;
            }

            if name.starts_with('.') || SKIP_DIRS.contains(&name) {
                continue;
            }

            if let Ok(rel) = path.strip_prefix(root)
                && exclude.is_match(rel)
            {
                continue;
            }

            subdirs.push(path);
        }

        if is_repo {
            repos.push(dir);
        } else if max_depth.is_none_or(|limit| depth < limit) {
            stack.extend(subdirs.into_iter().map(|p| (p, depth + 1)));
        }
    }

    repos
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn empty_exclude() -> GlobSet {
        GlobSetBuilder::new().build().unwrap()
    }

    #[test]
    fn test_scan_finds_repos() {
        let tmp = tempdir("finds-repos");
        let repo_a = tmp.join("repo-a");
        let repo_b = tmp.join("repo-b");
        let not_repo = tmp.join("not-a-repo");

        fs::create_dir_all(repo_a.join(".git")).unwrap();
        fs::create_dir_all(repo_b.join(".git")).unwrap();
        fs::create_dir_all(&not_repo).unwrap();

        let mut repos = scan_directory(&tmp, &empty_exclude(), None);
        repos.sort();

        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0], repo_a);
        assert_eq!(repos[1], repo_b);
    }

    #[test]
    fn test_scan_skips_hidden_dirs() {
        let tmp = tempdir("skips-hidden");
        let visible = tmp.join("visible");
        let hidden = tmp.join(".hidden");

        fs::create_dir_all(visible.join(".git")).unwrap();
        fs::create_dir_all(hidden.join(".git")).unwrap();

        let repos = scan_directory(&tmp, &empty_exclude(), None);

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0], visible);
    }

    #[test]
    fn test_scan_skips_node_modules() {
        let tmp = tempdir("skips-nm");
        let real_repo = tmp.join("real-repo");
        let nm_repo = tmp.join("node_modules").join("some-pkg");

        fs::create_dir_all(real_repo.join(".git")).unwrap();
        fs::create_dir_all(nm_repo.join(".git")).unwrap();

        let repos = scan_directory(&tmp, &empty_exclude(), None);

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0], real_repo);
    }

    #[test]
    fn test_scan_nested_repos() {
        let tmp = tempdir("nested");
        let outer = tmp.join("org");
        let inner = outer.join("project");

        fs::create_dir_all(inner.join(".git")).unwrap();

        let repos = scan_directory(&tmp, &empty_exclude(), None);

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0], inner);
    }

    #[test]
    fn test_scan_detects_git_file() {
        let tmp = tempdir("git-file");
        let submodule = tmp.join("parent").join("sub");

        fs::create_dir_all(&submodule).unwrap();
        fs::write(submodule.join(".git"), "gitdir: ../../.git/modules/sub").unwrap();

        let repos = scan_directory(&tmp, &empty_exclude(), None);

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0], submodule);
    }

    #[test]
    fn test_scan_empty_directory() {
        let tmp = tempdir("empty");
        let repos = scan_directory(&tmp, &empty_exclude(), None);
        assert!(repos.is_empty());
    }

    #[test]
    fn test_scan_excludes_by_name() {
        let tmp = tempdir("exclude-name");
        let kept = tmp.join("kept");
        let excluded = tmp.join("build-output");

        fs::create_dir_all(kept.join(".git")).unwrap();
        fs::create_dir_all(excluded.join("nested-repo").join(".git")).unwrap();

        let exclude = build_exclude_set(&["build-output".to_string()]).unwrap();
        let repos = scan_directory(&tmp, &exclude, None);

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0], kept);
    }

    #[test]
    fn test_scan_excludes_by_glob() {
        let tmp = tempdir("exclude-glob");
        let kept = tmp.join("my-project");
        let excluded_a = tmp.join("foo-build");
        let excluded_b = tmp.join("bar-build");

        fs::create_dir_all(kept.join(".git")).unwrap();
        fs::create_dir_all(excluded_a.join("repo").join(".git")).unwrap();
        fs::create_dir_all(excluded_b.join("repo").join(".git")).unwrap();

        let exclude = build_exclude_set(&["*-build".to_string()]).unwrap();
        let repos = scan_directory(&tmp, &exclude, None);

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0], kept);
    }

    #[test]
    fn test_scan_excludes_nested_path() {
        let tmp = tempdir("exclude-nested");
        let kept = tmp.join("project").join("src");
        let excluded = tmp.join("project").join("external");

        fs::create_dir_all(kept.join(".git")).unwrap();
        fs::create_dir_all(excluded.join("dep").join(".git")).unwrap();

        let exclude = build_exclude_set(&["project/external".to_string()]).unwrap();
        let repos = scan_directory(&tmp, &exclude, None);

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0], kept);
    }

    #[test]
    fn test_scan_max_depth_zero_finds_root_repo() {
        let tmp = tempdir("depth-zero");
        fs::create_dir_all(tmp.join(".git")).unwrap();
        fs::create_dir_all(tmp.join("child").join(".git")).unwrap();

        let repos = scan_directory(&tmp, &empty_exclude(), Some(0));

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0], tmp);
    }

    #[test]
    fn test_scan_max_depth_limits_traversal() {
        let tmp = tempdir("depth-limit");
        // depth 1: org/repo-a
        let shallow = tmp.join("org").join("repo-a");
        // depth 2: org/group/repo-b
        let deep = tmp.join("org").join("group").join("repo-b");

        fs::create_dir_all(shallow.join(".git")).unwrap();
        fs::create_dir_all(deep.join(".git")).unwrap();

        let repos_limited = scan_directory(&tmp, &empty_exclude(), Some(2));
        assert_eq!(repos_limited.len(), 1);
        assert_eq!(repos_limited[0], shallow);

        let repos_unlimited = scan_directory(&tmp, &empty_exclude(), None);
        assert_eq!(repos_unlimited.len(), 2);
    }

    #[test]
    fn test_scan_max_depth_none_is_unlimited() {
        let tmp = tempdir("depth-unlimited");
        let deep = tmp.join("a").join("b").join("c").join("repo");
        fs::create_dir_all(deep.join(".git")).unwrap();

        let repos = scan_directory(&tmp, &empty_exclude(), None);

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0], deep);
    }

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("yarm-test-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
