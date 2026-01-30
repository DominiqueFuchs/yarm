use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::term::{eprint_hint, eprint_warning, format_home_path, SilentExit};

/// Executes the find command flow
pub fn run(repo: Option<&str>, pool: Option<&str>) -> Result<()> {
    if let Some(name) = pool {
        return find_pool(name);
    }

    let Some(repo) = repo else {
        anyhow::bail!("Provide a repository name or use --pool <name>");
    };

    let state = crate::state::load()?;

    if state.repositories.is_empty() {
        eprint_warning("No repositories in state");
        eprint_hint("Run `yarm scan` to discover repositories");
        return Err(SilentExit(1).into());
    }

    let matches = find_matches(&state.repositories, repo);

    match matches.len() {
        0 => {
            eprint_warning(format!("No repository matching '{repo}'"));
            if let Some(suggestion) = find_suggestion(&state.repositories, repo) {
                eprint_hint(format!("Did you mean '{suggestion}'?"));
            }
            return Err(SilentExit(1).into());
        }
        1 => {
            println!("{}", matches[0].display());
            Ok(())
        }
        _ => {
            eprint_warning(format!("Ambiguous match '{repo}', found {} repositories:", matches.len()));
            for m in &matches {
                eprintln!("  {}", format_home_path(m));
            }
            return Err(SilentExit(1).into());
        }
    }
}

/// Finds a repository pool by basename and prints its path.
fn find_pool(name: &str) -> Result<()> {
    let config = crate::config::load()?;
    let pools = config.pool_paths();

    if pools.is_empty() {
        eprint_warning("No repository pools configured");
        eprint_hint("Add pools to ~/.config/yarm.toml");
        return Err(SilentExit(1).into());
    }

    let name_lower = name.to_lowercase();
    let matches: Vec<_> = pools
        .iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.to_lowercase() == name_lower)
        })
        .collect();

    match matches.len() {
        0 => {
            eprint_warning(format!("No pool matching '{name}'"));
            eprint_hint("Configured pools:");
            for p in &pools {
                eprintln!("  {}", format_home_path(p));
            }
            return Err(SilentExit(1).into());
        }
        1 => {
            println!("{}", matches[0].display());
            Ok(())
        }
        _ => {
            eprint_warning(format!("Ambiguous pool name '{name}':"));
            for m in &matches {
                eprintln!("  {}", format_home_path(m));
            }
            return Err(SilentExit(1).into());
        }
    }
}

/// Prints repository basenames for shell completion (one per line).
pub fn complete_repo_names() -> Result<()> {
    let state = crate::state::load()?;
    let mut names: Vec<_> = state
        .repositories
        .iter()
        .filter_map(|r| r.file_name()?.to_str().map(String::from))
        .collect();
    names.sort();
    names.dedup();
    for name in &names {
        println!("{name}");
    }
    Ok(())
}

/// Prints pool basenames for shell completion (one per line).
pub fn complete_pool_names() -> Result<()> {
    let config = crate::config::load()?;
    for pool in config.pool_paths() {
        if let Some(name) = pool.file_name().and_then(|n| n.to_str()) {
            println!("{name}");
        }
    }
    Ok(())
}

/// Resolves a name-or-path argument to a repository path.
/// Tries state-based name lookup first, then filesystem path.
pub(crate) fn resolve_repo(name_or_path: &str) -> Result<PathBuf> {
    let state = crate::state::load()?;

    if !state.repositories.is_empty() {
        let matches = find_matches(&state.repositories, name_or_path);
        if matches.len() == 1 {
            return Ok(matches.into_iter().next().unwrap());
        }
    }

    let path = PathBuf::from(name_or_path);
    let path = if path.is_relative() {
        std::env::current_dir()
            .context("Failed to get current directory")?
            .join(&path)
    } else {
        path
    };

    let path = path
        .canonicalize()
        .with_context(|| format!("Path not found: {name_or_path}"))?;

    if path.join(".git").exists() {
        return Ok(path);
    }

    bail!("'{name_or_path}' is not a known repository name or a valid git repo path");
}

