use anyhow::{Context, Result};
use console::Term;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::expand_tilde;
use crate::git;
use crate::term::{MenuLevel, format_home_path, is_cancelled};

/// Error message when no profiles are found
pub const NO_PROFILES_ERROR: &str =
    "No git profiles found. Configure user.name/user.email in a gitconfig file.";

/// Context for profile resolution - provides path/URL for includeIf matching
#[derive(Debug, Default)]
pub struct ProfileContext {
    /// Target repository path (for gitdir: matching)
    pub target_path: Option<PathBuf>,
    /// Clone URL (for hasconfig:remote.*.url: matching)
    pub clone_url: Option<String>,
}

impl ProfileContext {
    pub fn new(path: PathBuf, url: Option<String>) -> Self {
        Self {
            target_path: Some(path),
            clone_url: url,
        }
    }
}

/// An includeIf rule parsed from a gitconfig file
#[derive(Debug, Clone)]
struct IncludeIfRule {
    /// The condition type and pattern (e.g., "gitdir:~/work/")
    condition: String,
    /// The included config file path
    target_path: PathBuf,
}

impl IncludeIfRule {
    /// Checks if this rule matches the given context
    fn matches(&self, context: &ProfileContext) -> bool {
        if let Some(pattern) = self.condition.strip_prefix("gitdir:") {
            return Self::matches_gitdir(pattern, context, false);
        }
        if let Some(pattern) = self.condition.strip_prefix("gitdir/i:") {
            return Self::matches_gitdir(pattern, context, true);
        }
        if let Some(pattern) = self.condition.strip_prefix("hasconfig:remote.*.url:") {
            return Self::matches_url(pattern, context);
        }
        false
    }

    /// Matches gitdir: patterns against the target path
    fn matches_gitdir(pattern: &str, context: &ProfileContext, case_insensitive: bool) -> bool {
        let Some(target) = &context.target_path else {
            return false;
        };

        let target = match target.canonicalize() {
            Ok(p) => p,
            Err(_) => target.clone(),
        };

        let pattern_path = expand_tilde(pattern);

        let pattern_normalized = match pattern_path.canonicalize() {
            Ok(p) => p,
            Err(_) => pattern_path,
        };

        let target_str = target.to_string_lossy();
        let pattern_str = pattern_normalized.to_string_lossy();

        let (target_cmp, pattern_cmp) = if case_insensitive {
            (target_str.to_lowercase(), pattern_str.to_lowercase())
        } else {
            (target_str.to_string(), pattern_str.to_string())
        };

        if pattern.ends_with('/') || pattern.ends_with("/**") {
            // Directory prefix match
            let prefix = pattern_cmp.trim_end_matches('/').trim_end_matches("**");
            target_cmp.starts_with(prefix)
        } else if pattern.contains('*') {
            // Glob pattern - simple wildcard matching
            glob_match(&pattern_cmp, &target_cmp)
        } else {
            // Exact match
            target_cmp == pattern_cmp
        }
    }

    /// Matches hasconfig:remote.*.url: patterns against the clone URL
    fn matches_url(pattern: &str, context: &ProfileContext) -> bool {
        let Some(url) = &context.clone_url else {
            return false;
        };

        glob_match(pattern, url)
    }
}

/// Simple glob matching supporting * and **
fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern_parts: Vec<&str> = pattern.split('*').collect();

    if pattern_parts.len() == 1 {
        return pattern == text;
    }

    let mut pos = 0;
    for (i, part) in pattern_parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }

        if i == 0 {
            if !text.starts_with(part) {
                return false;
            }
            pos = part.len();
        } else if i == pattern_parts.len() - 1 {
            if !text.ends_with(part) {
                return false;
            }
        } else if let Some(found) = text[pos..].find(part) {
            pos += found + part.len();
        } else {
            return false;
        }
    }

    true
}

/// Parses includeIf rules from all gitconfig files
fn parse_include_if_rules() -> Vec<IncludeIfRule> {
    let mut rules = Vec::new();

    if let Some(home) = dirs::home_dir() {
        let main_gitconfig = home.join(".gitconfig");
        if main_gitconfig.exists() {
            rules.extend(parse_include_if_from_file(&main_gitconfig));
        }

        let xdg_config = home.join(".config/git/config");
        if xdg_config.exists() {
            rules.extend(parse_include_if_from_file(&xdg_config));
        }
    }

    rules
}

