# yarm – Yet Another Repository Manager

yarm eliminates repetitive configuration tasks when cloning, initializing, or updating git repositories.

## Features

- Easily create, list and edit profile-based gitconfig files with common identity- and environment-based settings
- Apply profiles manually, based on folder location / remote URL or interactively choose a suitable profile on repo initialization or clone time
- Shell completions for bash, zsh, fish, powershell, and elvish

## Installation

```bash
# From crates.io
cargo install yarm

# From source
cargo install --path .
```

## Usage

### Profiles

```bash
# Manage profiles interactively (create, edit, delete)
yarm profiles

# List profiles without interactive menu
yarm profiles --show
```

### Clone

```bash
# Clone with interactive profile selection
yarm clone https://github.com/someone/repo.git

# Clone to specific directory
yarm clone https://github.com/someone/repo.git ~/projects/repo

# Clone with specific profile (non-interactive)
yarm clone https://github.com/someone/repo.git -p work
```

### Init

```bash
# Initialize current directory with profile selection
yarm init

# Initialize specific directory
yarm init ~/projects/new-repo

# Initialize with specific profile
yarm init -p work
```

### Apply

```bash
# Apply profile to current repository
yarm apply

# Apply profile to specific repository
yarm apply ~/projects/existing-repo

# Apply specific profile (non-interactive)
yarm apply -p work
```

### Scan

```bash
# Scan configured repository pools for git repositories
yarm scan
```

Recursively walks each directory listed in `repositories.pools` and updates corresponding tracking data.

### Status

```bash
# Show repository pool status
yarm status
```

Displays configured pool directories and the number of scanned repositories in each.

## Shell Completions

```bash
# Zsh
yarm completions zsh > ~/.zfunc/_yarm
# Add to .zshrc: fpath+=~/.zfunc && autoload -Uz compinit && compinit

# Bash
yarm completions bash > /etc/bash_completion.d/yarm
# Or: eval "$(yarm completions bash)"

# Fish
yarm completions fish > ~/.config/fish/completions/yarm.fish
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

## includeIf Support

yarm respects git's `includeIf` directives. If you have conditional includes in your `~/.gitconfig`, matching profiles are automatically promoted to the top of the selection list.

```gitconfig
# ~/.gitconfig
[includeIf "gitdir:~/work/"]
    path = ~/.gitconfig-work

[includeIf "hasconfig:remote.*.url:*github.com/mycompany/*"]
    path = ~/.gitconfig-work
```

When cloning or initializing a repo under `~/work/`, or cloning from `github.com/mycompany/*`, the `work` profile will be suggested first.

## Configuration

yarm can be configured via `~/.config/yarm.toml`.

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
# Directories where repositories are expected to reside
pools = [
    "~/projects",
    "~/work"
]
```

| Key | Description |
|-----|-------------|
| `profiles.default` | Profile to pre-select when no `-p` flag and no `includeIf` rule applies |
| `profiles.paths` | Additional directories to scan for gitconfig files |
| `repositories.pools` | Directories where repositories are expected to reside |
