use anyhow::{Context, Result};
use console::Term;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::git;
use crate::term::MenuLevel;

/// Error message when no profiles are found
pub const NO_PROFILES_ERROR: &str =
    "No git profiles found. Configure user.name/user.email in a gitconfig file.";

/// A discovered git identity profile
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// Derived from filename (e.g., "work" from "work.gitconfig")
    pub name: String,
    /// Source file path
    pub source: PathBuf,
    /// Git user.name value
    pub user_name: Option<String>,
    /// Git user.email value
    pub user_email: Option<String>,
    /// Git user.signingkey value
    pub signing_key: Option<String>,
    /// Git commit.gpgsign value
    pub gpg_sign: Option<bool>,
}

/// A profile field with its display label and value
pub struct ProfileField<'a> {
    pub label: &'static str,
    pub value: &'a str,
}

impl Profile {
    /// Returns an iterator over the profile's fields with their display labels
    pub fn fields(&self) -> impl Iterator<Item = ProfileField<'_>> {
        let name = self.user_name.as_deref().map(|v| ProfileField {
            label: "Name",
            value: v,
        });
        let email = self.user_email.as_deref().map(|v| ProfileField {
            label: "Email",
            value: v,
        });
        let key = self.signing_key.as_deref().map(|v| ProfileField {
            label: "GPG key",
            value: v,
        });
        let gpg_sign = self
            .gpg_sign
            .filter(|&v| v)
            .map(|_| ProfileField {
                label: "GPG signing",
                value: "enabled",
            });

        [name, email, key, gpg_sign].into_iter().flatten()
    }

    /// Returns a display string showing the config values that were applied
    pub fn config_summary(&self) -> String {
        self.fields()
            .map(|f| format!("{}: {}", f.label, f.value))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Returns a display string for menu selection: "name (~/path/to/source)"
    pub fn display_option(&self) -> String {
        format!("{} ({})", self.name, format_source_path(&self.source))
    }
}

/// Discovers git identity profiles from gitconfig files.
///
/// This discovers profiles from two sources:
/// 1. Files git knows about (`git config --list --show-origin`)
/// 2. Additional `*.gitconfig` files in common locations
///
/// Profiles are ordered: current effective profile first, then git-known
/// profiles alphabetically, then additional discovered profiles alphabetically.
pub fn discover_profiles() -> Result<Vec<Profile>> {
    let mut git_profiles = Vec::new();
    let mut additional_profiles = Vec::new();
    let mut seen_sources: HashSet<PathBuf> = HashSet::new();

    // Get current effective config to identify the "active" profile
    let current_email = get_current_git_config("user.email");

    let output = Command::new("git")
        .args(["config", "--list", "--show-origin"])
        .output()
        .context("Failed to execute git config")?;

    if output.status.success() {
        let stdout = String::from_utf8(output.stdout).context("Invalid UTF-8 in git config output")?;
        for profile in parse_git_config_output(&stdout) {
            seen_sources.insert(profile.source.clone());
            git_profiles.push(profile);
        }
    }

    for path in find_gitconfig_files() {
        if seen_sources.contains(&path) {
            continue;
        }
        if let Some(profile) = parse_gitconfig_file(&path) {
            seen_sources.insert(path);
            additional_profiles.push(profile);
        }
    }

    git_profiles.sort_by(|a, b| a.name.cmp(&b.name));
    additional_profiles.sort_by(|a, b| a.name.cmp(&b.name));

    let current_idx = current_email.as_ref().and_then(|email| {
        git_profiles.iter().position(|p| p.user_email.as_ref() == Some(email))
    });

    let mut profiles = Vec::new();

    // Add current profile first if found
    if let Some(idx) = current_idx {
        profiles.push(git_profiles.remove(idx));
    }

    profiles.extend(git_profiles);
    profiles.extend(additional_profiles);

    Ok(profiles)
}

/// Formats a path for display, using ~ for home directory
pub fn format_source_path(path: &Path) -> String {
    if let Some(home) = dirs::home_dir()
        && let Ok(relative) = path.strip_prefix(&home)
    {
        return format!("~/{}", relative.display());
    }
    path.display().to_string()
}

