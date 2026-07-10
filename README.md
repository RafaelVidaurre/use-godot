# use-godot (`ug`)

`ug` is a safe, scriptable Godot version manager. It installs official builds,
keeps build variants distinct, resolves semantic version selectors, and exposes
the selected editor through an owned `godot` shim. It is designed to feel like
NVM without taking ownership of system paths or existing Godot applications.

The current release targets macOS Apple Silicon first. Official macOS archives
are Universal 2, so their stored identity uses `macos-universal`; Linux and
Windows asset naming is also modeled for later platform validation.

## Status

The CLI is implemented and tested, but it has **not** replaced this machine's
legacy `ug` alias, `~/scripts/switch_godot_version.sh`, applications, or
`/usr/local/bin/godot` symlink. Migration is preview-first and requires an
explicit `--yes`.

## Build and test

Rust 1.85 or newer is required.

```sh
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

All integration tests use temporary `--root` directories and mock servers.
They do not read or mutate `/Applications`, shell startup files, the legacy
script, or `/usr/local/bin/godot`.

## Safe installation

The installer copies only the compiled `ug` binary. It does not edit shell
files or system paths:

```sh
./scripts/install.sh
eval "$("$HOME/.local/bin/ug" shell init zsh)"
```

The second command changes only the current shell. Confirm the binary and shim
locations before considering persistent migration:

```sh
ug --version
ug doctor
ug migrate plan
```

By default, managed data lives in `~/.local/share/use-godot`. Override it with
`--root DIR` or `UG_ROOT`; this is also how tests isolate every filesystem
operation.

## Quick start

```sh
# Discover releases and install official builds with verified integrity.
ug list --remote
ug install 4.7
ug install 4.7@mono

# Keep variants independent and select explicitly when necessary.
ug list
ug use 4.7@standard
ug default 4.7@standard
ug current
ug which

# Named aliases are managed state, not shell aliases.
ug alias set studio 4.7@standard
ug alias list
ug use studio

# One shot: does not change the active/default selection.
ug exec 4.7@mono -- --editor /path/to/project.godot
```

Version selectors accept a major (`4`), branch (`4.7`), exact semantic version
(`4.7.1`), exact or grouped channels (`4.7.1-rc1`, `4.8-dev`), and an optional
variant (`@standard`, `@mono`, `@double`, `@godotjs`, or `@custom:name`). Stable
is the default channel. When two variants have the same top version, an
unqualified selector fails as ambiguous instead of silently choosing one.

## Custom, double-precision, and GodotJS builds

These are first-class identities, but they are not presented as artifacts from
the official standard/.NET editor feed. Import a trusted local build:

```sh
ug install 4.7@double --from "/path/to/Godot Double.app"
ug install 4.7@godotjs --from "/path/to/GodotJS.app"
ug install 4.7@custom:studio --from "/path/to/Godot Studio.app"
```

The source is copied atomically into the managed root and its original path is
recorded. Local imports declared `standard` or `mono` additionally require
`--checksum SHA256`, preventing an arbitrary local binary from being recorded
as an official-family build without an integrity assertion.

## Command summary

```text
ug install SELECTOR [--variant VARIANT] [--from PATH] [--checksum SHA256]
ug list [--remote] [--prerelease] [--refresh]
ug use SELECTOR
ug default [SELECTOR | --unset]
ug alias set|remove|list|resolve ...
ug current
ug which [SELECTOR]
ug exec SELECTOR -- GODOT_ARGS...
ug uninstall SELECTOR [--force]
ug doctor
ug shell init zsh
ug shell completions zsh
ug migrate plan
ug migrate apply --zshrc PATH --ug-binary ABSOLUTE_PATH --yes
```

Commands that expose records support global `--json`; global `--quiet` removes
routine success output. Errors use exit code 1, `doctor` uses 2 for unhealthy
managed state, and `exec` passes through the child exit code.

## Safety model

- Official downloads are accepted only when the release API supplies an asset
  SHA-256 digest or the release's `SHA512-SUMS.txt` contains that asset. Size and
  digest must both match.
- ZIP paths and symlinks are constrained to the staging directory.
- Installs extract into hidden staging directories and become visible with one
  directory rename only after validation and manifest persistence.
- State, manifests, backups, and shim changes use same-directory atomic
  replacement. A lock serializes mutating commands.
- Uninstall refuses active/default builds without `--force`, stages removal by
  rename, and clears aliases that point to the removed identity.
- `doctor` identifies incomplete staging/trash directories for recovery without
  deleting evidence automatically.
- Migration backs up `.zshrc`, replaces only the `ug` alias, and deliberately
  preserves the legacy script, convenience aliases, applications, and symlink.

See [architecture](docs/architecture.md), [migration](docs/migration.md), and
[testing](docs/testing.md) for design rationale and operational details.

