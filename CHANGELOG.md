# Changelog

## Unreleased

- Add hierarchical project settings via `ug.toml` (child overrides parent) layered
  over machine `$UG_ROOT/ug.toml`, with CLI/env still winning for exit-noise policy.
- Store machine defaults as `ug.toml` (same keys as project files); migrate legacy
  `$UG_ROOT/config.json` on load/save.
- Keep `.ugrc` as the version pin only; surface project sources on
  `ug config get` / `ug config get --effective`.

## 0.2.0 - 2026-07-13

- Publish checksummed native archives for macOS arm64/x86_64, glibc Linux
  arm64/x86_64, and Windows x86_64.
- Add non-invasive shell and PowerShell release installers with neutral default
  paths, SHA-256 verification, and atomic executable replacement.
- Build and smoke-test the full platform matrix on native GitHub-hosted runners
  before merge, including fixture-backed install/use and recovery checks.
- Generate Homebrew branches for supported macOS and Linux targets and audit
  the published formula on both platforms.
- Remove the premature code-of-conduct and contribution-process documents.

## 0.1.2 - 2026-07-11

- Add `ug shell path` for explicit, shell-neutral access to the managed
  `godot` shim.
- Separate shim activation from completion setup in the shell documentation.
- Avoid rerunning zsh `compinit` when it is already loaded.
- Reject official downloads and archives that exceed fixed resource ceilings
  before publishing an installation.
- Prevent canonical identity collisions when custom target components contain
  delimiter characters.
- Replace the `ug` process with Godot during `ug exec` on Unix so Godot directly
  owns the PID, terminal, signals, and job control.
- Keep internal repository metadata out of published source packages and scan
  reachable history for committed secrets in CI.
- Use native Windows profile directories for managed state and isolate them in
  automated tests.

## 0.1.1 - 2026-07-10

- Make the documented release-installer fallback shell-independent and
  non-invasive by default.
- Normalize, audit, install, and test every published Homebrew formula.
- Use the current GitHub checkout action runtime in CI.

## 0.1.0 - 2026-07-10

- Initial production-oriented CLI with semantic/channel/variant resolution.
- Project-local `.ugrc` selectors with atomic `ug pin` support.
- Verified official standard and .NET downloads plus local custom imports.
- Managed aliases, default/active selection, shims, execution, and uninstall.
- Durable recovery journals for active/default changes and uninstall.
- Remote/installed listing, structured output, diagnostics, zsh/bash/fish
  integration, completions, and interactive install progress.
- Isolated automated coverage for resolution, integrity, atomic staging,
  variants, aliases, interrupted operations, and shell integration.
- Tagged binary releases, a verified release installer, and Homebrew tap
  distribution.