/// Formats a profile for display
fn format_profile_display(profile: &Profile) -> String {
    let mut parts = Vec::new();

    match (&profile.user_name, &profile.user_email) {
        (Some(name), Some(email)) => parts.push(format!("{name} <{email}>")),
        (Some(name), None) => parts.push(name.clone()),
        (None, Some(email)) => parts.push(format!("<{email}>")),
        (None, None) => {}
    }

    let mut attrs = Vec::new();
    if let Some(ref key) = profile.signing_key {
        attrs.push(format!("signing key: {key}"));
    }
    if profile.gpg_sign == Some(true) {
        attrs.push("gpgsign".to_string());
    }
    if !attrs.is_empty() {
        parts.push(format!("[{}]", attrs.join(", ")));
    }

    let source_display = format_source_path(&profile.source);
    parts.push(format!("({source_display})"));

    parts.join(" ")
}

/// Discovers and resolves a profile, either by name or interactive selection.
///
/// This combines profile discovery, empty-check, and selection/lookup into one call.
pub fn resolve_profile(profile_name: Option<&str>) -> Result<Profile> {
    let profiles = discover_profiles()?;

    if profiles.is_empty() {
        anyhow::bail!(NO_PROFILES_ERROR);
    }

    match profile_name {
        Some(name) => find_profile_by_name(&profiles, name),
        None => select_profile(profiles),
    }
}

/// Interactive profile selection
fn select_profile(profiles: Vec<Profile>) -> Result<Profile> {
    let options: Vec<String> = profiles.iter().map(format_profile_display).collect();

    let selection = MenuLevel::Sub
        .select_filterable("Select profile:", options.clone())
        .prompt()
        .context("Profile selection cancelled")?;

    let selected_idx = options
        .iter()
        .position(|s| s == &selection)
        .ok_or_else(|| anyhow::anyhow!("Failed to find selected profile"))?;

    let selected = profiles.into_iter().nth(selected_idx).unwrap();

    let term = Term::stdout();
    let _ = term.clear_last_lines(1);

    Ok(selected)
}

/// Finds a profile by name with fallback matching
///
/// Matching priority:
/// 1. Exact match on profile name
/// 2. Exact match on source path
/// 3. Match with dot prefix (e.g., "work" matches ".work")
/// 4. Match with .gitconfig- prefix (e.g., "work" matches ".gitconfig-work")
pub fn find_profile_by_name(profiles: &[Profile], name: &str) -> Result<Profile> {
    let search_path = PathBuf::from(name);
    let dotted_name = format!(".{name}");
    let gitconfig_name = format!(".gitconfig-{name}");

    // Exact match on name or path takes priority
    if let Some(profile) = profiles
        .iter()
        .find(|p| p.name == name || p.source == search_path)
    {
        return Ok(profile.clone());
    }

    // Fallback: try with dot prefix or .gitconfig- prefix
    if let Some(profile) = profiles
        .iter()
        .find(|p| p.name == dotted_name || p.name == gitconfig_name)
    {
        return Ok(profile.clone());
    }

    anyhow::bail!(
        "Profile '{name}' not found. Available profiles: {}",
        profiles
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// Applies profile settings to a repository
pub fn apply_profile(repo_path: &Path, profile: &Profile) -> Result<()> {
    let git_dir = repo_path.join(".git");
    if !git_dir.exists() {
        anyhow::bail!("Not a git repository: {}", repo_path.display());
    }

    if let Some(ref name) = profile.user_name {
        git::set_config(repo_path, "user.name", Some(name))?;
    }

    if let Some(ref email) = profile.user_email {
        git::set_config(repo_path, "user.email", Some(email))?;
    }

    if let Some(ref key) = profile.signing_key {
        git::set_config(repo_path, "user.signingkey", Some(key))?;
    }

    if let Some(gpg_sign) = profile.gpg_sign {
        git::set_config(repo_path, "commit.gpgsign", Some(if gpg_sign { "true" } else { "false" }))?;
    }

    Ok(())
}

/// Gets a git config value for the current context
fn get_current_git_config(key: &str) -> Option<String> {
    Command::new("git")
        .args(["config", key])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Finds gitconfig files in common locations
fn find_gitconfig_files() -> Vec<PathBuf> {
    let mut files = Vec::new();

    if let Some(home) = dirs::home_dir() {
        if let Ok(entries) = fs::read_dir(&home) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str())
                    && (name.starts_with(".gitconfig-") || name.starts_with(".gitconfig."))
                {
                    files.push(path);
                }
            }
        }

        let config_git = home.join(".config/git");
        if let Ok(entries) = fs::read_dir(&config_git) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str())
                    && name.ends_with(".gitconfig")
                    && name != "config"
                {
                    files.push(path);
                }
            }
        }
    }

    files
}

