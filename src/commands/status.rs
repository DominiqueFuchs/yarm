use anyhow::Result;
use console::style;

use crate::term::{format_elapsed, print_hint, print_warning};

/// Executes the status command flow
pub fn run(full: bool) -> Result<()> {
    let config = crate::config::load()?;
    let pools = config.pool_paths();
    let state = crate::state::load()?;

    if pools.is_empty() {
        print_warning("No repository pools configured");
        println!();
        print_hint(format!(
            "Add pools to {}:",
            style("~/.config/yarm.toml").dim()
        ));
        println!();
        println!("        [repositories]");
        println!("        pools = [\"~/projects\", \"~/work\"]");
        return Ok(());
    }

    println!("  {}", style("Repository pools:").bold());

    for pool in &pools {
        let pool_repos: Vec<_> = state
            .repositories
            .iter()
            .filter(|r| r.starts_with(pool))
            .collect();
        let repo_count = pool_repos.len();

        let exists = pool.is_dir();
        let path_display = format_pool_path(pool);

        if !exists {
            println!(
                "    {} {} {}",
                style("•").dim(),
                style(&path_display).dim(),
                style("(not found)").red()
            );
        } else if repo_count == 0 {
            println!(
                "    {} {} {}",
                style("•").dim(),
                path_display,
                style("(no scan data)").dim()
            );
        } else {
            let label = if repo_count == 1 {
                "repository"
            } else {
                "repositories"
            };
            println!(
                "    {} {} {}",
                style("•").cyan(),
                path_display,
                style(format!("({repo_count} {label})")).dim()
            );

            if full {
                print_repo_list(&pool_repos, pool);
            }
        }
    }

    if let Some(scan_time) = state.last_scan_time() {
        println!();
        println!(
            "  {} {}",
            style("Last scan:").bold(),
            style(format_elapsed(scan_time)).dim()
        );
    }

    if state.repositories.is_empty() {
        println!();
        print_hint(format!(
            "Run {} to discover repositories",
            style("yarm scan").cyan()
        ));
    }

    Ok(())
}

fn print_repo_list(repos: &[&std::path::PathBuf], pool: &std::path::Path) {
    let mut rel_paths: Vec<_> = repos
        .iter()
        .map(|r| r.strip_prefix(pool).unwrap_or(r))
        .collect();
    rel_paths.sort();

    for rel in &rel_paths {
        println!("        {}", rel.display());
    }
}

fn format_pool_path(path: &std::path::Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rest) = path.strip_prefix(&home) {
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}
