use anyhow::{Context, Result};
use console::style;
use std::fmt;
use std::fs;

use crate::git;
use crate::profile::{Profile, discover_profiles, find_profile_by_name};
use crate::term::{
    MenuLevel, MenuSession, format_home_path, is_cancelled, print_success, print_warning,
    prompt_confirm, prompt_required_text, prompt_text, prompt_text_with_help,
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

/// Menu options when a specific profile is targeted
#[derive(Clone, Copy)]
enum ProfileAction {
    Show,
    Edit,
    Delete,
}

impl fmt::Display for ProfileAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Show => write!(f, "Show details"),
            Self::Edit => write!(f, "Edit profile"),
            Self::Delete => write!(f, "Delete profile"),
        }
    }
}

/// Main entry point for the profiles command
pub fn run(name: Option<&str>, show_only: bool) -> Result<()> {
    if let Some(name) = name {
        let profiles = discover_profiles()?;
        let profile = find_profile_by_name(&profiles, name)?;

        if show_only {
            println!();
            print_profile(&profile);
            return Ok(());
        }

        return single_profile_menu(&profile);
    }

    if show_only {
        return show_profiles();
    }

    interactive_menu()
}

/// Interactive menu for a specific named profile
fn single_profile_menu(profile: &Profile) -> Result<()> {
    let mut session = MenuSession::new();

    loop {
        session.prepare();

        let options = vec![
            ProfileAction::Show,
            ProfileAction::Edit,
            ProfileAction::Delete,
        ];

        let selection = MenuLevel::Top
            .select(&format!("Profile '{}':", profile.name), options)
            .prompt();

        match selection {
            Ok(ProfileAction::Show) => {
                println!();
                print_profile(profile);
                println!();
                session.printed_output();
            }
            Ok(ProfileAction::Edit) => {
                edit_single_profile(profile)?;
                break;
            }
            Ok(ProfileAction::Delete) => {
                delete_single_profile(profile)?;
                break;
            }
            Err(_) => break,
        }
    }

    Ok(())
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

    for (i, profile) in profiles.iter().enumerate() {
        if i > 0 {
            println!();
        }
        print_profile(profile);
    }

    Ok(())
}

/// Prints a single profile's details (no trailing blank line)
fn print_profile(profile: &Profile) {
    let source_display = format_home_path(&profile.source);

    if profile.is_default {
        println!(
            "  {} {} {}",
            style(&profile.name).bold(),
            style("(yarm default)").cyan(),
            style(format!("({source_display})")).dim()
        );
    } else {
        println!(
            "  {} {}",
            style(&profile.name).bold(),
            style(format!("({source_display})")).dim()
        );
    }

    if let Some(identity) = profile.identity() {
        println!("    {identity}");
    }
    for field in profile.fields() {
        println!("    {:<16}{}", field.label, field.value);
    }
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
            Ok(MenuOption::Edit) => {
                edit_profile()?;
                break;
            }
            Ok(MenuOption::Create) => {
                create_profile()?;
                break;
            }
            Ok(MenuOption::Delete) => {
                delete_profile()?;
                break;
            }
            Ok(MenuOption::List) => {
                println!();
                show_profiles()?;
                session.printed_output();
            }
            Err(_) => break,
        }
    }

    Ok(())
}

/// Edit an existing profile (with interactive selection)
fn edit_profile() -> Result<()> {
    let profiles = discover_profiles()?;

    if profiles.is_empty() {
        print_warning("No profiles to edit");
        return Ok(());
    }

    let options: Vec<String> = profiles
        .iter()
        .map(super::super::profile::Profile::display_option)
        .collect();

    let selection = match MenuLevel::Sub
        .select_filterable("Select profile to edit:", options.clone())
        .prompt()
    {
        Ok(s) => s,
        Err(e) if is_cancelled(&e) => return Ok(()),
        Err(e) => return Err(e).context("Selection failed"),
    };

    let idx = options
        .iter()
        .position(|s| s == &selection)
        .expect("selection must be in options");
    let profile = &profiles[idx];

    edit_single_profile(profile)
}

