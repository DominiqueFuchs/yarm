use anyhow::{Context, Result};
use console::style;
use std::fmt;
use std::fs;

use crate::git;
use crate::profile::{discover_profiles, format_source_path, Profile};
use crate::term::{
    is_cancelled, print_success, print_warning, prompt_confirm, prompt_required_text, prompt_text,
    prompt_text_with_help, MenuLevel, MenuSession,
};

/// Menu options for profile management
#[derive(Clone, Copy)]
enum MenuOption {
    Edit,
    Create,
    Delete,
    List,
}

impl fmt::Display for MenuOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Edit => write!(f, "Edit profile"),
            Self::Create => write!(f, "Create new profile"),
            Self::Delete => write!(f, "Delete profile"),
            Self::List => write!(f, "List profiles"),
        }
    }
}

/// Main entry point for the profiles command
pub fn run(show_only: bool) -> Result<()> {
    if show_only {
        return show_profiles();
    }

    interactive_menu()
}

/// Lists all discovered profiles (non-interactive)
fn show_profiles() -> Result<()> {
    let profiles = discover_profiles()?;

    if profiles.is_empty() {
        print_warning("No profiles found");
        println!();
        println!("  Configure user.name and user.email in a gitconfig file to create a profile.");
        return Ok(());
    }

    print_success(format!(
        "{} profile{} found",
        profiles.len(),
        if profiles.len() == 1 { "" } else { "s" }
    ));
    println!();

    for profile in profiles {
        print_profile(&profile);
    }

    Ok(())
}

/// Prints a single profile's details
fn print_profile(profile: &Profile) {
    let source_display = format_source_path(&profile.source);

    println!(
        "  {} {}",
        style(&profile.name).bold(),
        style(format!("({source_display})")).dim()
    );

    for field in profile.fields() {
        println!("    {}: {}", field.label, field.value);
    }
    println!();
}


/// Interactive menu for managing profiles
fn interactive_menu() -> Result<()> {
    let mut session = MenuSession::new();

    loop {
        session.prepare();

        let profiles = discover_profiles()?;

        let mut options = vec![MenuOption::Create];
        if !profiles.is_empty() {
            options.insert(0, MenuOption::Edit);
            options.push(MenuOption::Delete);
        }
        options.push(MenuOption::List);

        let selection = MenuLevel::Top.select("Manage profiles:", options).prompt();

        match selection {
            Ok(MenuOption::Edit) => edit_profile()?,
            Ok(MenuOption::Create) => create_profile()?,
            Ok(MenuOption::Delete) => delete_profile()?,
            Ok(MenuOption::List) => {
                println!();
                show_profiles()?;
            }
            Err(_) => break,
        }
    }

    Ok(())
}

/// Edit an existing profile
fn edit_profile() -> Result<()> {
    let profiles = discover_profiles()?;

    if profiles.is_empty() {
        print_warning("No profiles to edit");
        return Ok(());
    }

    let options: Vec<String> = profiles.iter().map(|p| p.display_option()).collect();

    let selection = match MenuLevel::Sub.select_filterable("Select profile to edit:", options.clone()).prompt() {
        Ok(s) => s,
        Err(e) if is_cancelled(&e) => return Ok(()),
        Err(e) => return Err(e).context("Selection failed"),
    };

    let idx = options.iter().position(|s| s == &selection).unwrap();
    let profile = &profiles[idx];

    println!();
    println!("  Editing: {}", style(&profile.name).bold());
    println!("  Source:  {}", format_source_path(&profile.source));
    println!();

    // Store old values for diff
    let old_name = profile.user_name.clone();
    let old_email = profile.user_email.clone();
    let old_key = profile.signing_key.clone();
    let old_gpg_sign = profile.gpg_sign;

    let Some(new_name) = prompt_required_text("Name:", profile.user_name.as_deref())? else {
        return Ok(());
    };

    let Some(new_email) = prompt_text("Email:", profile.user_email.as_deref())? else {
        return Ok(());
    };

    let Some(new_key) = prompt_text("GPG signing key:", profile.signing_key.as_deref())? else {
        return Ok(());
    };

    let Some(new_gpg_sign) = prompt_confirm("Enable commit signing?", profile.gpg_sign.unwrap_or(false))? else {
        return Ok(());
    };

    // Apply changes
    let path = &profile.source;

    git::set_config(path, "user.name", Some(&new_name))?;

    if new_email.is_empty() {
        git::set_config(path, "user.email", None)?;
    } else {
        git::set_config(path, "user.email", Some(&new_email))?;
    }

    if new_key.is_empty() {
        git::set_config(path, "user.signingkey", None)?;
    } else {
        git::set_config(path, "user.signingkey", Some(&new_key))?;
    }

    if new_gpg_sign {
        git::set_config(path, "commit.gpgsign", Some("true"))?;
    } else {
        git::set_config(path, "commit.gpgsign", None)?;
    }

    println!();
    print_success(format!("Profile '{}' updated", profile.name));
    println!();

    print_field_diff("Name", old_name.as_deref(), Some(&new_name));
    print_field_diff("Email", old_email.as_deref(), if new_email.is_empty() { None } else { Some(&new_email) });
    print_field_diff("GPG key", old_key.as_deref(), if new_key.is_empty() { None } else { Some(&new_key) });
    print_field_diff(
        "Signing",
        Some(if old_gpg_sign == Some(true) { "enabled" } else { "disabled" }),
        Some(if new_gpg_sign { "enabled" } else { "disabled" }),
    );
    println!();

    Ok(())
}