/// Parses includeIf rules from a single gitconfig file
fn parse_include_if_from_file(path: &Path) -> Vec<IncludeIfRule> {
    let mut rules = Vec::new();

    let Ok(content) = fs::read_to_string(path) else {
        return rules;
    };

    let mut current_condition: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();

        if let Some(condition) = line
            .strip_prefix("[includeIf \"")
            .and_then(|s| s.strip_suffix("\"]"))
        {
            current_condition = Some(condition.to_string());
        } else if line.starts_with('[') {
            current_condition = None;
        } else if let Some(ref condition) = current_condition
            && let Some(path_value) = line
                .strip_prefix("path")
                .and_then(|s| s.trim_start().strip_prefix('='))
                .map(str::trim)
        {
            rules.push(IncludeIfRule {
                condition: condition.clone(),
                target_path: expand_tilde(path_value),
            });
        }
    }

    rules
}

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
    /// Git gpg.format value (openpgp, x509, ssh)
    pub gpg_format: Option<String>,
    /// Git tag.gpgsign value
    pub tag_gpg_sign: Option<bool>,
    /// Whether this profile is the configured yarm default
    pub is_default: bool,
}

/// A profile field with its display label and value
pub struct ProfileField<'a> {
    pub label: &'static str,
    pub value: &'a str,
}

impl Profile {
    /// Returns the identity as "Name <Email>", "Name", or "<Email>" depending on available fields
    pub fn identity(&self) -> Option<String> {
        match (self.user_name.as_deref(), self.user_email.as_deref()) {
            (Some(name), Some(email)) => Some(format!("{name} <{email}>")),
            (Some(name), None) => Some(name.to_string()),
            (None, Some(email)) => Some(format!("<{email}>")),
            (None, None) => None,
        }
    }

    /// Returns an iterator over the profile's non-identity fields
    pub fn fields(&self) -> impl Iterator<Item = ProfileField<'_>> {
        let key = self.signing_key.as_deref().map(|v| ProfileField {
            label: "Signing key",
            value: v,
        });
        let gpg_format = self.gpg_format.as_deref().map(|v| ProfileField {
            label: "Signing format",
            value: v,
        });
        let gpg_sign = self.gpg_sign.map(|v| ProfileField {
            label: "Sign commits",
            value: if v { "enabled" } else { "disabled" },
        });
        let tag_gpg_sign = self.tag_gpg_sign.map(|v| ProfileField {
            label: "Sign tags",
            value: if v { "enabled" } else { "disabled" },
        });

        [key, gpg_format, gpg_sign, tag_gpg_sign]
            .into_iter()
            .flatten()
    }

    /// Returns a display string showing the config values that were applied
    pub fn config_summary(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(identity) = self.identity() {
            parts.push(identity);
        }
        parts.extend(self.fields().map(|f| format!("{}: {}", f.label, f.value)));
        parts.join(", ")
    }

    /// Returns a display string for menu selection: "name (~/path/to/source)"
    pub fn display_option(&self) -> String {
        format!("{} ({})", self.name, format_home_path(&self.source))
    }
}

/// Discovers git identity profiles from gitconfig files.
///
/// This discovers profiles from three sources:
/// 1. Files git knows about (`git config --list --show-origin`)
/// 2. Additional `*.gitconfig` files in common locations
/// 3. Custom directories from yarm configuration (`~/.config/yarm.toml`)
///
/// Profiles are ordered: current effective profile first, then git-known
/// profiles alphabetically, then additional discovered profiles alphabetically.
pub fn discover_profiles() -> Result<Vec<Profile>> {
    let config = crate::config::load()?;
    let extra_paths = config.profile_paths();

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
        let stdout =
            String::from_utf8(output.stdout).context("Invalid UTF-8 in git config output")?;
        for profile in parse_git_config_output(&stdout) {
            seen_sources.insert(profile.source.clone());
            git_profiles.push(profile);
        }
    }

    for path in find_gitconfig_files(&extra_paths) {
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
        git_profiles
            .iter()
            .position(|p| p.user_email.as_ref() == Some(email))
    });

    let mut profiles = Vec::new();

    // Add current profile first if found
    if let Some(idx) = current_idx {
        profiles.push(git_profiles.remove(idx));
    }

    profiles.extend(git_profiles);
    profiles.extend(additional_profiles);

    if let Some(default_name) = config.profiles.default.as_deref()
        && let Some(p) = profiles.iter_mut().find(|p| p.name == default_name)
    {
        p.is_default = true;
    }

    Ok(profiles)
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

    let source_display = format_home_path(&profile.source);
    parts.push(format!("({source_display})"));

    parts.join(" ")
}

