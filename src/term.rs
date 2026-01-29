use anyhow::{Context, Result};
use console::{style, StyledObject, Term};
use inquire::ui::{RenderConfig, Styled};
use inquire::{Confirm, InquireError, Select, Text};
use std::fmt::Display;

/// Returns a styled success icon (green ✓)
pub fn icon_success() -> StyledObject<&'static str> {
    style("✓").green().bold()
}

/// Returns a styled warning icon (yellow !)
pub fn icon_warning() -> StyledObject<&'static str> {
    style("!").yellow().bold()
}

/// Returns a styled error icon (red ✗)
pub fn icon_error() -> StyledObject<&'static str> {
    style("✗").red().bold()
}

/// Prints a header line with bold label (e.g., "Cloning: owner/repo")
pub fn print_header(label: &str, value: impl Display) {
    println!("  {} {}", style(label).bold(), value);
}

/// Prints a success message with green checkmark
pub fn print_success(message: impl Display) {
    println!("  {} {}", icon_success(), message);
}

/// Prints a warning message with yellow exclamation
pub fn print_warning(message: impl Display) {
    println!("  {} {}", icon_warning(), message);
}

/// Manages terminal state for interactive menu sessions.
/// Handles clearing previous menu output between iterations.
pub struct MenuSession {
    term: Term,
    started: bool,
}

impl MenuSession {
    pub fn new() -> Self {
        Self {
            term: Term::stdout(),
            started: false,
        }
    }

    /// Call before showing each menu prompt.
    /// Clears the previous menu line if this isn't the first iteration.
    pub fn prepare(&mut self) {
        if self.started {
            let _ = self.term.clear_last_lines(1);
        }
        self.started = true;
    }
}

/// Menu hierarchy level for contextual help messages
pub enum MenuLevel {
    /// Top-level menu (esc quits the application)
    Top,
    /// Sub-level menu or prompt (esc cancels and returns to parent)
    Sub,
}

impl MenuLevel {
    /// Returns the appropriate help message for this menu level
    pub fn help(&self) -> &'static str {
        match self {
            Self::Top => "esc to quit",
            Self::Sub => "esc to cancel",
        }
    }

    /// Returns help message for filterable menus
    fn help_filterable(&self) -> &'static str {
        match self {
            Self::Top => "type to filter — esc to quit",
            Self::Sub => "type to filter — esc to cancel",
        }
    }

    /// Returns help message with additional context prepended
    pub fn help_with(&self, prefix: &str) -> String {
        format!("{} — {}", prefix, self.help())
    }

    /// Creates a Select prompt configured for this menu level
    /// Uses a simplified header style with dashes
    pub fn select<'a, T: Display>(&self, message: &'a str, options: Vec<T>) -> SimpleSelect<'a, T> {
        let config = RenderConfig::default()
            .with_prompt_prefix(Styled::new("── "))
            .with_answered_prompt_prefix(Styled::new("── "));

        let select = Select::new(message, options)
            .with_help_message(self.help())
            .with_render_config(config);

        SimpleSelect::new(select)
    }

    /// Creates a Select prompt with filtering enabled (for long lists)
    /// Uses case-insensitive substring matching to hide non-matching options.
    /// Shows "(no matches)" placeholder when filter yields no results.
    pub fn select_filterable<'a>(
        &self,
        message: &'a str,
        options: Vec<String>,
    ) -> FilterableSelect<'a> {
        FilterableSelect::new(message, options, self.help_filterable())
    }
}

/// A simple (non-filterable) Select prompt that clears output on cancellation
pub struct SimpleSelect<'a, T: Display> {
    select: Select<'a, T>,
}

impl<'a, T: Display> SimpleSelect<'a, T> {
    fn new(select: Select<'a, T>) -> Self {
        Self { select }
    }

    /// Shows the prompt and returns the selected option
    /// Clears the prompt line on cancellation to prevent terminal growth
    pub fn prompt(self) -> Result<T, InquireError> {
        match self.select.prompt() {
            Ok(result) => Ok(result),
            Err(e) if is_cancelled(&e) => {
                let _ = Term::stdout().clear_last_lines(1);
                Err(e)
            }
            Err(e) => Err(e),
        }
    }
}

/// Placeholder text shown when no options match the filter (unstyled for comparison)
const NO_MATCHES_TEXT: &str = "(no matches)";

/// Returns the styled placeholder string
fn no_matches_placeholder() -> String {
    style(NO_MATCHES_TEXT).dim().to_string()
}

/// A filterable Select prompt that shows a placeholder when no options match
pub struct FilterableSelect<'a> {
    message: &'a str,
    options: Vec<String>,
    help: &'a str,
}