/// Parses a single gitconfig file using git
fn parse_gitconfig_file(path: &Path) -> Option<Profile> {
    let output = Command::new("git")
        .args(["config", "--file", &path.to_string_lossy(), "--list"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;

    let mut user_name = None;
    let mut user_email = None;
    let mut signing_key = None;
    let mut gpg_sign = None;

    for line in stdout.lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key {
                "user.name" => user_name = Some(value.to_string()),
                "user.email" => user_email = Some(value.to_string()),
                "user.signingkey" => signing_key = Some(value.to_string()),
                "commit.gpgsign" => gpg_sign = parse_bool(value),
                _ => {}
            }
        }
    }

    // Skip files without user config
    if user_name.is_none() && user_email.is_none() {
        return None;
    }

    let name = derive_profile_name(path);

    Some(Profile {
        name,
        source: path.to_path_buf(),
        user_name,
        user_email,
        signing_key,
        gpg_sign,
    })
}

/// Parses the output of `git config --list --show-origin`
fn parse_git_config_output(output: &str) -> Vec<Profile> {
    let mut entries_by_file: HashMap<PathBuf, Vec<(String, String)>> = HashMap::new();

    for line in output.lines() {
        if let Some((source, key, value)) = parse_config_line(line) {
            entries_by_file
                .entry(source)
                .or_default()
                .push((key, value));
        }
    }

    let mut profiles = Vec::new();

    for (source, entries) in entries_by_file {
        let mut user_name = None;
        let mut user_email = None;
        let mut signing_key = None;
        let mut gpg_sign = None;

        // Last-value-wins for duplicate keys (matches git behavior)
        for (key, value) in entries {
            match key.as_str() {
                "user.name" => user_name = Some(value),
                "user.email" => user_email = Some(value),
                "user.signingkey" => signing_key = Some(value),
                "commit.gpgsign" => gpg_sign = parse_bool(&value),
                _ => {}
            }
        }

        if user_name.is_none() && user_email.is_none() {
            continue;
        }

        let name = derive_profile_name(&source);

        profiles.push(Profile {
            name,
            source,
            user_name,
            user_email,
            signing_key,
            gpg_sign,
        });
    }

    profiles.sort_by(|a, b| a.name.cmp(&b.name));

    profiles
}

/// Parses a single line from git config --show-origin output.
///
/// Format: `file:/path/to/file<TAB>key=value`
fn parse_config_line(line: &str) -> Option<(PathBuf, String, String)> {
    let (origin, rest) = line.split_once('\t')?;

    let path_str = origin.strip_prefix("file:")?;
    let source = PathBuf::from(path_str);

    let (key, value) = rest.split_once('=')?;

    Some((source, key.to_string(), value.to_string()))
}

/// Derives a profile name from a gitconfig file path
fn derive_profile_name(path: &Path) -> String {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    // Handle common gitconfig naming patterns
    match file_name {
        ".gitconfig" | "config" => {
            // Use parent directory name for disambiguation
            if let Some(parent) = path.parent()
                && let Some(parent_name) = parent.file_name().and_then(|n| n.to_str())
            {
                if parent_name == ".git" {
                    return "local".to_string();
                } else if parent_name == "git" {
                    return "default".to_string();
                }
            }
            "default".to_string()
        }
        name => {
            // Strip common extensions to get profile name
            name.trim_end_matches(".gitconfig")
                .trim_end_matches(".git")
                .to_string()
        }
    }
}