/// Discovers and resolves a profile with context for includeIf matching.
///
/// Profiles matching includeIf rules for the given context are promoted to the top.
/// Returns `Ok(None)` if the user cancels the interactive selection.
pub fn resolve_profile_with_context(
    profile_name: Option<&str>,
    context: &ProfileContext,
) -> Result<Option<Profile>> {
    let config = crate::config::load()?;
    let profiles = discover_profiles()?;

    if profiles.is_empty() {
        anyhow::bail!(NO_PROFILES_ERROR);
    }

    let profiles =
        reorder_profiles_by_context(profiles, context, config.profiles.default.as_deref());

    match profile_name {
        Some(name) => find_profile_by_name(&profiles, name).map(Some),
        None => select_profile(profiles),
    }
}

/// Reorders profiles so those matching includeIf rules come first.
/// Falls back to promoting the configured default profile if no rules match.
fn reorder_profiles_by_context(
    profiles: Vec<Profile>,
    context: &ProfileContext,
    default_profile: Option<&str>,
) -> Vec<Profile> {
    if context.target_path.is_some() || context.clone_url.is_some() {
        let rules = parse_include_if_rules();
        return reorder_profiles_by_rules(profiles, context, &rules, default_profile);
    }

    promote_default(profiles, default_profile)
}

/// Reorders profiles by matching against the given includeIf rules.
/// Falls back to promoting the configured default profile if no rules match.
fn reorder_profiles_by_rules(
    profiles: Vec<Profile>,
    context: &ProfileContext,
    rules: &[IncludeIfRule],
    default_profile: Option<&str>,
) -> Vec<Profile> {
    if !rules.is_empty() {
        let matching_sources: HashSet<PathBuf> = rules
            .iter()
            .filter(|rule| rule.matches(context))
            .map(|rule| rule.target_path.clone())
            .collect();

        if !matching_sources.is_empty() {
            let mut matching = Vec::new();
            let mut non_matching = Vec::new();

            for profile in profiles {
                let source_canonical = profile
                    .source
                    .canonicalize()
                    .unwrap_or_else(|_| profile.source.clone());
                let matches = matching_sources.iter().any(|rule_target| {
                    let target_canonical = rule_target
                        .canonicalize()
                        .unwrap_or_else(|_| rule_target.clone());
                    source_canonical == target_canonical
                });

                if matches {
                    matching.push(profile);
                } else {
                    non_matching.push(profile);
                }
            }

            matching.extend(non_matching);
            return matching;
        }
    }

    promote_default(profiles, default_profile)
}

/// Promotes the configured default profile to the top of the list.
fn promote_default(mut profiles: Vec<Profile>, default_name: Option<&str>) -> Vec<Profile> {
    let Some(name) = default_name else {
        return profiles;
    };

    if let Some(idx) = profiles.iter().position(|p| p.name == name) {
        let default = profiles.remove(idx);
        profiles.insert(0, default);
    }

    profiles
}

