use anyhow::Result;
use std::path::PathBuf;

use crate::commands::find;
use crate::git;
use crate::profile::{ProfileContext, apply_profile, resolve_profile_with_context};
use crate::term::{print_header, print_success, print_warning};

/// Executes the apply command flow
pub fn run(name: Option<&str>, profile_name: Option<&str>, pool: Option<&str>) -> Result<()> {
    git::ensure_available()?;

    if let Some(pool_name) = pool {
        return run_pool(pool_name, profile_name);
    }

    let target = match name {
        Some(name) => find::resolve_repo(name)?,
        None => PathBuf::from("."),
    };

    apply_to_repo(&target, profile_name)
}

fn apply_to_repo(target: &PathBuf, profile_name: Option<&str>) -> Result<()> {
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

    apply_profile(target, &selected)?;

    print_success(format!(
        "Applied profile '{}' ({})",
        selected.name,
        selected.config_summary()
    ));

    Ok(())
}

fn run_pool(pool_name: &str, profile_name: Option<&str>) -> Result<()> {
    let pool_path = find::resolve_pool(pool_name)?;
    let pool_path = pool_path.canonicalize().unwrap_or(pool_path);

    let state = crate::state::load()?;
    let repos: Vec<_> = state
        .repositories
        .iter()
        .filter(|r| r.starts_with(&pool_path))
        .collect();

    if repos.is_empty() {
        print_warning(format!("No repositories found in pool '{pool_name}'"));
        return Ok(());
    }

    print_header("Pool:", pool_name);
    println!();

    let context = ProfileContext::new(pool_path, None);
    let Some(selected) = resolve_profile_with_context(profile_name, &context)? else {
        return Ok(());
    };

    let mut applied = 0;
    for repo in &repos {
        let display = repo
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| repo.display().to_string());

        apply_profile(repo, &selected)?;
        print_success(format!("Applied to {display}"));
        applied += 1;
    }

    println!();
    print_success(format!(
        "Applied profile '{}' ({}) to {applied} {}",
        selected.name,
        selected.config_summary(),
        if applied == 1 {
            "repository"
        } else {
            "repositories"
        }
    ));

    Ok(())
}
