use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::git;
use crate::profile::{ProfileContext, apply_profile, resolve_profile_with_context};
use crate::term::{print_header, print_success};

/// Executes the init command flow
pub fn run(profile_name: Option<&str>) -> Result<()> {
    git::ensure_available()?;

    let target = PathBuf::from(".");

    let display_path = target.canonicalize().unwrap_or_else(|_| target.clone());

    if target.join(".git").exists() {
        anyhow::bail!("Already a git repository: {}", display_path.display());
    }

    print_header("Initializing:", display_path.display());
    println!();

    let context = ProfileContext::new(display_path.clone(), None);
    let Some(selected) = resolve_profile_with_context(profile_name, &context)? else {
        return Ok(());
    };

    init_repo(&target)?;

    apply_profile(&target, &selected)?;

    let config = crate::config::load()?;
    if crate::config::is_in_pool(&display_path, &config.pool_paths()) {
        crate::state::register_repo(&display_path)?;
    }

    print_success(format!(
        "Initialized repository in {}",
        display_path.display()
    ));
    print_success(format!(
        "Applied profile '{}' ({})",
        selected.name,
        selected.config_summary()
    ));

    Ok(())
}

/// Initializes a git repository
fn init_repo(target: &std::path::Path) -> Result<()> {
    let output = Command::new("git")
        .args(["init", &target.to_string_lossy()])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .context("Failed to execute git init")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{}", git::format_error("Init failed", &stderr));
    }

    Ok(())
}
