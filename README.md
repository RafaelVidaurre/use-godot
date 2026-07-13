# ug

[![CI](https://github.com/RafaelVidaurre/use-godot/actions/workflows/ci.yml/badge.svg)](https://github.com/RafaelVidaurre/use-godot/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/RafaelVidaurre/use-godot)](https://github.com/RafaelVidaurre/use-godot/releases/latest)
[![License](https://img.shields.io/github/license/RafaelVidaurre/use-godot)](LICENSE)

Install and switch Godot versions without the download-page ritual.

`ug` (short for **use Godot**) pulls official builds, keeps standard and .NET
side by side, checks checksums, and lets each project pin the version it wants
with a `.ugrc` file. If you keep more than one Godot around — or one project
needs mono and another doesn’t — this is the boring tool for that.

Prebuilt releases cover the common desktop targets:

| Platform | Architectures | Standalone installer |
| --- | --- | --- |
| macOS | Apple Silicon, Intel | POSIX shell |
| Linux (glibc) | arm64, x86_64 | POSIX shell |
| Windows | x86_64 | PowerShell |

The installers reject unsupported architectures and Linux libc combinations
instead of guessing. Every native archive has a SHA-256 file beside it.

## Install

Homebrew is the easy path on supported macOS and Linux systems:

```sh
brew install RafaelVidaurre/tap/ug
```

Other options if you prefer them:

<details>
<summary>Release installer</summary>

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/RafaelVidaurre/use-godot/releases/latest/download/use-godot-installer.sh \
  | sh
```

This verifies the native archive and atomically installs `ug` in
`~/.local/bin`. It does not edit shell startup files. Set `UG_BIN_DIR` to choose
another directory.

On Windows PowerShell:

```powershell
irm https://github.com/RafaelVidaurre/use-godot/releases/latest/download/use-godot-installer.ps1 | iex
```

This installs to `%LOCALAPPDATA%\Programs\ug\bin` without changing the user
`PATH` registry value. Set `$env:UG_BIN_DIR` to choose another directory.

Neither standalone installer requires Rust.

</details>

<details>
<summary>Build from source</summary>

Needs Rust 1.85 or newer:

```sh
git clone https://github.com/RafaelVidaurre/use-godot.git
cd use-godot
./scripts/install.sh
```

The binary lands in `~/.local/bin` by default. Override with `UG_BIN_DIR`.

</details>

## 60-second start

```sh
ug list --remote          # what's available upstream
ug install 4.7            # latest stable 4.7.x
ug use 4.7                # make it current

ug current                # which build is active
ug which                  # path to that binary
ug exec 4.7 -- --version  # run Godot without switching
```

Want the editor as plain `godot`? Put the managed shim on your `PATH` (optional;
`ug exec` always works without this):

```sh
export PATH="$(ug shell path):$PATH"
ug use 4.7
godot --version
```

## What you'll actually use

### Switch versions

```sh
ug install 4.6
ug use 4.6

ug install 4.7@mono
ug use 4.7@mono
```

`use` points the managed `godot` shim at an install. It does not touch your
shell config or `PATH`.

`default` remembers a preferred install *and* runs `use` for you. A later
`use` can move the shim without changing the default. `ug default --unset`
clears only the stored default.

### Pin a project

```sh
cd my-game
ug pin 4.7@mono
```

That writes `.ugrc`. From that directory (or below it), these commands pick up
the pin when you omit a version:

```sh
ug install
ug use
ug which
ug exec -- --editor project.godot
```

An explicit selector always wins over `.ugrc`. Commit the file if you want the
team on the same Godot.

### Aliases

Named shortcuts managed by `ug` (not shell aliases):

```sh
ug alias set studio 4.7@mono
ug use studio
ug alias list
ug alias remove studio
```

### Local / custom builds

Official downloads cover `standard` and `mono`. Double-precision, GodotJS, and
other custom apps you import yourself:

```sh
ug install 4.7@double --from "/path/to/Godot Double.app"
ug install 4.7@godotjs --from "/path/to/GodotJS.app"
ug install 4.7@custom:studio --from "/path/to/Godot Studio.app"
```

`ug` copies the source into its own storage. Importing a single-file binary as
`standard` or `mono` also needs `--checksum SHA256`.

## Selectors

| You type | You get |
| --- | --- |
| `latest` | latest stable |
| `4` | latest stable 4.x |
| `4.7` | latest stable 4.7.x |
| `4.7.1` | exactly 4.7.1 |
| `4.8-beta` | latest 4.8 beta |
| `4.8-beta2` | 4.8 beta 2 |
| `4.7@mono` | latest stable 4.7.x .NET |

Prerelease channels: `rc`, `beta`, `alpha`, `dev`. No channel means stable.

Variants are part of the install name, so `4.7` and `4.7@mono` can both live
on disk. Default variant is `standard`. Import-only: `double`, `godotjs`, and
`custom:NAME`.

## Commands

| | |
| --- | --- |
| `ug install [SELECTOR]` | install official or import local |
| `ug list` / `ug list --remote` | installed builds / upstream releases |
| `ug use [SELECTOR]` | select for the `godot` shim |
| `ug default [SELECTOR]` | get, set, or clear the default |
| `ug pin SELECTOR` | write `.ugrc` |
| `ug current` / `ug which` | active name / executable path |
| `ug exec [SELECTOR] -- …` | run Godot without switching |
| `ug alias …` | named selectors |
| `ug uninstall SELECTOR` | remove an install |
| `ug doctor` | check managed state |
| `ug shell …` | shim path, init, completions |

`ug help` and `ug help COMMAND` have the full flags. Useful globals: `--json`,
`--quiet`, `--root` / `UG_ROOT`.

Exit codes: `1` on error, `2` from `doctor` when state is unhealthy, and
`exec` passes through the child process status.

## Shell integration (optional)

You do not need this. Use `ug exec` if you only want a one-off run.

Shell setup is for two conveniences: typing `godot` after `ug use`, and tab
completion. Nothing here edits your startup files; you paste what you want.

```sh
# bash / zsh — PATH + completions for this session
eval "$(ug shell init zsh)"   # or: bash

# or only the godot shim:
export PATH="$(ug shell path):$PATH"

# fish
ug shell init fish | source
# or: fish_add_path --prepend (ug shell path)
```

Completions alone: `ug shell completions SHELL`. Persistent setup and more
detail live in [Shell integration](docs/shell-integration.md).

## Storage and integrity

Data defaults to `~/.local/share/use-godot` (macOS/Linux) or
`%LOCALAPPDATA%\use-godot` (Windows). Point somewhere else with `UG_ROOT` or
`--root` when you want isolation.

Official downloads must match Godot's published checksums (SHA-256 or an entry
in `SHA512-SUMS.txt`). If verification fails, the install aborts. Partial
downloads get cleaned up instead of leaving half-installed trees. Internals are
in [Architecture](docs/architecture.md) if you care.

## Development

```sh
cargo test --all-targets --locked
```

Full gates and release procedures live in [Testing](docs/testing.md) and
[Distribution](docs/distribution.md). Bugs go to [GitHub
Issues](https://github.com/RafaelVidaurre/use-godot/issues).

## License

[MIT](LICENSE)