/// Edit a known profile
#[allow(clippy::too_many_lines)]
fn edit_single_profile(profile: &Profile) -> Result<()> {
    println!();
    println!("  Editing: {}", style(&profile.name).bold());
    println!("  Source:  {}", format_home_path(&profile.source));
    println!();

    // Store old values for diff
    let old_name = profile.user_name.clone();
    let old_email = profile.user_email.clone();
    let old_key = profile.signing_key.clone();
    let old_format = profile.gpg_format.clone();
    let old_gpg_sign = profile.gpg_sign;
    let old_tag_gpg_sign = profile.tag_gpg_sign;

    let Some(new_name) = prompt_required_text("Name:", profile.user_name.as_deref())? else {
        return Ok(());
    };

    let Some(new_email) = prompt_text("Email:", profile.user_email.as_deref())? else {
        return Ok(());
    };

    let Some(new_key) = prompt_text("Signing key:", profile.signing_key.as_deref())? else {
        return Ok(());
    };

    let (new_format, new_gpg_sign, new_tag_gpg_sign) = if new_key.is_empty() {
        (None, false, false)
    } else {
        let current_format = profile.gpg_format.as_deref().unwrap_or("openpgp");
        let format_options = vec!["openpgp (GPG)", "ssh", "x509"];
        let default_idx = match current_format {
            "ssh" => 1,
            "x509" => 2,
            _ => 0,
        };
        let format = match MenuLevel::Sub
            .select_with_default("Signing format:", format_options, default_idx)
            .prompt()
        {
            Ok(s) => s,
            Err(e) if is_cancelled(&e) => return Ok(()),
            Err(e) => return Err(e).context("Selection failed"),
        };
        let gpg_format = match format.split_whitespace().next().unwrap() {
            "openpgp" => None,
            f => Some(f.to_string()),
        };

        let Some(commit_sign) = prompt_confirm("Sign commits?", profile.gpg_sign.unwrap_or(false))?
        else {
            return Ok(());
        };
        let Some(tag_sign) = prompt_confirm("Sign tags?", profile.tag_gpg_sign.unwrap_or(false))?
        else {
            return Ok(());
        };
        (gpg_format, commit_sign, tag_sign)
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
        git::set_config(path, "gpg.format", None)?;
        git::set_config(path, "commit.gpgsign", None)?;
        git::set_config(path, "tag.gpgsign", None)?;
    } else {
        git::set_config(path, "user.signingkey", Some(&new_key))?;
        if let Some(ref format) = new_format {
            git::set_config(path, "gpg.format", Some(format))?;
        } else {
            git::set_config(path, "gpg.format", None)?;
        }
        if new_gpg_sign {
            git::set_config(path, "commit.gpgsign", Some("true"))?;
        } else {
            git::set_config(path, "commit.gpgsign", None)?;
        }
        if new_tag_gpg_sign {
            git::set_config(path, "tag.gpgsign", Some("true"))?;
        } else {
            git::set_config(path, "tag.gpgsign", None)?;
        }
    }

    println!();
    print_success(format!("Profile '{}' updated", profile.name));

    print_field_diff("Name", old_name.as_deref(), Some(&new_name));
    print_field_diff(
        "Email",
        old_email.as_deref(),
        if new_email.is_empty() {
            None
        } else {
            Some(&new_email)
        },
    );
    print_field_diff(
        "Signing key",
        old_key.as_deref(),
        if new_key.is_empty() {
            None
        } else {
            Some(&new_key)
        },
    );
    let has_signing = old_key.is_some() || !new_key.is_empty();
    if has_signing {
        let effective_old_format = if old_key.is_some() {
            Some(old_format.as_deref().unwrap_or("openpgp"))
        } else {
            None
        };
        let effective_new_format = if new_key.is_empty() {
            None
        } else {
            Some(new_format.as_deref().unwrap_or("openpgp"))
        };
        print_field_diff("Format", effective_old_format, effective_new_format);
        print_field_diff(
            "Sign commits",
            old_key.as_ref().map(|_| {
                if old_gpg_sign == Some(true) {
                    "enabled"
                } else {
                    "disabled"
                }
            }),
            if new_key.is_empty() {
                None
            } else {
                Some(if new_gpg_sign { "enabled" } else { "disabled" })
            },
        );
        print_field_diff(
            "Sign tags",
            old_key.as_ref().map(|_| {
                if old_tag_gpg_sign == Some(true) {
                    "enabled"
                } else {
                    "disabled"
                }
            }),
            if new_key.is_empty() {
                None
            } else {
                Some(if new_tag_gpg_sign {
                    "enabled"
                } else {
                    "disabled"
                })
            },
        );
    }

    Ok(())
}

/// Prints a field diff if the value changed
fn print_field_diff(label: &str, old: Option<&str>, new: Option<&str>) {
    match (old, new) {
        (Some(o), Some(n)) if o != n => {
            println!(
                "    {}: {} {} {}",
                label,
                style(o).red(),
                style("→").dim(),
                style(n).green()
            );
        }
        (None, Some(n)) => {
            println!("    {}: {} {}", label, style("+").green(), style(n).green());
        }
        (Some(o), None) => {
            println!("    {}: {} {}", label, style("-").red(), style(o).red());
        }
        _ => {} // No change
    }
}