/// Prints a field diff if the value changed
fn print_field_diff(label: &str, old: Option<&str>, new: Option<&str>) {
    match (old, new) {
        (Some(o), Some(n)) if o != n => {
            println!(
                "    {}: {} {} {}",
                label,
                style(o).red().dim(),
                style("→").dim(),
                style(n).green()
            );
        }
        (None, Some(n)) => {
            println!(
                "    {}: {} {}",
                label,
                style("+").green(),
                style(n).green()
            );
        }
        (Some(o), None) => {
            println!(
                "    {}: {} {}",
                label,
                style("-").red(),
                style(o).red().dim()
            );
        }
        _ => {} // No change
    }
}

/// Create a new profile
fn create_profile() -> Result<()> {
    println!();

    let Some(name) = prompt_text_with_help("Profile name:", &MenuLevel::Sub.help_with("e.g., 'work', 'personal', 'oss'"))? else {
        return Ok(());
    };

    if name.is_empty() {
        print_warning("Profile name cannot be empty");
        return Ok(());
    }

    let home = dirs::home_dir().context("Could not determine home directory")?;
    let gitconfig_path = home.join(format!(".gitconfig-{name}"));
    let xdg_path = home.join(format!(".config/git/{name}.gitconfig"));

    let location_options = vec![
        format!("~/.gitconfig-{name}"),
        format!("~/.config/git/{name}.gitconfig"),
    ];

    let location = match MenuLevel::Sub.select("Where to create the profile:", location_options).prompt() {
        Ok(s) => s,
        Err(e) if is_cancelled(&e) => return Ok(()),
        Err(e) => return Err(e).context("Selection failed"),
    };

    let path = if location.starts_with("~/.config") {
        // Ensure directory exists
        let parent = xdg_path.parent().unwrap();
        if !parent.exists() {
            fs::create_dir_all(parent).context("Failed to create config directory")?;
        }
        xdg_path
    } else {
        gitconfig_path
    };

    if path.exists() {
        print_warning(format!(
            "Profile file already exists: {}",
            format_source_path(&path)
        ));
        return Ok(());
    }

    let Some(user_name) = prompt_required_text("Name:", None)? else {
        return Ok(());
    };

    let Some(user_email) = prompt_text("Email:", None)? else {
        return Ok(());
    };

    let Some(signing_key) = prompt_text("GPG signing key:", None)? else {
        return Ok(());
    };

    let gpg_sign = if signing_key.is_empty() {
        false
    } else {
        let Some(value) = prompt_confirm("Enable commit signing?", true)? else {
            return Ok(());
        };
        value
    };

    fs::write(&path, "# Git profile configuration\n").context("Failed to create profile file")?;

    git::set_config(&path, "user.name", Some(&user_name))?;
    if !user_email.is_empty() {
        git::set_config(&path, "user.email", Some(&user_email))?;
    }
    if !signing_key.is_empty() {
        git::set_config(&path, "user.signingkey", Some(&signing_key))?;
    }
    if gpg_sign {
        git::set_config(&path, "commit.gpgsign", Some("true"))?;
    }

    println!();
    print_success(format!(
        "Created profile '{}' at {}",
        name,
        format_source_path(&path)
    ));
    println!();

    Ok(())
}

/// Delete a profile
fn delete_profile() -> Result<()> {
    let profiles = discover_profiles()?;

    if profiles.is_empty() {
        print_warning("No profiles to delete");
        return Ok(());
    }

    // Filter to only show deletable profiles (not system gitconfig)
    let deletable: Vec<_> = profiles
        .iter()
        .filter(|p| {
            let path_str = p.source.to_string_lossy();
            // Don't allow deleting main .gitconfig or system configs
            !path_str.ends_with("/.gitconfig")
                && !path_str.contains("/etc/")
                && !path_str.ends_with("/.git/config")
        })
        .collect();

    if deletable.is_empty() {
        print_warning("No deletable profiles found");
        println!("  (System and main gitconfig files cannot be deleted)");
        return Ok(());
    }

    let options: Vec<String> = deletable.iter().map(|p| p.display_option()).collect();

    let selection = match MenuLevel::Sub.select_filterable("Select profile to delete:", options.clone()).prompt() {
        Ok(s) => s,
        Err(e) if is_cancelled(&e) => return Ok(()),
        Err(e) => return Err(e).context("Selection failed"),
    };

    let idx = options.iter().position(|s| s == &selection).unwrap();
    let profile = deletable[idx];

    println!();
    print_profile(profile);

    let Some(confirmed) = prompt_confirm(&format!("Delete profile '{}' and its config file?", profile.name), false)? else {
        return Ok(());
    };

    if !confirmed {
        println!("  Deletion cancelled");
        return Ok(());
    }

    fs::remove_file(&profile.source).context("Failed to delete profile file")?;

    println!();
    print_success(format!("Deleted profile '{}'", profile.name));
    println!();

    Ok(())
}
