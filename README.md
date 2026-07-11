# use-godot (`ug`)

[![CI](https://github.com/RafaelVidaurre/use-godot/actions/workflows/ci.yml/badge.svg)](https://github.com/RafaelVidaurre/use-godot/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/RafaelVidaurre/use-godot)](https://github.com/RafaelVidaurre/use-godot/releases/latest)
[![License](https://img.shields.io/github/license/RafaelVidaurre/use-godot)](LICENSE)

`ug` (short for **use Godot**) installs and selects Godot versions. It keeps
versions and build variants side by side, verifies official downloads, and
supports project-local version selection through `.ugrc`.

Releases currently target macOS on Apple Silicon. Linux and Windows releases
are not yet built or tested.

## Install

With Homebrew:

```sh
brew install RafaelVidaurre/tap/ug
```

Or with the release installer:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/RafaelVidaurre/use-godot/releases/latest/download/use-godot-installer.sh | USE_GODOT_NO_MODIFY_PATH=1 sh
```

The release installer puts `ug` in `$HOME/.cargo/bin`. Add that directory to
`PATH` if it is not already there.

To build from source, install Rust 1.85 or newer, then run:

```sh
git clone https://github.com/RafaelVidaurre/use-godot.git
cd use-godot
./scripts/install.sh
```

The source installer puts `ug` in `$HOME/.local/bin`. Set `UG_BIN_DIR` to choose
a different directory.

## Usage

```sh
# Show available official releases.
ug list --remote

# Install and select the latest stable 4.7 release.
ug install 4.7
ug use 4.7

# Show the active identity and executable.
ug current
ug which

# Run another version without changing the active one.
ug install 4.7@mono
ug exec 4.7@mono -- --editor project.godot
```

`ug install` shows download progress in an interactive terminal. Pass `--quiet`
to suppress routine output.

### Selectors

| Selector | Meaning |
| --- | --- |
| `latest` | Latest stable release |
| `4` | Latest stable 4.x release |
| `4.7` | Latest stable 4.7.x release |
| `4.7.1` | Godot 4.7.1 |
| `4.8-beta` | Latest 4.8 beta |
| `4.8-beta2` | Godot 4.8 beta 2 |
| `4.7@mono` | Latest stable 4.7.x .NET build |

Prerelease channels are `rc`, `beta`, `alpha`, and `dev`. Without a channel,
selectors resolve to stable releases.

The available variants are:

| Variant | Installation source |
| --- | --- |
| `standard` | Official download |
| `mono` | Official .NET download |
| `double` | Local import |
| `godotjs` | Local import |
| `custom:NAME` | Named local import |

The default variant is `standard`. A variant is part of an installed build's
identity, so `4.7@standard` and `4.7@double` can both be installed.

### Project versions

`ug pin` writes a selector to `.ugrc`:

```sh
ug pin 4.7@mono
```

Within that directory and its children, the following commands read the nearest
`.ugrc` when no selector is given:

```sh
ug install
ug use
ug which
ug exec -- --editor project.godot
```

An explicit selector takes precedence over `.ugrc`.

### Defaults and aliases

`use` changes the active build. `default` records a fallback and also activates
it:

```sh
ug use 4.7
ug default 4.7@mono
ug default --unset
```

Named aliases are selectors managed by `ug`, not shell aliases:

```sh
ug alias set studio 4.7@mono
ug use studio
ug alias list
ug alias remove studio
```

### Local builds

Import double-precision, GodotJS, and custom builds with `--from`:

```sh
ug install 4.7@double --from "/path/to/Godot Double.app"
ug install 4.7@godotjs --from "/path/to/GodotJS.app"
ug install 4.7@custom:studio --from "/path/to/Godot Studio.app"
```

The source is copied into managed storage. A single-file local import using the
`standard` or `mono` identity also requires `--checksum SHA256`.

## Commands

| Command | Description |
| --- | --- |
| `ug install [SELECTOR]` | Install an official release or import a local build |
| `ug list` | List installed builds |
| `ug list --remote` | List official releases |
| `ug use [SELECTOR]` | Select an installed build |
| `ug default [SELECTOR]` | Get, set, or clear the default |
| `ug alias …` | Manage named selectors |
| `ug current` | Print the active identity |
| `ug which [SELECTOR]` | Print an installed executable path |
| `ug exec [SELECTOR] -- …` | Run Godot without changing the active build |
| `ug pin SELECTOR` | Write `.ugrc` |
| `ug uninstall SELECTOR` | Remove an installed build |
| `ug doctor` | Check managed state |
| `ug shell …` | Show the shim path or generate shell setup and completions |

Run `ug help COMMAND` for all options and examples. `--json` produces structured
output where supported; `--quiet` suppresses routine output and progress.

Errors return status 1. `doctor` returns 2 when managed state is unhealthy.
`exec` returns the child process's status.

## Shell integration

`ug` works without shell integration. Use `ug exec` to run a selected build
directly:

```sh
ug exec 4.7 -- --editor project.godot
```

Shell integration is only needed for two conveniences: running the build
selected by `ug use` as `godot`, and enabling tab completion.

To expose the managed `godot` shim in bash, zsh, or another POSIX-style shell:

```sh
export PATH="$(ug shell path):$PATH"
```

For fish:

```fish
fish_add_path --prepend (ug shell path)
```

Completions can be loaded separately with `ug shell completions SHELL`. For a
combined current-session setup:

```sh
# zsh
eval "$(ug shell init zsh)"

# bash
eval "$(ug shell init bash)"

# fish
ug shell init fish | source
```

These commands print shell code; they do not edit startup files. See
[Shell integration](docs/shell-integration.md) for completion commands and
persistent setup.

## Storage and integrity

The default data directory is `~/.local/share/use-godot`. Use `UG_ROOT` or
`--root` for an isolated location:

```sh
UG_ROOT=/path/to/root ug list
ug --root /path/to/root doctor
```

Official downloads must match a published SHA-256 digest or an entry in the
release's `SHA512-SUMS.txt`. Installations are staged before being moved into
managed storage. Archive paths and symlinks are checked for escapes, mutations
are serialized, and interrupted activation or uninstall operations are
recovered from a journal.

See [Architecture](docs/architecture.md) for the state layout and release
metadata sources.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked
shellcheck scripts/*.sh
```

Tests use temporary managed roots and local fixtures. See [Testing](docs/testing.md)
and [Distribution](docs/distribution.md) for the test and release procedures.

Report bugs through [GitHub Issues](https://github.com/RafaelVidaurre/use-godot/issues).
Open an issue before starting a large change. Pull requests that change behavior
should include tests. See [Contributing](CONTRIBUTING.md) for branch, commit,
validation, and review requirements.

## License

[MIT](LICENSE)
