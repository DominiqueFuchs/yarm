use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use std::io;
use std::path::PathBuf;

mod commands;
mod config;
mod git;
mod profile;
mod state;
mod term;

/// Yet Another Repository Manager
#[derive(Parser)]
#[command(name = "yarm")]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Clone a repository with profile selection
    Clone {
        /// Repository URL to clone
        url: String,
        /// Target directory (defaults to repo name from URL)
        path: Option<PathBuf>,
        /// Use named profile instead of interactive selection
        #[arg(short, long)]
        profile: Option<String>,
    },

    /// Initialize a new repository with profile selection
    Init {
        /// Directory to initialize (defaults to current directory)
        path: Option<PathBuf>,
        /// Use named profile instead of interactive selection
        #[arg(short, long)]
        profile: Option<String>,
    },

    /// Apply a profile to an existing repository
    Apply {
        /// Repository path (defaults to current directory)
        path: Option<PathBuf>,
        /// Use named profile instead of interactive selection
        #[arg(short, long)]
        profile: Option<String>,
    },

    /// Manage git identity profiles
    Profiles {
        /// List profiles without interactive menu
        #[arg(short, long)]
        show: bool,
    },

    /// Scan repository pools for git repositories
    Scan,

    /// Show repository pool status
    Status {
        /// List all repositories in each pool
        #[arg(short, long)]
        full: bool,
    },

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Clone { url, path, profile } => {
            commands::clone::run(&url, path, profile.as_deref())?;
        }
        Command::Init { path, profile } => {
            commands::init::run(path, profile.as_deref())?;
        }
        Command::Apply { path, profile } => {
            commands::apply::run(path, profile.as_deref())?;
        }
        Command::Profiles { show } => {
            commands::profiles::run(show)?;
        }
        Command::Scan => {
            commands::scan::run()?;
        }
        Command::Status { full } => {
            commands::status::run(full)?;
        }
        Command::Completions { shell } => {
            generate(shell, &mut Cli::command(), "yarm", &mut io::stdout());
        }
    }

    Ok(())
}
