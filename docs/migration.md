# Conservative installation and migration

Migration is intentionally split into installation, current-shell evaluation,
preview, and a separate confirmed write.

## 1. Validate from the worktree

```sh
cargo test --all-targets
cargo build --release
UG_ROOT="$(mktemp -d)" target/release/ug doctor
```

## 2. Install only the binary

```sh
./scripts/install.sh
"$HOME/.local/bin/ug" --version
```

`scripts/install.sh` atomically installs the executable under
`${UG_BIN_DIR:-$HOME/.local/bin}`. It does not edit `.zshrc` or touch Godot.

## 3. Try one shell without persistence

```sh
eval "$("$HOME/.local/bin/ug" shell init zsh)"
ug list --remote
ug install 4.7@standard
ug use 4.7@standard
command -v godot
ug current
ug doctor
```

`command -v godot` should resolve inside
`~/.local/share/use-godot/shims`. No external Godot path is modified.

## 4. Preview persistent shell integration

```sh
ug migrate plan --zshrc "$HOME/.zshrc"
```

The plan reads the selected shell file and writes nothing. If migrating from a
custom switcher, its paths can be inspected explicitly without making them
product defaults:

```sh
ug migrate plan \
  --zshrc "$HOME/.zshrc" \
  --legacy-script "/path/to/previous-switcher" \
  --legacy-link "/path/to/previous-godot-link"
```

## 5. Apply only after explicit user approval

```sh
ug migrate apply \
  --zshrc "$HOME/.zshrc" \
  --ug-binary "$HOME/.local/bin/ug" \
  --yes
```

Apply creates the first available `.zshrc.ug-backup[.N]`, then atomically
replaces the `alias ug=...` line with a marked `ug shell init zsh` block. It
leaves every other shell line and all external files unchanged.

Open a new shell and run `ug doctor`. Roll back by atomically restoring the
backup if validation fails. Removing any previous switcher or external symlink
is a separate decision; `ug migrate apply` never performs it.
