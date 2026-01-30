use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;

use anyhow::{Context, Result};

use crate::git;
use crate::profile::{apply_profile, resolve_profile_with_context, ProfileContext};
use crate::term::{print_header, print_success};

/// Executes the clone command flow
pub fn run(url: &str, path: Option<PathBuf>, profile_name: Option<&str>) -> Result<()> {
    git::ensure_available()?;

    let target = path.unwrap_or_else(|| derive_target_from_url(url));

    if target.exists() {
        anyhow::bail!(
            "Target directory '{}' already exists",
            target.display()
        );
    }

    print_header("Cloning:", extract_repo_display_name(url));
    println!();

    let context = ProfileContext::new(target.clone(), Some(url.to_string()));
    let Some(selected) = resolve_profile_with_context(profile_name, &context)? else {
        return Ok(());
    };

    clone_repo(url, &target)?;

    apply_profile(&target, &selected)?;

    print_success(format!("Cloned to {}", target.display()));
    print_success(format!(
        "Applied profile '{}' ({})",
        selected.name,
        selected.config_summary()
    ));

    Ok(())
}

/// Extracts repo name from URL for display
fn extract_repo_display_name(url: &str) -> String {
    let url = url.trim_end_matches(".git");

    // Handle SSH format: git@github.com:owner/repo
    if let Some(colon_pos) = url.find(':')
        && url[..colon_pos].contains('@')
    {
        // SSH URL - everything after colon is owner/repo
        return url[colon_pos + 1..].to_string();
    }

    // Handle HTTPS format: https://github.com/owner/repo
    if let Some(pos) = url.rfind('/') {
        let after_slash = &url[pos + 1..];
        // Try to get owner/repo for GitHub-style URLs
        if let Some(owner_pos) = url[..pos].rfind('/') {
            return format!("{}/{}", &url[owner_pos + 1..pos], after_slash);
        }
        return after_slash.to_string();
    }

    url.to_string()
}

/// Derives target directory from URL
fn derive_target_from_url(url: &str) -> PathBuf {
    let url = url.trim_end_matches(".git");

    let repo_name = url
        .rsplit('/')
        .next()
        .or_else(|| url.rsplit(':').next())
        .unwrap_or("repo");

    PathBuf::from(repo_name)
}

/// Clones the repository with progress spinner showing git stages
fn clone_repo(url: &str, target: &Path) -> Result<()> {
    let spinner = crate::term::spinner("Cloning repository...");

    let mut child = Command::new("git")
        .args(["clone", "--progress", url, &target.to_string_lossy()])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to execute git clone")?;

    // Channel to collect stderr for error reporting
    let (tx, rx) = mpsc::channel();

    // Read stderr in a separate thread to avoid blocking
    // Git uses \r for progress updates (same-line overwrites), so we read raw and split on \r or \n
    let mut stderr = child.stderr.take().expect("stderr was piped");
    let spinner_clone = spinner.clone();
    let reader_thread = thread::spawn(move || {
        let mut all_output = String::new();
        let mut buf = [0u8; 256];
        let mut line_buf = String::new();

        while let Ok(n) = stderr.read(&mut buf) {
            if n == 0 {
                break;
            }
            let chunk = String::from_utf8_lossy(&buf[..n]);
            all_output.push_str(&chunk);

            for c in chunk.chars() {
                if c == '\r' || c == '\n' {
                    if !line_buf.is_empty() {
                        // Parse git progress output and update spinner
                        if let Some((stage, percent)) = parse_git_progress(&line_buf) {
                            let msg = match percent {
                                Some(p) => format!("Cloning repository [{stage}: {p}%]..."),
                                None => format!("Cloning repository [{stage}]..."),
                            };
                            spinner_clone.set_message(msg);
                        }
                        line_buf.clear();
                    }
                } else {
                    line_buf.push(c);
                }
            }
        }

        let _ = tx.send(all_output);
    });

    let status = child.wait().context("Failed to wait for git clone")?;

    let _ = reader_thread.join();
    let stderr_output = rx.recv().unwrap_or_default();

    spinner.finish_and_clear();

    if !status.success() {
        anyhow::bail!("{}", git::format_error("Clone failed", &stderr_output));
    }

    Ok(())
}

/// Parses git progress output to extract the current stage and optional percentage
fn parse_git_progress(line: &str) -> Option<(&str, Option<u8>)> {
    // Strip optional "remote:" prefix, then parse "Stage: NN%" format
    let line = line.trim().strip_prefix("remote:").unwrap_or(line.trim()).trim();

    let colon_pos = line.find(':')?;
    let stage = line[..colon_pos].trim();

    if is_progress_stage(stage) {
        let percent = extract_percent(&line[colon_pos + 1..]);
        Some((stage, percent))
    } else {
        None
    }
}

/// Checks if the given string is a recognized git progress stage
fn is_progress_stage(stage: &str) -> bool {
    matches!(
        stage,
        "Cloning into"
            | "Enumerating objects"
            | "Counting objects"
            | "Compressing objects"
            | "Receiving objects"
            | "Resolving deltas"
            | "Updating files"
    )
}

/// Extracts percentage from a string like " 45% (55/123)" or "100% (50/50), done."
fn extract_percent(s: &str) -> Option<u8> {
    let s = s.trim();
    let percent_pos = s.find('%')?;
    let num_str = s[..percent_pos].trim();
    num_str.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_target_from_url_https() {
        assert_eq!(
            derive_target_from_url("https://github.com/owner/repo.git"),
            PathBuf::from("repo")
        );
    }

    #[test]
    fn test_derive_target_from_url_ssh() {
        assert_eq!(
            derive_target_from_url("git@github.com:owner/repo.git"),
            PathBuf::from("repo")
        );
    }

    #[test]
    fn test_derive_target_from_url_no_extension() {
        assert_eq!(
            derive_target_from_url("https://github.com/owner/repo"),
            PathBuf::from("repo")
        );
    }

    #[test]
    fn test_extract_repo_display_name_https() {
        assert_eq!(
            extract_repo_display_name("https://github.com/anthropics/claude-code.git"),
            "anthropics/claude-code"
        );
    }

    #[test]
    fn test_extract_repo_display_name_ssh() {
        assert_eq!(
            extract_repo_display_name("git@github.com:anthropics/claude-code.git"),
            "anthropics/claude-code"
        );
    }

    #[test]
    fn test_parse_git_progress_remote_stage() {
        assert_eq!(
            parse_git_progress("remote: Enumerating objects: 123, done."),
            Some(("Enumerating objects", None))
        );
    }

    #[test]
    fn test_parse_git_progress_direct_stage_with_percent() {
        assert_eq!(
            parse_git_progress("Receiving objects:  45% (55/123)"),
            Some(("Receiving objects", Some(45)))
        );
    }

    #[test]
    fn test_parse_git_progress_resolving_deltas_complete() {
        assert_eq!(
            parse_git_progress("Resolving deltas: 100% (50/50), done."),
            Some(("Resolving deltas", Some(100)))
        );
    }

    #[test]
    fn test_parse_git_progress_remote_with_percent() {
        assert_eq!(
            parse_git_progress("remote: Counting objects: 75% (90/120)"),
            Some(("Counting objects", Some(75)))
        );
    }

    #[test]
    fn test_parse_git_progress_non_stage_line() {
        assert_eq!(parse_git_progress("fatal: repository not found"), None);
    }

    #[test]
    fn test_parse_git_progress_empty_line() {
        assert_eq!(parse_git_progress(""), None);
    }
}
