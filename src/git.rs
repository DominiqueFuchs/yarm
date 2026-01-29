use anyhow::{bail, Context, Result};
use console::style;
use std::path::Path;
use std::process::Command;

use crate::term::icon_error;

/// Verifies that git is available and returns a friendly error if not
pub fn ensure_available() -> Result<()> {
    match Command::new("git").arg("--version").output() {
        Ok(output) if output.status.success() => Ok(()),
        Ok(_) => bail!(
            "{}\n\n  git is installed but returned an error.\n  Try running 'git --version' to diagnose.",
            style("Git is not working properly").red().bold()
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => bail!(
            "{}\n\n  Install git from https://git-scm.com/downloads",
            style("Git is not installed or not in PATH").red().bold()
        ),
        Err(e) => bail!(
            "{}\n\n  {}",
            style("Failed to run git").red().bold(),
            e
        ),
    }
}

/// Formats a git command failure with styled output
pub fn format_error(operation: &str, stderr: &str) -> String {
    let header = format!("{} {}", icon_error(), style(operation).bold());

    let stderr = stderr.trim();
    if stderr.is_empty() {
        return header;
    }

    // Indent each line of the error output
    let details: String = stderr
        .lines()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n");

    format!("{header}\n\n{details}")
}

/// Sets or unsets a git config value.
///
/// Automatically detects whether `path` is a repository directory or a config file:
/// - Directory: uses `git -C <path> config --local`
/// - File: uses `git config --file <path>`
///
/// Pass `None` for `value` to unset the key.
pub fn set_config(path: &Path, key: &str, value: Option<&str>) -> Result<()> {
    let path_str = path.to_string_lossy().into_owned();

    let mut cmd = Command::new("git");

    if path.is_dir() {
        cmd.args(["-C", &path_str, "config", "--local"]);
    } else {
        cmd.args(["config", "--file", &path_str]);
    }

    match value {
        Some(v) => cmd.args([key, v]),
        None => cmd.args(["--unset", key]),
    };

    let status = cmd
        .status()
        .with_context(|| format!("Failed to run git config for {key}"))?;

    // For unset operations, exit code 5 means "key not found" which is fine
    if value.is_none() && status.code() == Some(5) {
        return Ok(());
    }

    if !status.success() {
        bail!("Failed to set git config {key}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_error_with_message() {
        let result = format_error("Clone failed", "fatal: repository not found");
        // Check structure (styled text makes exact comparison tricky)
        assert!(result.contains("Clone failed"));
        assert!(result.contains("fatal: repository not found"));
    }

    #[test]
    fn test_format_error_empty_stderr() {
        let result = format_error("Clone failed", "");
        assert!(result.contains("Clone failed"));
        assert!(!result.contains("\n\n"));
    }

    #[test]
    fn test_format_error_multiline() {
        let result = format_error("Clone failed", "line1\nline2\nline3");
        assert!(result.contains("line1"));
        assert!(result.contains("line2"));
        assert!(result.contains("line3"));
    }
}
