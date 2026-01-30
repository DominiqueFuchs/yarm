use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{Duration, SystemTime};

use anyhow::Result;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};

use crate::git;
use crate::term::{format_elapsed, print_header, print_warning};

/// Executes the stat command flow
pub fn run(repo: Option<String>) -> Result<()> {
    git::ensure_available()?;

    let repo_path = resolve_target(repo)?;
    let display_name = repo_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    print_header("Repository:", display_name);
    println!();

    let branch = git::current_branch(&repo_path)?;
    let remotes = git::remotes(&repo_path)?;
    let dirty = git::is_dirty(&repo_path)?;
    let fetch_time = last_fetch_time(&repo_path);

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("  {spinner:.cyan} {msg}")
            .expect("valid template"),
    );
    spinner.enable_steady_tick(Duration::from_millis(80));
    spinner.set_message("Calculating size...");

    let (total_size, file_count, dir_count) = dir_stats(&repo_path);

    spinner.finish_and_clear();

    print_field("Branch:", &branch);
    if remotes.is_empty() {
        print_field("Remotes:", &style("(none)").dim().to_string());
    } else {
        for (i, (name, url)) in remotes.iter().enumerate() {
            let label = if i == 0 {
                "Remotes:".to_string()
            } else {
                String::new()
            };
            print_field(&label, &format!("{} {}", style(name).cyan(), url));
        }
    }
    print_field(
        "Status:",
        &if dirty {
            style("dirty").yellow().to_string()
        } else {
            style("clean").green().to_string()
        },
    );

    print_field(
        "Size:",
        &format!(
            "{} ({} files, {} directories)",
            format_size(total_size),
            format_count(file_count),
            format_count(dir_count)
        ),
    );
    print_field(
        "Last fetch:",
        &match fetch_time {
            Some(t) => format_elapsed(t),
            None => style("(never)").dim().to_string(),
        },
    );

    Ok(())
}

fn resolve_target(repo: Option<String>) -> Result<PathBuf> {
    match repo {
        None => {
            let cwd = std::env::current_dir()?;
            if !cwd.join(".git").exists() {
                print_warning(format!("Not a git repository: {}", cwd.display()));
                process::exit(1);
            }
            Ok(cwd)
        }
        Some(name_or_path) => match super::find::resolve_repo(&name_or_path) {
            Ok(path) => Ok(path),
            Err(_) => {
                print_warning(format!(
                    "'{name_or_path}' is not a known repository name or a valid git repo path"
                ));
                process::exit(1);
            }
        },
    }
}

fn print_field(label: &str, value: &str) {
    println!("    {:<14}{value}", style(label).bold());
}

fn last_fetch_time(repo: &Path) -> Option<SystemTime> {
    // FETCH_HEAD is written by `git fetch` and `git pull`, but not by `git clone`.
    // Fall back to .git/HEAD mtime which is set during clone and on checkout/fetch.
    let candidates = [".git/FETCH_HEAD", ".git/HEAD"];
    candidates
        .iter()
        .filter_map(|f| fs::metadata(repo.join(f)).ok()?.modified().ok())
        .next()
}

fn dir_stats(path: &Path) -> (u64, u64, u64) {
    let mut total: u64 = 0;
    let mut files: u64 = 0;
    let mut dirs: u64 = 0;
    let mut stack = vec![path.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                dirs += 1;
                stack.push(entry.path());
            } else {
                total += meta.len();
                files += 1;
            }
        }
    }

    (total, files, dirs)
}

#[allow(clippy::cast_precision_loss)]
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[allow(clippy::cast_precision_loss)]
fn format_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{},{:03}", n / 1000, n % 1000)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
    }

    #[test]
    fn test_format_size_kb() {
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
    }

    #[test]
    fn test_format_size_mb() {
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(45 * 1024 * 1024), "45.0 MB");
    }

    #[test]
    fn test_format_size_gb() {
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn test_format_count_small() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(42), "42");
        assert_eq!(format_count(999), "999");
    }

    #[test]
    fn test_format_count_thousands() {
        assert_eq!(format_count(1000), "1,000");
        assert_eq!(format_count(1847), "1,847");
        assert_eq!(format_count(42_000), "42,000");
    }

    #[test]
    fn test_format_count_millions() {
        assert_eq!(format_count(1_000_000), "1.0M");
        assert_eq!(format_count(2_500_000), "2.5M");
    }
}
