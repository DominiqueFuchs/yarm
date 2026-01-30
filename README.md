# yarm – Yet Another Repository Manager

[![CI](https://github.com/DominiqueFuchs/yarm/actions/workflows/ci.yml/badge.svg)](https://github.com/DominiqueFuchs/yarm/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/yarm.svg)](https://crates.io/crates/yarm)

A small workflow utility for managing local git repositories. It handles git identity configuration via profiles and keeps track of repositories across directory pools.

- **Profiles** — create, edit, and apply gitconfig-based identity profiles (`user.name`, `user.email`, GPG settings). Profiles are selected interactively or matched automatically via git's `includeIf` rules.
- **Repository tracking** — scan directory pools, look up repositories by name, jump to them via a shell function (`ye`), and inspect repo status at a glance.
- **Shell completions** for bash, zsh, fish, powershell, and elvish.

## Installation

```bash
# From crates.io
cargo install yarm

# From source
cargo install --path .
```

Recommended: install [shell completions](#shell-completions-and-functions) for tab completion and the `ye` jump function.

## Getting Started

```bash
# 1. Clone a repo with interactive profile selection
yarm clone https://github.com/dominiquefuchs/yarm.git

# 2. Scan repository pools (edit ~/.config/yarm.toml — see Configuration below)
yarm scan

# 3. Jump to a repository (uses the `ye` shell function from completions)
ye my-repo

# 4. Check on the current repository
yarm stat
```

## Usage

Help texts are available for yarm and all of its subcommands with usage instructions and available options. To print help use `yarm <command> --help`. The following sections provide an overview of the different commands and their usage principles.

### Git Profile Management

| Command | Description |
|---------|-------------|
| `yarm profiles [name]` | Manage profiles interactively, or target a specific profile |
| `yarm profiles [name] --show` | List all profiles, or print a specific profile's details |

### Repository Setup

Clone, init, and apply all accept `-p <profile>` to skip interactive selection.

| Command | Description |
|---------|-------------|
| `yarm clone <url> [path]` | Clone and apply a profile |
| `yarm init` | Initialize repository and apply a profile |
| `yarm apply [repo]` | Apply a profile to a repository by name (current if omitted) |
| `yarm apply -P <pool>` | Apply a profile to all repositories in a pool |

### Repository Tracking

| Command | Description |
|---------|-------------|
| `yarm scan` | Scan configured pools for git repositories |
| `yarm find <name>` | Print full path of a repository by name |
| `yarm find -P <name>` | Print full path of a pool |
| `yarm stat [repo]` | Show branch, remote, status, size, last fetch |
| `yarm status` | Show pool overview and scan state |

`find` matches by basename first (case-insensitive), then by path suffix. Use path fragments to disambiguate: `yarm find work/my-repo`.

`stat` accepts a repository name, path, or defaults to the current directory.

### Shell Completions and Functions

Add one of the following to your shell configuration. This loads tab completions for `yarm` and the `ye` jump function:

```sh
eval "$(yarm completions zsh)"                  # for zsh

eval "$(yarm completions bash)"                 # for bash

yarm completions fish | source                  # for fish

yarm completions powershell | Invoke-Expression # for PS

eval (yarm completions elvish | slurp)          # for elvish
```

<details>
<summary>Alternative: file-based installation</summary>

If you prefer file-based completions (e.g. for zsh `compinit` caching), redirect the output to the appropriate path:

```bash
# for zsh
yarm completions zsh > ~/.zfunc/_yarm

# for bash
yarm completions bash > ~/.local/share/bash-completion/completions/yarm

# for fish
yarm completions fish > ~/.config/fish/completions/yarm.fish
```

</details>

#### The `ye` Jump Function

The `ye` function uses `yarm find` under the hood to `cd` into a repository by name. Tab completion is included for both repository and pool names.

```bash
ye my-repo       # cd to a repository
ye -P projects   # cd to a pool directory
```

## Profile Discovery

yarm discovers profiles from three sources:

1. Files known to git (`git config --list --show-origin`)
2. Additional gitconfig files in `~/.gitconfig-*`, `~/.gitconfig.*`, and `~/.config/git/*.gitconfig`
3. Custom directories configured in `~/.config/yarm.toml` (see [Configuration](#configuration))

| Config File | Profile Name |
|-------------|--------------|
| `~/.gitconfig` | default |
| `~/.gitconfig-work` | .gitconfig-work |
| `~/.config/git/oss.gitconfig` | oss |
| `.git/config` | local |

Only files containing `user.name` or `user.email` are shown as selectable profiles.

### includeIf Support

yarm respects git's `includeIf` directives. Matching profiles are automatically promoted to the top of the selection list.

```gitconfig
# ~/.gitconfig
[includeIf "gitdir:~/work/"]
    path = ~/.gitconfig-work

[includeIf "hasconfig:remote.*.url:*github.com/mycompany/*"]
    path = ~/.gitconfig-work
```

When cloning or initializing a repo under `~/work/`, or cloning from `github.com/mycompany/*`, the `work` profile will be suggested first.

## Configuration

`~/.config/yarm.toml`

```toml
[profiles]
# Pre-select this profile when no includeIf rule matches
default = "work"

# Additional directories to scan for gitconfig files
paths = [
    "~/custom/gitconfigs",
    "/shared/team-configs"
]

[repositories]
# Directories containing git repositories
pools = [
    "~/projects",
    "~/work"
]

# Glob patterns for directories to skip during scan
# Matched against the path relative to the pool root; use **/ for any depth
exclude = [
    "**/[Bb]uild"
]
```

| Key | Description |
|-----|-------------|
| `profiles.default` | Profile to pre-select when no `-p` flag and no `includeIf` rule applies |
| `profiles.paths` | Additional directories to scan for gitconfig files |
| `repositories.pools` | Directories containing git repositories |
| `repositories.exclude` | Glob patterns for directories to skip during `yarm scan` |
| `repositories.auto_rescan` | Auto-rescan pools when internal state is outdated (default: `true`) |
