use anyhow::Result;
use std::path::PathBuf;

use crate::git;
use crate::profile::{ProfileContext, apply_profile, resolve_profile_with_context};
use crate::term::{print_header, print_success};

/// Executes the apply command flow
pub fn run(path: Option<PathBuf>, profile_name: Option<&str>) -> Result<()> {
    git::ensure_available()?;

    let target = path.unwrap_or_else(|| PathBuf::from("."));

    let display_path = target
        .canonicalize()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| target.display().to_string());

    if !target.join(".git").exists() {
        anyhow::bail!("Not a git repository: {}", target.display());
    }

    print_header("Repository:", &display_path);
    println!();

    let context = ProfileContext::new(target.clone(), None);
    let Some(selected) = resolve_profile_with_context(profile_name, &context)? else {
        return Ok(());
    };

    apply_profile(&target, &selected)?;

    print_success(format!(
        "Applied profile '{}' ({})",
        selected.name,
        selected.config_summary()
    ));

    Ok(())
}
