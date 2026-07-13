# Distribution

Tagged releases are built by `cargo-dist` 0.32.0 using the committed
configuration in `dist-workspace.toml` and the generated
`.github/workflows/release.yml`.

The strict version/tag guard lives in the reusable
`.github/workflows/release-policy.yml` workflow and is registered through
`cargo-dist`'s `plan-jobs` configuration. This keeps `release.yml` generated and
regeneratable; do not insert hand-written steps into it.

## Release process

1. Choose the version using the compatibility rules in
   [Versioning](versioning.md).
2. Update the version in `Cargo.toml` and `Cargo.lock`, and move the relevant
   `Unreleased` entries to a dated `CHANGELOG.md` heading.
3. Run `./scripts/check-version-policy.sh` and the complete validation gate
   documented in `docs/testing.md`.
4. Regenerate release automation with the configured `cargo-dist` version.
5. Open and merge the release pull request.
6. Create and push an annotated `vMAJOR.MINOR.PATCH` tag on the release commit.

The release workflow rejects a tag unless it exactly matches the Cargo package
version, lockfile entry, and dated changelog heading. No release command pushes
branches or tags on a contributor's behalf.

## Supported release targets

Every target is built on a native GitHub-hosted runner:

| Target triple | Runner | Installer |
| --- | --- | --- |
| `aarch64-apple-darwin` | `macos-15` | shell, Homebrew |
| `x86_64-apple-darwin` | `macos-15-intel` | shell, Homebrew |
| `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` | shell, Homebrew |
| `x86_64-unknown-linux-gnu` | `ubuntu-24.04` | shell, Homebrew |
| `x86_64-pc-windows-msvc` | `windows-2025` | PowerShell |

Linux archives target glibc. Musl, 32-bit systems, and Windows on ARM are not
currently release targets; the installers fail with a clear unsupported-target
message on those systems.

`cargo-dist` builds the native archives, their SHA-256 files, the source
archive, and the Homebrew formula. `scripts/render-release-installers.py`
renders the project-owned shell and PowerShell installers from tracked
templates as extra artifacts. Keeping those installers project-owned makes
their non-invasive defaults testable without editing the generated release
workflow.

Pull requests use `pr-run-mode = "upload"`, so the complete artifact matrix is
built before merge. The regular CI matrix also runs the full Rust test suite,
builds the release binary, and exercises the native standalone installer on
all five runner/architecture combinations.

## Installation channels

Homebrew users install or upgrade with:

```sh
brew install RafaelVidaurre/tap/ug
brew upgrade RafaelVidaurre/tap/ug
```

The standalone installers download the matching archive and its SHA-256 file,
fail closed if either cannot be verified, and publish the executable through a
same-directory atomic replacement. They never edit shell profiles or the
Windows user `PATH` registry value.

- The shell installer uses `$UG_BIN_DIR` or defaults to `$HOME/.local/bin`.
- The PowerShell installer uses `$env:UG_BIN_DIR` or defaults to
  `%LOCALAPPDATA%\Programs\ug\bin`.
- Rust is required only for `cargo install` or a source checkout build.

The public installer commands are:

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/RafaelVidaurre/use-godot/releases/latest/download/use-godot-installer.sh \
  | sh
```

```powershell
irm https://github.com/RafaelVidaurre/use-godot/releases/latest/download/use-godot-installer.ps1 | iex
```

Homebrew formulas include macOS and Linux target branches. The post-release
workflow normalizes the formula once, then runs `brew style`, strict online
audit, installation, and `brew test` on macOS arm64 and Linux x86_64.

The tap credential is stored as the `HOMEBREW_TAP_TOKEN` repository secret and
must have write access to the tap repository.