impl<'a> FilterableSelect<'a> {
    fn new(message: &'a str, options: Vec<String>, help: &'a str) -> Self {
        Self { message, options, help }
    }

    /// Shows the prompt and returns the selected option
    /// Clears the prompt line on cancellation to prevent terminal growth
    pub fn prompt(self) -> Result<String, InquireError> {
        let placeholder = no_matches_placeholder();
        let term = Term::stdout();
        let options = self.options;

        loop {
            // Clone options for the scorer closure to check matches
            let options_for_scorer = options.clone();
            let placeholder_for_scorer = placeholder.clone();

            // Build options with placeholder
            let mut all_options = options.clone();
            all_options.push(placeholder.clone());

            let scorer = move |input: &str, _opt: &String, string_value: &str, _idx: usize| -> Option<i64> {
                let input_lower = input.to_lowercase();
                let is_placeholder = string_value == placeholder_for_scorer;

                if is_placeholder {
                    if input.is_empty() {
                        return None;
                    }
                    let any_match = options_for_scorer
                        .iter()
                        .any(|opt| opt.to_lowercase().contains(&input_lower));
                    if any_match {
                        None
                    } else {
                        Some(0)
                    }
                } else {
                    if string_value.to_lowercase().contains(&input_lower) {
                        Some(0)
                    } else {
                        None
                    }
                }
            };

            match Select::new(self.message, all_options)
                .with_help_message(self.help)
                .with_scorer(&scorer)
                .prompt()
            {
                Ok(selection) if selection == placeholder => {
                    let _ = term.clear_last_lines(1);
                    continue;
                }
                Ok(selection) => return Ok(selection),
                Err(e) if is_cancelled(&e) => {
                    let _ = term.clear_last_lines(1);
                    return Err(e);
                }
                Err(e) => return Err(e),
            }
        }
    }
}

/// Checks if the error is a user cancellation (ESC pressed)
pub fn is_cancelled(err: &InquireError) -> bool {
    matches!(err, InquireError::OperationCanceled | InquireError::OperationInterrupted)
}

/// Prompts for required text input, re-prompting if empty.
/// Returns `Ok(None)` if cancelled, `Ok(Some(value))` on success.
pub fn prompt_required_text(prompt: &str, initial_value: Option<&str>) -> Result<Option<String>> {
    let mut builder = Text::new(prompt).with_help_message(MenuLevel::Sub.help());
    if let Some(initial) = initial_value {
        builder = builder.with_initial_value(initial);
    }

    let mut value = match builder.prompt() {
        Ok(s) => s,
        Err(e) if is_cancelled(&e) => return Ok(None),
        Err(e) => return Err(e).context("Input failed"),
    };

    while value.is_empty() {
        print_warning("Name is required");
        value = match Text::new(prompt)
            .with_help_message(MenuLevel::Sub.help())
            .prompt()
        {
            Ok(s) => s,
            Err(e) if is_cancelled(&e) => return Ok(None),
            Err(e) => return Err(e).context("Input failed"),
        };
    }

    Ok(Some(value))
}

/// Prompts for optional text input.
/// Returns `Ok(None)` if cancelled, `Ok(Some(value))` on success.
pub fn prompt_text(prompt: &str, initial_value: Option<&str>) -> Result<Option<String>> {
    let mut builder = Text::new(prompt).with_help_message(MenuLevel::Sub.help());
    if let Some(initial) = initial_value {
        builder = builder.with_initial_value(initial);
    }

    match builder.prompt() {
        Ok(s) => Ok(Some(s)),
        Err(e) if is_cancelled(&e) => Ok(None),
        Err(e) => Err(e).context("Input failed"),
    }
}

/// Prompts for optional text input with custom help message.
/// Returns `Ok(None)` if cancelled, `Ok(Some(value))` on success.
pub fn prompt_text_with_help(prompt: &str, help: &str) -> Result<Option<String>> {
    match Text::new(prompt).with_help_message(help).prompt() {
        Ok(s) => Ok(Some(s)),
        Err(e) if is_cancelled(&e) => Ok(None),
        Err(e) => Err(e).context("Input failed"),
    }
}

/// Prompts for a yes/no confirmation.
/// Returns `Ok(None)` if cancelled, `Ok(Some(bool))` on success.
pub fn prompt_confirm(prompt: &str, default: bool) -> Result<Option<bool>> {
    match Confirm::new(prompt)
        .with_default(default)
        .with_help_message(MenuLevel::Sub.help())
        .prompt()
    {
        Ok(b) => Ok(Some(b)),
        Err(e) if is_cancelled(&e) => Ok(None),
        Err(e) => Err(e).context("Confirmation failed"),
    }
}