/// Finds repositories matching the query.
/// Tries exact basename match first, then falls back to suffix matching.
fn find_matches(repos: &[PathBuf], query: &str) -> Vec<PathBuf> {
    let query_lower = query.to_lowercase();
    let query_components: Vec<&str> = query.split('/').collect();

    let exact: Vec<_> = repos
        .iter()
        .filter(|r| {
            r.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.to_lowercase() == query_lower)
        })
        .cloned()
        .collect();

    if !exact.is_empty() {
        return exact;
    }

    // Suffix match on path components
    repos
        .iter()
        .filter(|r| path_suffix_matches(r, &query_components))
        .cloned()
        .collect()
}

/// Checks if the path ends with the given component sequence (case-insensitive).
fn path_suffix_matches(path: &Path, query_components: &[&str]) -> bool {
    let path_components: Vec<String> = path
        .components()
        .filter_map(|c| c.as_os_str().to_str().map(str::to_lowercase))
        .collect();

    if query_components.len() > path_components.len() {
        return false;
    }

    let start = path_components.len() - query_components.len();
    path_components[start..]
        .iter()
        .zip(query_components.iter())
        .all(|(p, q)| p == &q.to_lowercase())
}

/// Maximum edit distance to consider a basename as a suggestion.
const MAX_EDIT_DISTANCE: usize = 3;

/// Finds the closest repository basename to the query using edit distance.
fn find_suggestion(repos: &[PathBuf], query: &str) -> Option<String> {
    let query_lower = query.to_lowercase();
    repos
        .iter()
        .filter_map(|r| {
            let name = r.file_name()?.to_str()?;
            let dist = strsim::levenshtein(&query_lower, &name.to_lowercase());
            (dist > 0 && dist <= MAX_EDIT_DISTANCE).then(|| (dist, name.to_string()))
        })
        .min_by_key(|(dist, _)| *dist)
        .map(|(_, name)| name)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn repos() -> Vec<PathBuf> {
        vec![
            PathBuf::from("/home/user/projects/yarm"),
            PathBuf::from("/home/user/projects/other"),
            PathBuf::from("/home/user/work/yarm"),
            PathBuf::from("/home/user/Source/OSS/kfoo"),
        ]
    }

    #[test]
    fn test_exact_basename_single() {
        let matches = find_matches(&repos(), "other");
        assert_eq!(matches, vec![PathBuf::from("/home/user/projects/other")]);
    }

    #[test]
    fn test_exact_basename_multiple() {
        let matches = find_matches(&repos(), "yarm");
        assert_eq!(matches.len(), 2);
        assert!(matches.contains(&PathBuf::from("/home/user/projects/yarm")));
        assert!(matches.contains(&PathBuf::from("/home/user/work/yarm")));
    }

    #[test]
    fn test_exact_basename_case_insensitive() {
        let matches = find_matches(&repos(), "YARM");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_suffix_match() {
        let matches = find_matches(&repos(), "work/yarm");
        assert_eq!(matches, vec![PathBuf::from("/home/user/work/yarm")]);
    }

    #[test]
    fn test_suffix_match_case_insensitive() {
        let matches = find_matches(&repos(), "oss/kfoo");
        assert_eq!(matches, vec![PathBuf::from("/home/user/Source/OSS/kfoo")]);
    }

    #[test]
    fn test_no_match() {
        let matches = find_matches(&repos(), "nonexistent");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_suffix_too_long() {
        let matches = find_matches(&repos(), "a/b/c/d/e/f/g");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_suggestion_typo() {
        assert_eq!(find_suggestion(&repos(), "yram"), Some("yarm".to_string()));
    }

    #[test]
    fn test_suggestion_partial() {
        assert_eq!(find_suggestion(&repos(), "yar"), Some("yarm".to_string()));
    }

    #[test]
    fn test_suggestion_no_close_match() {
        assert_eq!(find_suggestion(&repos(), "zzzzzzzzz"), None);
    }

    #[test]
    fn test_suggestion_exact_excluded() {
        // Exact match (distance 0) should not be suggested
        assert_eq!(find_suggestion(&repos(), "yarm"), None);
    }

    #[test]
    fn test_path_suffix_matches_basic() {
        let path = PathBuf::from("/home/user/Source/OSS/yarm");
        assert!(path_suffix_matches(&path, &["yarm"]));
        assert!(path_suffix_matches(&path, &["OSS", "yarm"]));
        assert!(path_suffix_matches(&path, &["oss", "yarm"]));
        assert!(!path_suffix_matches(&path, &["projects", "yarm"]));
    }
}
