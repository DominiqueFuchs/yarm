use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};

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

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("  {spinner:.cyan} {msg}")
            .expect("valid template"),
    );
    spinner.enable_steady_tick(Duration::from_millis(80));

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

        let found = scan_directory(pool);
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

/// Recursively scans a directory for git repositories.
/// Returns the paths of directories containing a `.git` subdirectory.
fn scan_directory(root: &Path) -> Vec<PathBuf> {
    let mut repos = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
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

            if !name.starts_with('.') && !SKIP_DIRS.contains(&name) {
                subdirs.push(path);
            }
        }

        if is_repo {
            repos.push(dir);
        } else {
            stack.extend(subdirs);
        }
    }

    repos
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_scan_finds_repos() {
        let tmp = tempdir("finds-repos");
        let repo_a = tmp.join("repo-a");
        let repo_b = tmp.join("repo-b");
        let not_repo = tmp.join("not-a-repo");

        fs::create_dir_all(repo_a.join(".git")).unwrap();
        fs::create_dir_all(repo_b.join(".git")).unwrap();
        fs::create_dir_all(&not_repo).unwrap();

        let mut repos = scan_directory(&tmp);
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

        let repos = scan_directory(&tmp);

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

        let repos = scan_directory(&tmp);

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0], real_repo);
    }

    #[test]
    fn test_scan_nested_repos() {
        let tmp = tempdir("nested");
        let outer = tmp.join("org");
        let inner = outer.join("project");

        fs::create_dir_all(inner.join(".git")).unwrap();

        let repos = scan_directory(&tmp);

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0], inner);
    }

    #[test]
    fn test_scan_detects_git_file() {
        let tmp = tempdir("git-file");
        let submodule = tmp.join("parent").join("sub");

        fs::create_dir_all(&submodule).unwrap();
        fs::write(submodule.join(".git"), "gitdir: ../../.git/modules/sub").unwrap();

        let repos = scan_directory(&tmp);

        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0], submodule);
    }

    #[test]
    fn test_scan_empty_directory() {
        let tmp = tempdir("empty");
        let repos = scan_directory(&tmp);
        assert!(repos.is_empty());
    }

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("yarm-test-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