/// Parses a git boolean value
fn parse_bool(value: &str) -> Option<bool> {
    match value.to_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config_line() {
        let line = "file:/Users/test/.gitconfig\tuser.name=Test User";
        let result = parse_config_line(line);

        assert!(result.is_some());
        let (source, key, value) = result.unwrap();
        assert_eq!(source, PathBuf::from("/Users/test/.gitconfig"));
        assert_eq!(key, "user.name");
        assert_eq!(value, "Test User");
    }

    #[test]
    fn test_parse_config_line_with_equals_in_value() {
        let line = "file:/Users/test/.gitconfig\tcore.editor=code --wait";
        let result = parse_config_line(line);

        assert!(result.is_some());
        let (_, key, value) = result.unwrap();
        assert_eq!(key, "core.editor");
        assert_eq!(value, "code --wait");
    }

    #[test]
    fn test_derive_profile_name_gitconfig() {
        assert_eq!(
            derive_profile_name(Path::new("/Users/test/.gitconfig")),
            "default"
        );
    }

    #[test]
    fn test_derive_profile_name_named() {
        assert_eq!(
            derive_profile_name(Path::new("/Users/test/.config/git/work.gitconfig")),
            "work"
        );
    }

    #[test]
    fn test_derive_profile_name_local() {
        assert_eq!(
            derive_profile_name(Path::new("/project/.git/config")),
            "local"
        );
    }

    #[test]
    fn test_parse_git_config_output() {
        let output = r#"file:/Users/test/.gitconfig	user.name=Default User
file:/Users/test/.gitconfig	user.email=default@example.com
file:/Users/test/.config/git/work.gitconfig	user.name=Work User
file:/Users/test/.config/git/work.gitconfig	user.email=work@company.com
file:/Users/test/.config/git/work.gitconfig	commit.gpgsign=true"#;

        let profiles = parse_git_config_output(output);

        assert_eq!(profiles.len(), 2);

        // Profiles are sorted by name
        assert_eq!(profiles[0].name, "default");
        assert_eq!(profiles[0].user_name, Some("Default User".to_string()));
        assert_eq!(
            profiles[0].user_email,
            Some("default@example.com".to_string())
        );

        assert_eq!(profiles[1].name, "work");
        assert_eq!(profiles[1].user_name, Some("Work User".to_string()));
        assert_eq!(profiles[1].user_email, Some("work@company.com".to_string()));
        assert_eq!(profiles[1].gpg_sign, Some(true));
    }

    #[test]
    fn test_parse_git_config_output_skips_files_without_user_config() {
        let output = r#"file:/Users/test/.gitconfig	core.editor=vim
file:/Users/test/.gitconfig	core.pager=less"#;

        let profiles = parse_git_config_output(output);
        assert!(profiles.is_empty());
    }

    #[test]
    fn test_parse_git_config_output_last_value_wins() {
        let output = r#"file:/Users/test/.gitconfig	user.name=First
file:/Users/test/.gitconfig	user.name=Second"#;

        let profiles = parse_git_config_output(output);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].user_name, Some("Second".to_string()));
    }

    #[test]
    fn test_parse_bool() {
        assert_eq!(parse_bool("true"), Some(true));
        assert_eq!(parse_bool("True"), Some(true));
        assert_eq!(parse_bool("yes"), Some(true));
        assert_eq!(parse_bool("on"), Some(true));
        assert_eq!(parse_bool("1"), Some(true));

        assert_eq!(parse_bool("false"), Some(false));
        assert_eq!(parse_bool("False"), Some(false));
        assert_eq!(parse_bool("no"), Some(false));
        assert_eq!(parse_bool("off"), Some(false));
        assert_eq!(parse_bool("0"), Some(false));

        assert_eq!(parse_bool("invalid"), None);
    }

    #[test]
    fn test_profile_config_summary() {
        let profile = Profile {
            name: "test".to_string(),
            source: PathBuf::from("/test"),
            user_name: Some("John Doe".to_string()),
            user_email: Some("john@example.com".to_string()),
            signing_key: None,
            gpg_sign: Some(true),
        };

        assert_eq!(
            profile.config_summary(),
            "Name: John Doe, Email: john@example.com, GPG signing: enabled"
        );
    }

    #[test]
    fn test_profile_config_summary_with_key() {
        let profile = Profile {
            name: "test".to_string(),
            source: PathBuf::from("/test"),
            user_name: Some("Jane Doe".to_string()),
            user_email: Some("jane@example.com".to_string()),
            signing_key: Some("ABC123".to_string()),
            gpg_sign: Some(false),
        };

        assert_eq!(
            profile.config_summary(),
            "Name: Jane Doe, Email: jane@example.com, GPG key: ABC123"
        );
    }
}