/// Interactive profile selection
/// Returns `Ok(None)` if the user cancels.
fn select_profile(profiles: Vec<Profile>) -> Result<Option<Profile>> {
    let options: Vec<String> = profiles.iter().map(format_profile_display).collect();

    let selection = match MenuLevel::Sub
        .select_filterable("Select profile:", options.clone())
        .prompt()
    {
        Ok(s) => s,
        Err(e) if is_cancelled(&e) => return Ok(None),
        Err(e) => return Err(e).context("Profile selection failed"),
    };

    let selected_idx = options
        .iter()
        .position(|s| s == &selection)
        .ok_or_else(|| anyhow::anyhow!("Failed to find selected profile"))?;

    let selected = profiles.into_iter().nth(selected_idx).unwrap();

    let term = Term::stdout();
    let _ = term.clear_last_lines(1);

    Ok(Some(selected))
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

    if let Some(ref format) = profile.gpg_format {
        git::set_config(repo_path, "gpg.format", Some(format))?;
    }

    if let Some(gpg_sign) = profile.gpg_sign {
        git::set_config(
            repo_path,
            "commit.gpgsign",
            Some(if gpg_sign { "true" } else { "false" }),
        )?;
    }

    if let Some(tag_gpg_sign) = profile.tag_gpg_sign {
        git::set_config(
            repo_path,
            "tag.gpgsign",
            Some(if tag_gpg_sign { "true" } else { "false" }),
        )?;
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

/// Finds gitconfig files in common locations and custom directories
fn find_gitconfig_files(extra_dirs: &[PathBuf]) -> Vec<PathBuf> {
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

    for dir in extra_dirs {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    files.push(path);
                }
            }
        }
    }

    files
}

/// Parses a single gitconfig file using git
/// Accumulates git config key-value pairs into profile fields.
#[derive(Default)]
struct ProfileFields {
    user_name: Option<String>,
    user_email: Option<String>,
    signing_key: Option<String>,
    gpg_sign: Option<bool>,
    gpg_format: Option<String>,
    tag_gpg_sign: Option<bool>,
}

impl ProfileFields {
    fn apply(&mut self, key: &str, value: String) {
        match key {
            "user.name" => self.user_name = Some(value),
            "user.email" => self.user_email = Some(value),
            "user.signingkey" => self.signing_key = Some(value),
            "commit.gpgsign" => self.gpg_sign = parse_bool(&value),
            "gpg.format" => self.gpg_format = Some(value),
            "tag.gpgsign" => self.tag_gpg_sign = parse_bool(&value),
            _ => {}
        }
    }

    fn has_user_config(&self) -> bool {
        self.user_name.is_some() || self.user_email.is_some()
    }

    fn into_profile(self, source: PathBuf) -> Profile {
        let name = derive_profile_name(&source);
        Profile {
            name,
            source,
            user_name: self.user_name,
            user_email: self.user_email,
            signing_key: self.signing_key,
            gpg_sign: self.gpg_sign,
            gpg_format: self.gpg_format,
            tag_gpg_sign: self.tag_gpg_sign,
            is_default: false,
        }
    }
}

