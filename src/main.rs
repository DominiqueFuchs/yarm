use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};
use std::io;
use std::path::PathBuf;
use std::process;

use console::style;
use term::SilentExit;

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
        /// Use named profile instead of interactive selection
        #[arg(short, long)]
        profile: Option<String>,
    },

    /// Apply a profile to an existing repository
    Apply {
        /// Repository name or path (defaults to current directory)
        name: Option<String>,
        /// Use named profile instead of interactive selection
        #[arg(short, long)]
        profile: Option<String>,
        /// Apply to all repositories in a pool
        #[arg(short = 'P', long)]
        pool: Option<String>,
    },

    /// Manage git identity profiles
    Profiles {
        /// Profile name to show, edit, or delete
        name: Option<String>,
        /// Print profile details without interactive menu
        #[arg(short, long)]
        show: bool,
    },

    /// Print the full path of a scanned repository or pool
    Find {
        /// Repository name or path fragment to match
        repo: Option<String>,
        /// Find a repository pool by name instead of a repository
        #[arg(short = 'P', long)]
        pool: Option<String>,
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

    /// Output repository names for shell completion
    #[command(hide = true)]
    CompleteRepoNames,

    /// Output pool basenames for shell completion
    #[command(hide = true)]
    CompletePoolNames,
}

fn shell_functions(shell: Shell) -> String {
    match shell {
        Shell::Bash => {
            "\n\
ye() {\n\
  local dir\n\
  echo >&2\n\
  if dir=\"$(command yarm find \"$@\")\" && cd \"$dir\"; then\n\
    printf '  \\033[1;32m✓\\033[0m navigated to %s\\n' \"${dir/#$HOME/~}\" >&2\n\
  fi\n\
  echo >&2\n\
}\n\
\n\
_ye_complete() {\n\
  local cur=\"${COMP_WORDS[COMP_CWORD]}\"\n\
  local prev=\"${COMP_WORDS[COMP_CWORD-1]}\"\n\
  if [[ \"$prev\" == \"--pool\" || \"$prev\" == \"-P\" ]]; then\n\
    COMPREPLY=($(compgen -W \"$(command yarm complete-pool-names 2>/dev/null)\" -- \"$cur\"))\n\
  elif [[ \"$cur\" != -* ]]; then\n\
    COMPREPLY=($(compgen -W \"$(command yarm complete-repo-names 2>/dev/null)\" -- \"$cur\"))\n\
  fi\n\
}\n\
complete -F _ye_complete ye\n"
                .to_string()
        }
        Shell::Zsh => {
            "\n\
ye() {\n\
  local dir\n\
  echo >&2\n\
  if dir=\"$(command yarm find \"$@\")\" && cd \"$dir\"; then\n\
    printf '  \\033[1;32m✓\\033[0m navigated to %s\\n' \"${dir/#$HOME/~}\" >&2\n\
  fi\n\
  echo >&2\n\
}\n\
\n\
_ye() {\n\
  local -a repos pools\n\
  if [[ \"${words[CURRENT-1]}\" == \"-P\" || \"${words[CURRENT-1]}\" == \"--pool\" ]]; then\n\
    pools=(${(f)\"$(command yarm complete-pool-names 2>/dev/null)\"})\n\
    compadd -a pools\n\
  else\n\
    repos=(${(f)\"$(command yarm complete-repo-names 2>/dev/null)\"})\n\
    compadd -a repos\n\
  fi\n\
}\n\
compdef _ye ye\n"
                .to_string()
        }
        Shell::Fish => {
            "\n\
function ye\n\
  echo >&2\n\
  set -l dir (command yarm find $argv)\n\
  and cd $dir\n\
  and printf '  \\033[1;32m✓\\033[0m navigated to %s\\n' (string replace -- $HOME '~' $dir) >&2\n\
  echo >&2\n\
end\n\
\n\
complete -c ye -f\n\
complete -c ye -s P -l pool -xa '(command yarm complete-pool-names 2>/dev/null)'\n\
complete -c ye -n 'not __fish_seen_option -P pool' -xa '(command yarm complete-repo-names 2>/dev/null)'\n"
                .to_string()
        }
        Shell::PowerShell => {
            "\nfunction ye { Write-Host; $d = yarm find @args; if ($LASTEXITCODE -eq 0) { Set-Location $d; Write-Host \"  ✓ navigated to $($d -replace [regex]::Escape($HOME), '~')\" -ForegroundColor Green }; Write-Host }\n"
                .to_string()
        }
        Shell::Elvish => {
            "\nfn ye {|@args| echo >&2; var dir = (yarm find $@args); cd $dir; echo '  ✓ navigated to '(str:replace $E:HOME '~' $dir) >&2; echo >&2 }\n".to_string()
        }
        _ => String::new(),
    }
}

fn main() {
    if let Err(e) = run() {
        if let Some(exit) = e.downcast_ref::<SilentExit>() {
            process::exit(exit.0);
        }
        eprintln!("Error: {e:#}");
        process::exit(1);
    }
}

fn should_auto_rescan(command: &Command) -> bool {
    !matches!(
        command,
        Command::Scan
            | Command::Completions { .. }
            | Command::CompleteRepoNames
            | Command::CompletePoolNames
    )
}

fn try_auto_rescan() -> Result<()> {
    if state::version_matches() {
        return Ok(());
    }

    let config = config::load()?;
    if !config.repositories.auto_rescan || config.repositories.pools.is_empty() {
        return Ok(());
    }

    eprintln!(
        "  {} {}",
        style("↻").cyan(),
        style("State outdated, rescanning...").dim()
    );
    commands::scan::run()
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    if should_auto_rescan(&cli.command) {
        try_auto_rescan()?;
    }

    match cli.command {
        Command::Clone { url, path, profile } => {
            commands::clone::run(&url, path, profile.as_deref())?;
            println!();
        }
        Command::Init { profile } => {
            commands::init::run(profile.as_deref())?;
            println!();
        }
        Command::Apply {
            name,
            profile,
            pool,
        } => {
            commands::apply::run(name.as_deref(), profile.as_deref(), pool.as_deref())?;
            println!();
        }
        Command::Profiles { name, show } => {
            commands::profiles::run(name.as_deref(), show)?;
            println!();
        }
        Command::Find { repo, pool } => {
            commands::find::run(repo.as_deref(), pool.as_deref())?;
        }
        Command::Stat { repo } => {
            commands::stat::run(repo)?;
            println!();
        }
        Command::Scan => {
            commands::scan::run()?;
            println!();
        }
        Command::Status { full } => {
            commands::status::run(full)?;
            println!();
        }
        Command::Completions { shell } => {
            generate(shell, &mut Cli::command(), "yarm", &mut io::stdout());
            print!("{}", shell_functions(shell));
        }
        Command::CompleteRepoNames => {
            commands::find::complete_repo_names()?;
        }
        Command::CompletePoolNames => {
            commands::find::complete_pool_names()?;
        }
    }

    Ok(())
}
