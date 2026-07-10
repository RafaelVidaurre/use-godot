# Conservative installation and migration

Migration is intentionally split into installation, current-shell evaluation,
preview, and a separate confirmed write.

## 1. Validate from the worktree

```sh
cargo test --all-targets
cargo build --release
UG_ROOT="$(mktemp -d)" target/release/ug doctor \
  --legacy-link /tmp/nonexistent-godot \
  --legacy-script /tmp/nonexistent-switcher
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
`~/.local/share/use-godot/shims`, while `/usr/local/bin/godot` remains unchanged.

## 4. Preview the real machine migration

```sh
ug migrate plan
```

The plan reads `.zshrc`, the legacy script, and legacy symlink. It writes
nothing. Review the reported alias line and targets.

## 5. Apply only after explicit user approval

```sh
ug migrate apply \
  --zshrc "$HOME/.zshrc" \
  --ug-binary "$HOME/.local/bin/ug" \
  --yes
```

Apply creates the first available `.zshrc.ug-backup[.N]`, then atomically
replaces the `alias ug=...` line with a marked `ug shell init zsh` block. It
preserves `switch_godot_version`, `ug3`, `ug4`, double/JS convenience aliases,
the script itself, `/Applications`, and `/usr/local/bin/godot`.

Open a new shell and run `ug doctor`. Roll back by atomically restoring the
backup if validation fails. Removing the legacy script or system symlink is a
separate future decision; `ug migrate apply` never performs it.