/// Create a new profile
fn create_profile() -> Result<()> {
    println!();

    let Some(name) = prompt_text_with_help(
        "Profile name:",
        &MenuLevel::Sub.help_with("e.g., 'work', 'personal', 'oss'"),
    )?
    else {
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

    let location = match MenuLevel::Sub
        .select("Where to create the profile:", location_options)
        .prompt()
    {
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
            format_home_path(&path)
        ));
        return Ok(());
    }

    let Some(user_name) = prompt_required_text("Name:", None)? else {
        return Ok(());
    };

    let Some(user_email) = prompt_text("Email:", None)? else {
        return Ok(());
    };

    let Some(signing_key) = prompt_text("Signing key:", None)? else {
        return Ok(());
    };

    let (gpg_format, gpg_sign, tag_gpg_sign) = if signing_key.is_empty() {
        (None, false, false)
    } else {
        let format_options = vec!["openpgp (GPG)", "ssh", "x509"];
        let format = match MenuLevel::Sub
            .select("Signing format:", format_options)
            .prompt()
        {
            Ok(s) => s,
            Err(e) if is_cancelled(&e) => return Ok(()),
            Err(e) => return Err(e).context("Selection failed"),
        };
        let gpg_format = match format.split_whitespace().next().unwrap() {
            "openpgp" => None, // Default, no need to set
            f => Some(f.to_string()),
        };

        let Some(commit_sign) = prompt_confirm("Sign commits?", true)? else {
            return Ok(());
        };
        let Some(tag_sign) = prompt_confirm("Sign tags?", commit_sign)? else {
            return Ok(());
        };
        (gpg_format, commit_sign, tag_sign)
    };

    fs::write(&path, "# Git profile configuration\n").context("Failed to create profile file")?;

    git::set_config(&path, "user.name", Some(&user_name))?;
    if !user_email.is_empty() {
        git::set_config(&path, "user.email", Some(&user_email))?;
    }
    if !signing_key.is_empty() {
        git::set_config(&path, "user.signingkey", Some(&signing_key))?;
    }
    if let Some(ref format) = gpg_format {
        git::set_config(&path, "gpg.format", Some(format))?;
    }
    if gpg_sign {
        git::set_config(&path, "commit.gpgsign", Some("true"))?;
    }
    if tag_gpg_sign {
        git::set_config(&path, "tag.gpgsign", Some("true"))?;
    }

    println!();
    print_success(format!(
        "Created profile '{}' at {}",
        name,
        format_home_path(&path)
    ));

    Ok(())
}

/// Delete a profile (with interactive selection)
fn delete_profile() -> Result<()> {
    let profiles = discover_profiles()?;

    if profiles.is_empty() {
        print_warning("No profiles to delete");
        return Ok(());
    }

    // Filter to only show deletable profiles (not system gitconfig)
    let deletable: Vec<_> = profiles.iter().filter(|p| is_deletable(p)).collect();

    if deletable.is_empty() {
        print_warning("No deletable profiles found");
        println!("  (System and main gitconfig files cannot be deleted)");
        return Ok(());
    }

    let options: Vec<String> = deletable.iter().map(|p| p.display_option()).collect();

    let selection = match MenuLevel::Sub
        .select_filterable("Select profile to delete:", options.clone())
        .prompt()
    {
        Ok(s) => s,
        Err(e) if is_cancelled(&e) => return Ok(()),
        Err(e) => return Err(e).context("Selection failed"),
    };

    let idx = options
        .iter()
        .position(|s| s == &selection)
        .expect("selection must be in options");
    let profile = deletable[idx];

    delete_single_profile(profile)
}

/// Delete a known profile
fn delete_single_profile(profile: &Profile) -> Result<()> {
    if !is_deletable(profile) {
        print_warning(format!(
            "Cannot delete '{}' (system or main gitconfig)",
            profile.name
        ));
        return Ok(());
    }

    println!();
    print_profile(profile);

    let Some(confirmed) = prompt_confirm(
        &format!("Delete profile '{}' and its config file?", profile.name),
        false,
    )?
    else {
        return Ok(());
    };

    if !confirmed {
        println!("  Deletion cancelled");
        return Ok(());
    }

    fs::remove_file(&profile.source).context("Failed to delete profile file")?;

    println!();
    print_success(format!("Deleted profile '{}'", profile.name));

    Ok(())
}

/// Checks whether a profile can be deleted (not system/main gitconfig)
fn is_deletable(profile: &Profile) -> bool {
    let path_str = profile.source.to_string_lossy();
    !path_str.ends_with("/.gitconfig")
        && !path_str.contains("/etc/")
        && !path_str.ends_with("/.git/config")
}
