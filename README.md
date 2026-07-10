# use-godot (`ug`)

`ug` means **use Godot**. It is a safe, scriptable Godot version manager that
installs official builds, keeps build variants distinct, resolves semantic
version selectors, and exposes the selected editor through an owned `godot`
shim. It is designed to feel like NVM without taking ownership of system paths
or existing Godot applications.

The current release targets macOS Apple Silicon first. Official macOS archives
are Universal 2, so their stored identity uses `macos-universal`; Linux and
Windows asset naming is also modeled for later platform validation.

## Build and test

Rust 1.85 or newer is required.

```sh
cargo test --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

All integration tests use temporary `--root` directories and mock servers. They
do not read or mutate applications, shell startup files, or system executable
paths.

## Install

```sh
brew install RafaelVidaurre/tap/ug
```

Homebrew installs `ug` directly on `PATH`; no shell setup is required. If
Homebrew is not available, the release installer can install to Cargo's binary
directory without editing any shell startup file:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/RafaelVidaurre/use-godot/releases/latest/download/use-godot-installer.sh | USE_GODOT_NO_MODIFY_PATH=1 sh
"$HOME/.cargo/bin/ug" --version
```

To build the current checkout instead:

```sh
./scripts/install.sh
"$HOME/.local/bin/ug" --version
```

By default, managed data lives in `~/.local/share/use-godot`. Override it with
`--root DIR` or `UG_ROOT`; this is also how tests isolate every filesystem
operation.

Optional shell integration exposes the managed `godot` shim and completions.
It is not required to run `ug`; see [shell integration](docs/shell-integration.md).

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

`ug use` changes the active build without changing the default. `ug default`
sets the default and activates it immediately, ensuring the managed shim is
usable.

## Install selectors

`ug install SELECTOR` downloads the newest official release matching the
selector:

| Selector | Meaning |
| --- | --- |
| `latest` | Latest stable release |
| `4` | Latest stable 4.x release |
| `4.7` | Latest stable 4.7.x release |
| `4.7.1` | Exact stable version |
| `4.8-beta` | Latest beta in the 4.8 series |
| `4.8-beta2` | Exact beta release |
| `4.7@mono` | Latest stable 4.7.x .NET build |

Stable is the default channel. Other supported channels are `rc`, `beta`,
`alpha`, and `dev`, optionally followed by their release number. Build variants
are appended with `@`: `standard`, `mono`, `double`, `godotjs`, or
`custom:NAME`. `standard` is the default variant. Official downloads are
available for `standard` and `mono`; other variants are imported with `--from`.

Interactive downloads show resolution status, transferred bytes, speed, ETA,
verification, extraction, and commit phases. Progress is omitted for `--quiet`,
`--json`, and redirected output.

Run `ug install --help` for all arguments and examples.

## Per-project versions

`.ugrc` is the `.nvmrc` equivalent. It contains one selector:

```text
4.7@mono
```

Create it and use it from that directory or any child directory:

```sh
ug pin 4.7@mono
ug install
ug use
ug which
ug exec -- --editor project.godot
```

An explicit selector always overrides `.ugrc`. Without an explicit selector,
`install`, `use`, `which`, and `exec` search the current directory and its
parents for the nearest `.ugrc`.

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
ug install [SELECTOR] [--variant VARIANT] [--from PATH] [--checksum SHA256]
ug list [--remote] [--prerelease] [--refresh]
ug use [SELECTOR]
ug default [SELECTOR | --unset]
ug alias set|remove|list|resolve ...
ug current
ug which [SELECTOR]
ug exec [SELECTOR] -- GODOT_ARGS...
ug pin SELECTOR
ug uninstall SELECTOR [--force]
ug doctor
ug shell init zsh
ug shell init bash
ug shell init fish
ug shell completions zsh
ug shell completions bash
ug shell completions fish
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
- State, manifests, and shim changes use same-directory atomic replacement. A
  lock serializes mutating commands, and a durable operation journal recovers
  interrupted activation and uninstall transitions.
- Uninstall refuses active/default builds without `--force`, stages removal by
  rename, and clears aliases that point to the removed identity.
- `doctor` identifies incomplete staging/trash directories and pending
  operations without deleting evidence automatically.
- Shell integration is emitted to standard output for explicit evaluation; it
  never edits startup files.

See [architecture](docs/architecture.md), [shell integration](docs/shell-integration.md),
[testing](docs/testing.md), and [distribution](docs/distribution.md) for design
rationale and operational details.
