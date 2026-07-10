# Repository instructions

- Build `ug` as a production-quality Godot version manager, not as a collection
  of machine-specific shell shortcuts.
- Treat all shell configuration, application directories, pre-existing version
  managers, and system command links as external user state.
- Never use privilege escalation or mutate external user/system state during
  automated tests.
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
- Support common shells without assuming a user's preferred shell.
- Document architecture and non-obvious decisions. Add tests for version
  resolution, aliases, variant selection, interrupted operations, and shell
  integration.
- Commit implementation in coherent increments and leave the repository in a
  runnable, documented state.
