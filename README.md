# yarm – Yet Another Repository Manager

yarm eliminates repetitive configuration tasks when cloning, initializing, or updating git repositories.

## Features

- Interactive profile selection based on existing gitconfig files
- Apply `user.name`, `user.email`, `user.signingkey`, and `commit.gpgsign` to repos
- Apply profiles to existing repositories with `yarm apply`
- Manage profiles interactively: create, edit, delete with `yarm profiles`
- Progress display during clone operations
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

yarm discovers profiles from two sources:

1. Files known to git (`git config --list --show-origin`)
2. Additional gitconfig files in `~/.gitconfig-*`, `~/.gitconfig.*`, and `~/.config/git/*.gitconfig`

| Config File | Profile Name |
|-------------|--------------|
| `~/.gitconfig` | default |
| `~/.gitconfig-work` | .gitconfig-work |
| `~/.config/git/oss.gitconfig` | oss |
| `.git/config` | local |

Only files containing `user.name` or `user.email` are shown as selectable profiles.
