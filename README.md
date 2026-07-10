# use-godot (`ug`)

[![CI](https://github.com/RafaelVidaurre/use-godot/actions/workflows/ci.yml/badge.svg)](https://github.com/RafaelVidaurre/use-godot/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/RafaelVidaurre/use-godot)](https://github.com/RafaelVidaurre/use-godot/releases/latest)
[![License](https://img.shields.io/github/license/RafaelVidaurre/use-godot)](LICENSE)

`ug` means **use Godot**. It is a safe, scriptable Godot version manager for
installing, selecting, and running multiple Godot builds side by side.

It provides an NVM-like workflow while keeping Godot versions and build
variants explicit. `ug` does not replace system executables, modify installed
applications, or edit shell startup files.

> [!NOTE]
> macOS on Apple Silicon is the production-supported target today. The core is
> designed for other platforms, but they are not yet covered by release CI.

## Features

- Resolve versions with selectors such as `latest`, `4`, `4.7`, or
  `4.8-beta2`.
- Install official standard and .NET builds with checksum verification.
- Keep standard, .NET, double-precision, GodotJS, and custom builds as distinct
  identities.
- Pin projects with a `.ugrc` file, similar to `.nvmrc`.
- Create CLI-managed aliases and choose a global default.
- Run a version once without changing the active selection.
- Inspect installed and remote versions with human-readable or JSON output.
- Recover safely from interrupted activation and uninstall operations.
- Generate optional integration and completions for multiple shells.

## Installation

### Homebrew

```sh
brew install RafaelVidaurre/tap/ug
ug --version
```

No shell initialization is required to use `ug`.

### Release installer

If Homebrew is unavailable, install the latest release without modifying shell
startup files:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/RafaelVidaurre/use-godot/releases/latest/download/use-godot-installer.sh | USE_GODOT_NO_MODIFY_PATH=1 sh
"$HOME/.cargo/bin/ug" --version
```

### From source

Rust 1.85 or newer is required.

```sh
git clone https://github.com/RafaelVidaurre/use-godot.git
cd use-godot
./scripts/install.sh
"$HOME/.local/bin/ug" --version
```

## Quick start

```sh
# Browse official releases.
ug list --remote

# Install the latest stable 4.7 standard build.
ug install 4.7

# Select it for the managed Godot shim.
ug use 4.7

# Inspect the selection and executable path.
ug current
ug which

# Run Godot once without changing the selection.
ug exec 4.7 -- --editor project.godot
```

Interactive installs report resolution, download progress, speed, ETA,
verification, extraction, and completion. Use `--quiet` for automation.

## Version selectors

Commands accept partial semantic versions, release channels, build variants,
or named aliases:

| Selector | Resolves to |
| --- | --- |
| `latest` | Latest stable release |
| `4` | Latest stable 4.x release |
| `4.7` | Latest stable 4.7.x release |
| `4.7.1` | Exact stable release |
| `4.8-beta` | Latest beta in the 4.8 series |
| `4.8-beta2` | Exact beta release |
| `4.7@mono` | Latest stable 4.7.x .NET build |

Stable is the default channel. Supported prerelease channels are `rc`, `beta`,
`alpha`, and `dev`, optionally followed by a release number.

The default variant is `standard`. Append one of these variants with `@`:

| Variant | Source |
| --- | --- |
| `standard` | Verified official download |
| `mono` | Verified official .NET download |
| `double` | Trusted local import |
| `godotjs` | Trusted local import |
| `custom:NAME` | Trusted named local import |

For example, `4.7@standard` and `4.7@double` are independently installable and
selectable identities.

## Project versions

`.ugrc` is the `.nvmrc` equivalent. Pin a selector in the current project:

```sh
ug pin 4.7@mono
```

This creates a `.ugrc` containing:

```text
4.7@mono
```

From that directory or any child directory, selector-less commands use the
nearest `.ugrc`:

```sh
ug install
ug use
ug which
ug exec -- --editor project.godot
```

An explicit selector always takes precedence over `.ugrc`.

## Defaults and aliases

`use` changes the active version. `default` records a fallback and activates it
immediately:

```sh
ug use 4.7@standard
ug default 4.7@standard
ug default --unset
```

Named aliases are managed by `ug`; they are not shell aliases:

```sh
ug alias set studio 4.7@standard
ug alias list
ug use studio
ug alias remove studio
```

## Importing local builds

Double-precision, GodotJS, and custom builds are imported from a local
executable or application bundle:

```sh
ug install 4.7@double --from "/path/to/Godot Double.app"
ug install 4.7@godotjs --from "/path/to/GodotJS.app"
ug install 4.7@custom:studio --from "/path/to/Godot Studio.app"
```

The source is copied into managed storage and committed atomically. Importing a
local build as `standard` or `mono` also requires `--checksum SHA256`, so an
arbitrary binary cannot be recorded as an official-family build without an
integrity assertion.

## Command reference

| Command | Purpose |
| --- | --- |
| `ug install [SELECTOR]` | Install an official release or import a local build |
| `ug list` | List installed builds |
| `ug list --remote` | List matching official releases |
| `ug use [SELECTOR]` | Select an installed build |
| `ug default [SELECTOR]` | Get, set, or clear the default selection |
| `ug alias …` | Set, remove, list, or resolve named selectors |
| `ug current` | Print the active identity |
| `ug which [SELECTOR]` | Print an installed executable path |
| `ug exec [SELECTOR] -- …` | Run Godot once without switching |
| `ug pin SELECTOR` | Write `.ugrc` in the current directory |
| `ug uninstall SELECTOR` | Remove an installed build |
| `ug doctor` | Diagnose managed state and interrupted operations |
| `ug shell …` | Generate optional shell integration or completions |

Run `ug help COMMAND` for complete arguments and examples.

### Automation

Commands that expose records support global `--json`. Global `--quiet`
suppresses routine output and interactive progress.

```sh
ug --json list
ug --quiet install 4.7
```

Errors return exit code 1. `doctor` returns 2 for unhealthy managed state, and
`exec` passes through the child process's exit code.

## Shell integration

Shell integration is optional. It adds the managed `godot` shim and completions
to the current session; it is not needed to invoke `ug`.

```sh
# zsh
eval "$(ug shell init zsh)"

# bash
eval "$(ug shell init bash)"

# fish
ug shell init fish | source
```

`ug` only emits shell code. It never edits startup files or assumes a preferred
shell. See [Shell integration](docs/shell-integration.md) for standalone
completion generation and safety details.

## Managed data

Managed state is stored in `~/.local/share/use-godot` by default. Override the
root for CI, testing, or isolated environments:

```sh
UG_ROOT=/path/to/root ug list
ug --root /path/to/root doctor
```

## Safety and integrity

- Official artifacts must match an authoritative SHA-256 digest or the
  release's `SHA512-SUMS.txt`; missing integrity data fails closed.
- Downloads and extraction happen in hidden staging locations before one
  atomic installation commit.
- Archive paths and symlinks cannot escape managed storage.
- State writes, manifests, aliases, and shim changes use atomic replacement.
- A process lock serializes mutations, while a durable journal recovers
  interrupted activation and uninstall transitions.
- Uninstall protects active and default builds unless explicitly forced.
- Automated tests use temporary roots and mock servers; they do not touch
  applications, startup files, or system executable paths.

Read [Architecture](docs/architecture.md) for the complete design and upstream
release-metadata decisions.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked
shellcheck scripts/*.sh
```

The integration suite exercises real CLI flows against isolated temporary
roots, including integrity failures, malicious archives, variant identity,
aliases, project selectors, and interrupted-operation recovery.

See [Testing](docs/testing.md) and [Distribution](docs/distribution.md) for the
full validation and release processes.

## Contributing

Bug reports, focused feature proposals, and pull requests are welcome. Before
submitting a change:

1. Keep filesystem and platform dependencies injectable.
2. Add or update tests for behavior changes.
3. Run the development checks above.
4. Document user-visible behavior and non-obvious design decisions.

Please use [GitHub Issues](https://github.com/RafaelVidaurre/use-godot/issues) to
report bugs or discuss substantial changes before implementation.

## License

`use-godot` is available under the [MIT License](LICENSE).
