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

    /// Print the full path of a scanned repository
    Find {
        /// Repository name or path fragment to match
        repo: String,
    },

    /// Show information about a repository
    Stat {
        /// Repository name or path (defaults to current directory)
        repo: Option<String>,
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

fn ye_function(shell: Shell) -> &'static str {
    match shell {
        Shell::Bash | Shell::Zsh => {
            "\nye() {\n  local dir\n  dir=\"$(command yarm find \"$@\")\" && cd \"$dir\"\n}\n"
        }
        Shell::Fish => {
            "\nfunction ye\n  set -l dir (command yarm find $argv)\n  and cd $dir\nend\n"
        }
        Shell::PowerShell => {
            "\nfunction ye { $d = yarm find @args; if ($LASTEXITCODE -eq 0) { Set-Location $d } }\n"
        }
        Shell::Elvish => {
            "\nfn ye {|@args| var dir = (yarm find $@args); cd $dir }\n"
        }
        _ => "",
    }
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
        Command::Find { repo } => {
            commands::find::run(&repo)?;
        }
        Command::Stat { repo } => {
            commands::stat::run(repo)?;
        }
        Command::Scan => {
            commands::scan::run()?;
        }
        Command::Status { full } => {
            commands::status::run(full)?;
        }
        Command::Completions { shell } => {
            generate(shell, &mut Cli::command(), "yarm", &mut io::stdout());
            print!("{}", ye_function(shell));
        }
    }

    Ok(())
}
