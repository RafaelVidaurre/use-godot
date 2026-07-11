# Changelog

## Unreleased

- Add `ug shell path` for explicit, shell-neutral access to the managed
  `godot` shim.
- Separate shim activation from completion setup in the shell documentation.
- Avoid rerunning zsh `compinit` when it is already loaded.

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
