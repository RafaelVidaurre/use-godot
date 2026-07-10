# Repository instructions

- Build `ug` as a production-quality Godot version manager, not as a collection
  of machine-specific shell shortcuts.
- Preserve the existing `~/scripts/switch_godot_version.sh`, `~/.zshrc` alias,
  and `/usr/local/bin/godot` target until an explicit, verified install or
  migration step is ready.
- Never use `sudo`, mutate `/Applications`, rewrite shell startup files, or
  replace the live Godot symlink during automated tests.
- Make filesystem roots and platform services injectable so tests use temporary
  directories and fixtures.
- Prefer atomic operations for downloads, extraction, configuration writes,
  alias changes, and active-version changes.
- Verify downloaded official artifacts using authoritative release metadata or
  checksums. Fail closed when integrity cannot be established.
- Treat build variants as first-class identity. Standard and double-precision
  builds with the same semantic version must be independently installable and
  selectable.
- Keep CLI output scriptable: stable exit codes, a quiet mode where useful, and
  structured output for commands that benefit from automation.
- Document architecture and non-obvious decisions. Add tests for version
  resolution, aliases, variant selection, interrupted operations, and migration.
- Commit implementation in coherent increments and leave the repository in a
  runnable, documented state.