fn parse_gitconfig_file(path: &Path) -> Option<Profile> {
    let output = Command::new("git")
        .args(["config", "--file", &path.to_string_lossy(), "--list"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;

    let mut fields = ProfileFields::default();
    for line in stdout.lines() {
        if let Some((key, value)) = line.split_once('=') {
            fields.apply(key, value.to_string());
        }
    }

    if !fields.has_user_config() {
        return None;
    }

    Some(fields.into_profile(path.to_path_buf()))
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
        let mut fields = ProfileFields::default();
        for (key, value) in entries {
            fields.apply(&key, value);
        }

        if !fields.has_user_config() {
            continue;
        }

        profiles.push(fields.into_profile(source));
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
                    return "global".to_string();
                }
            }
            "global".to_string()
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
    fn test_parse_config_line_preserves_equals_in_value() {
        let line = "file:/Users/test/.gitconfig\tcore.sshCommand=ssh -o SendEnv=GIT_PROTOCOL";
        let (_, key, value) = parse_config_line(line).unwrap();
        assert_eq!(key, "core.sshCommand");
        assert_eq!(value, "ssh -o SendEnv=GIT_PROTOCOL");
    }

    #[test]
    fn test_derive_profile_name_gitconfig() {
        assert_eq!(
            derive_profile_name(Path::new("/Users/test/.gitconfig")),
            "global"
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
        let output = r"file:/Users/test/.gitconfig	user.name=Default User
file:/Users/test/.gitconfig	user.email=default@example.com
file:/Users/test/.config/git/work.gitconfig	user.name=Work User
file:/Users/test/.config/git/work.gitconfig	user.email=work@company.com
file:/Users/test/.config/git/work.gitconfig	commit.gpgsign=true";

        let profiles = parse_git_config_output(output);

        assert_eq!(profiles.len(), 2);

        // Profiles are sorted by name
        assert_eq!(profiles[0].name, "global");
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
        let output = r"file:/Users/test/.gitconfig	core.editor=vim
file:/Users/test/.gitconfig	core.pager=less";

        let profiles = parse_git_config_output(output);
        assert!(profiles.is_empty());
    }

    #[test]
    fn test_parse_git_config_output_last_value_wins() {
        let output = r"file:/Users/test/.gitconfig	user.name=First
file:/Users/test/.gitconfig	user.name=Second";

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
            gpg_format: None,
            tag_gpg_sign: None,
            is_default: false,
        };

        assert_eq!(
            profile.config_summary(),
            "John Doe <john@example.com>, Sign commits: enabled"
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
            gpg_format: Some("ssh".to_string()),
            tag_gpg_sign: Some(true),
            is_default: false,
        };

        assert_eq!(
            profile.config_summary(),
            "Jane Doe <jane@example.com>, Signing key: ABC123, Signing format: ssh, Sign commits: disabled, Sign tags: enabled"
        );
    }

    #[test]
    fn test_glob_match_exact() {
        assert!(glob_match("hello", "hello"));
        assert!(!glob_match("hello", "world"));
    }

    #[test]
    fn test_glob_match_wildcard() {
        assert!(glob_match("*.com", "example.com"));
        assert!(glob_match("*.com", "test.com"));
        assert!(!glob_match("*.com", "example.org"));
    }

    #[test]
    fn test_glob_match_prefix_suffix() {
        assert!(glob_match("https://*", "https://github.com"));
        assert!(glob_match("*github.com*", "https://github.com/user/repo"));
        assert!(!glob_match("https://*", "http://github.com"));
    }

    #[test]
    fn test_glob_match_middle() {
        assert!(glob_match("*github*repo*", "https://github.com/user/repo"));
        assert!(!glob_match("*gitlab*repo*", "https://github.com/user/repo"));
    }

    #[test]
    fn test_include_if_rule_url_match() {
        let rule = IncludeIfRule {
            condition: "hasconfig:remote.*.url:*github.com/mycompany/*".to_string(),
            target_path: PathBuf::from("/test"),
        };

        let matching_context = ProfileContext {
            target_path: None,
            clone_url: Some("https://github.com/mycompany/project.git".to_string()),
        };
        assert!(rule.matches(&matching_context));

        let non_matching_context = ProfileContext {
            target_path: None,
            clone_url: Some("https://github.com/other/project.git".to_string()),
        };
        assert!(!rule.matches(&non_matching_context));
    }

    // --- Profile::identity() ---

    #[test]
    fn test_identity_name_and_email() {
        let p = test_profile("t", Some("Alice"), Some("alice@ex.com"));
        assert_eq!(p.identity(), Some("Alice <alice@ex.com>".to_string()));
    }

    #[test]
    fn test_identity_name_only() {
        let p = test_profile("t", Some("Alice"), None);
        assert_eq!(p.identity(), Some("Alice".to_string()));
    }

    #[test]
    fn test_identity_email_only() {
        let p = test_profile("t", None, Some("alice@ex.com"));
        assert_eq!(p.identity(), Some("<alice@ex.com>".to_string()));
    }

    #[test]
    fn test_identity_none() {
        let p = test_profile("t", None, None);
        assert_eq!(p.identity(), None);
    }

    // --- find_profile_by_name ---

    fn sample_profiles() -> Vec<Profile> {
        vec![
            test_profile_with_source(
                "work",
                "/home/user/.gitconfig-work",
                Some("Work"),
                Some("w@co.com"),
            ),
            test_profile_with_source(
                ".personal",
                "/home/user/.personal",
                Some("Me"),
                Some("me@me.com"),
            ),
            test_profile_with_source(
                ".gitconfig-oss",
                "/home/user/.gitconfig-oss",
                Some("OSS"),
                Some("oss@ex.com"),
            ),
            test_profile_with_source(
                "global",
                "/home/user/.gitconfig",
                Some("Default"),
                Some("d@ex.com"),
            ),
        ]
    }

    #[test]
    fn test_find_profile_by_name_exact() {
        let profiles = sample_profiles();
        let found = find_profile_by_name(&profiles, "work").unwrap();
        assert_eq!(found.name, "work");
    }

    #[test]
    fn test_find_profile_by_name_exact_path() {
        let profiles = sample_profiles();
        let found = find_profile_by_name(&profiles, "/home/user/.gitconfig-work").unwrap();
        assert_eq!(found.name, "work");
    }

    #[test]
    fn test_find_profile_by_name_dot_prefix_fallback() {
        let profiles = sample_profiles();
        let found = find_profile_by_name(&profiles, "personal").unwrap();
        assert_eq!(found.name, ".personal");
    }

    #[test]
    fn test_find_profile_by_name_gitconfig_prefix_fallback() {
        let profiles = sample_profiles();
        let found = find_profile_by_name(&profiles, "oss").unwrap();
        assert_eq!(found.name, ".gitconfig-oss");
    }

    #[test]
    fn test_find_profile_by_name_not_found() {
        let profiles = sample_profiles();
        let result = find_profile_by_name(&profiles, "nonexistent");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("nonexistent"));
        assert!(err.contains("not found"));
    }

    // --- promote_default ---

    #[test]
    fn test_promote_default_moves_to_front() {
        let profiles = vec![
            test_profile("alpha", Some("A"), Some("a@ex.com")),
            test_profile("beta", Some("B"), Some("b@ex.com")),
            test_profile("gamma", Some("C"), Some("c@ex.com")),
        ];
        let result = promote_default(profiles, Some("gamma"));
        assert_eq!(result[0].name, "gamma");
        assert_eq!(result[1].name, "alpha");
        assert_eq!(result[2].name, "beta");
    }

    #[test]
    fn test_promote_default_none_is_noop() {
        let profiles = vec![
            test_profile("alpha", Some("A"), Some("a@ex.com")),
            test_profile("beta", Some("B"), Some("b@ex.com")),
        ];
        let result = promote_default(profiles, None);
        assert_eq!(result[0].name, "alpha");
        assert_eq!(result[1].name, "beta");
    }

    #[test]
    fn test_promote_default_not_found_is_noop() {
        let profiles = vec![
            test_profile("alpha", Some("A"), Some("a@ex.com")),
            test_profile("beta", Some("B"), Some("b@ex.com")),
        ];
        let result = promote_default(profiles, Some("nonexistent"));
        assert_eq!(result[0].name, "alpha");
        assert_eq!(result[1].name, "beta");
    }

    #[test]
    fn test_promote_default_already_first() {
        let profiles = vec![
            test_profile("alpha", Some("A"), Some("a@ex.com")),
            test_profile("beta", Some("B"), Some("b@ex.com")),
        ];
        let result = promote_default(profiles, Some("alpha"));
        assert_eq!(result[0].name, "alpha");
        assert_eq!(result[1].name, "beta");
    }

    // --- derive_profile_name edge cases ---

    #[test]
    fn test_derive_profile_name_gitconfig_dash_prefix() {
        assert_eq!(
            derive_profile_name(Path::new("/home/user/.gitconfig-work")),
            ".gitconfig-work"
        );
    }

    #[test]
    fn test_derive_profile_name_gitconfig_dot_prefix() {
        assert_eq!(
            derive_profile_name(Path::new("/home/user/.gitconfig.personal")),
            ".gitconfig.personal"
        );
    }

    #[test]
    fn test_derive_profile_name_xdg_config() {
        assert_eq!(
            derive_profile_name(Path::new("/home/user/.config/git/config")),
            "global"
        );
    }

    #[test]
    fn test_derive_profile_name_bare_file() {
        assert_eq!(
            derive_profile_name(Path::new("/custom/profiles/myprofile")),
            "myprofile"
        );
    }

    // --- IncludeIfRule::matches_gitdir ---

    #[test]
    fn test_matches_gitdir_directory_prefix() {
        let tmp = tempdir("gitdir-prefix");
        let sub = tmp.join("work").join("project");
        std::fs::create_dir_all(&sub).unwrap();

        let rule = IncludeIfRule {
            condition: format!("gitdir:{}/", tmp.join("work").display()),
            target_path: PathBuf::from("/dummy"),
        };
        let ctx = ProfileContext {
            target_path: Some(sub),
            clone_url: None,
        };
        assert!(rule.matches(&ctx));
    }

    #[test]
    fn test_matches_gitdir_directory_prefix_no_match() {
        let tmp = tempdir("gitdir-prefix-no");
        let sub = tmp.join("personal").join("project");
        std::fs::create_dir_all(&sub).unwrap();

        let rule = IncludeIfRule {
            condition: format!("gitdir:{}/", tmp.join("work").display()),
            target_path: PathBuf::from("/dummy"),
        };
        let ctx = ProfileContext {
            target_path: Some(sub),
            clone_url: None,
        };
        assert!(!rule.matches(&ctx));
    }

    #[test]
    fn test_matches_gitdir_case_insensitive() {
        let tmp = tempdir("gitdir-case");
        let sub = tmp.join("Work").join("project");
        std::fs::create_dir_all(&sub).unwrap();

        let rule = IncludeIfRule {
            condition: format!("gitdir/i:{}/", tmp.join("work").display()),
            target_path: PathBuf::from("/dummy"),
        };
        let ctx = ProfileContext {
            target_path: Some(sub),
            clone_url: None,
        };
        assert!(rule.matches(&ctx));
    }

    #[test]
    fn test_matches_gitdir_no_target_path() {
        let rule = IncludeIfRule {
            condition: "gitdir:/some/path/".to_string(),
            target_path: PathBuf::from("/dummy"),
        };
        let ctx = ProfileContext {
            target_path: None,
            clone_url: None,
        };
        assert!(!rule.matches(&ctx));
    }

    #[test]
    fn test_matches_url_no_clone_url() {
        let rule = IncludeIfRule {
            condition: "hasconfig:remote.*.url:*github.com*".to_string(),
            target_path: PathBuf::from("/dummy"),
        };
        let ctx = ProfileContext {
            target_path: None,
            clone_url: None,
        };
        assert!(!rule.matches(&ctx));
    }

    #[test]
    fn test_matches_unknown_condition() {
        let rule = IncludeIfRule {
            condition: "onbranch:main".to_string(),
            target_path: PathBuf::from("/dummy"),
        };
        let ctx = ProfileContext {
            target_path: Some(PathBuf::from("/some/path")),
            clone_url: Some("https://github.com/user/repo".to_string()),
        };
        assert!(!rule.matches(&ctx));
    }

    #[test]
    fn test_matches_gitdir_double_star_suffix() {
        let tmp = tempdir("gitdir-dstar");
        let work = tmp.join("work");
        let sub = work.join("deep").join("project");
        std::fs::create_dir_all(&sub).unwrap();

        // Canonicalize the work dir so the pattern matches the canonicalized target
        let work_canonical = work.canonicalize().unwrap();
        let rule = IncludeIfRule {
            condition: format!("gitdir:{}/**", work_canonical.display()),
            target_path: PathBuf::from("/dummy"),
        };
        let ctx = ProfileContext {
            target_path: Some(sub),
            clone_url: None,
        };
        assert!(rule.matches(&ctx));
    }

    // --- parse_include_if_from_file ---

    #[test]
    fn test_parse_include_if_from_file_basic() {
        let tmp = tempdir("parse-includeif");
        let config_file = tmp.join("gitconfig");
        std::fs::write(
            &config_file,
            r#"[includeIf "gitdir:~/work/"]
	path = ~/.config/git/work.gitconfig
[includeIf "hasconfig:remote.*.url:*github.com/company/*"]
	path = ~/.config/git/company.gitconfig
[user]
	name = Default
"#,
        )
        .unwrap();

        let rules = parse_include_if_from_file(&config_file);
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].condition, "gitdir:~/work/");
        assert_eq!(
            rules[1].condition,
            "hasconfig:remote.*.url:*github.com/company/*"
        );
    }

    #[test]
    fn test_parse_include_if_from_file_no_rules() {
        let tmp = tempdir("parse-includeif-none");
        let config_file = tmp.join("gitconfig");
        std::fs::write(&config_file, "[user]\n\tname = Test\n").unwrap();

        let rules = parse_include_if_from_file(&config_file);
        assert!(rules.is_empty());
    }

    #[test]
    fn test_parse_include_if_from_file_nonexistent() {
        let rules = parse_include_if_from_file(Path::new("/nonexistent/gitconfig"));
        assert!(rules.is_empty());
    }

    // --- glob_match edge cases ---

    #[test]
    fn test_glob_match_single_star_matches_anything() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("*", ""));
    }

    #[test]
    fn test_glob_match_double_star_matches_anything() {
        assert!(glob_match("**", "anything/with/slashes"));
    }

    #[test]
    fn test_glob_match_empty_pattern_empty_text() {
        assert!(glob_match("", ""));
        assert!(!glob_match("", "notempty"));
    }

    #[test]
    fn test_glob_match_no_wildcard_must_be_exact() {
        assert!(glob_match("/exact/path", "/exact/path"));
        assert!(!glob_match("/exact/path", "/exact/path/extra"));
    }

    // --- reorder_profiles_by_context (with injected rules) ---

    #[test]
    fn test_reorder_by_matching_rules() {
        let profiles = vec![
            test_profile_with_source(
                "personal",
                "/home/user/.gitconfig-personal",
                Some("P"),
                Some("p@ex.com"),
            ),
            test_profile_with_source(
                "work",
                "/home/user/.gitconfig-work",
                Some("W"),
                Some("w@co.com"),
            ),
        ];

        let rules = vec![IncludeIfRule {
            condition: "hasconfig:remote.*.url:*company.com*".to_string(),
            target_path: PathBuf::from("/home/user/.gitconfig-work"),
        }];

        let context = ProfileContext {
            target_path: None,
            clone_url: Some("https://company.com/repo.git".to_string()),
        };

        let result = reorder_profiles_by_rules(profiles, &context, &rules, None);
        assert_eq!(result[0].name, "work");
        assert_eq!(result[1].name, "personal");
    }

    #[test]
    fn test_reorder_no_matching_rules_falls_back_to_default() {
        let profiles = vec![
            test_profile("alpha", Some("A"), Some("a@ex.com")),
            test_profile("beta", Some("B"), Some("b@ex.com")),
        ];

        let rules = vec![IncludeIfRule {
            condition: "hasconfig:remote.*.url:*nomatch*".to_string(),
            target_path: PathBuf::from("/dummy"),
        }];

        let context = ProfileContext {
            target_path: None,
            clone_url: Some("https://github.com/user/repo.git".to_string()),
        };

        let result = reorder_profiles_by_rules(profiles, &context, &rules, Some("beta"));
        assert_eq!(result[0].name, "beta");
        assert_eq!(result[1].name, "alpha");
    }

    #[test]
    fn test_reorder_empty_rules_falls_back_to_default() {
        let profiles = vec![
            test_profile("alpha", Some("A"), Some("a@ex.com")),
            test_profile("beta", Some("B"), Some("b@ex.com")),
        ];

        let context = ProfileContext {
            target_path: Some(PathBuf::from("/some/path")),
            clone_url: None,
        };

        let result = reorder_profiles_by_rules(profiles, &context, &[], Some("beta"));
        assert_eq!(result[0].name, "beta");
        assert_eq!(result[1].name, "alpha");
    }

    // --- test helpers ---

    fn test_profile(name: &str, user_name: Option<&str>, user_email: Option<&str>) -> Profile {
        Profile {
            name: name.to_string(),
            source: PathBuf::from(format!("/test/{name}")),
            user_name: user_name.map(String::from),
            user_email: user_email.map(String::from),
            signing_key: None,
            gpg_sign: None,
            gpg_format: None,
            tag_gpg_sign: None,
            is_default: false,
        }
    }

    fn test_profile_with_source(
        name: &str,
        source: &str,
        user_name: Option<&str>,
        user_email: Option<&str>,
    ) -> Profile {
        Profile {
            name: name.to_string(),
            source: PathBuf::from(source),
            user_name: user_name.map(String::from),
            user_email: user_email.map(String::from),
            signing_key: None,
            gpg_sign: None,
            gpg_format: None,
            tag_gpg_sign: None,
            is_default: false,
        }
    }

    fn tempdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("yarm-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
