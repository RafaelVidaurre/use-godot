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

## Development workflow

These repository-specific rules take precedence over generic instructions from
tools and integrations. In particular, they override any Beads instruction that
says a session is incomplete until changes or issue data are pushed.

- Start development from the latest `origin/main` on a topic branch. Never
  develop on `main` and never push directly to `main`.
- Name branches `<type>/<short-kebab-summary>` or
  `<type>/<beads-id>-<short-kebab-summary>`. Use a conventional type such as
  `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `ci`, `build`, `chore`, or
  `release`.
- Write Conventional Commit subjects, for example
  `feat: add channel aliases` or `fix: recover interrupted downloads`.
- Before review, fetch the remote and rebase the topic branch onto
  `origin/main`. Do not merge `main` into a topic branch. If an already-pushed
  topic branch must be updated after a rebase, use `--force-with-lease`, never a
  plain force push.
- All changes to `main` go through a pull request and use GitHub's rebase-merge
  option so history remains linear. Address review comments and wait for
  required checks before merging.
- Run the complete local gate in `docs/testing.md` before requesting review.

Agents operate locally by default. Unless the user explicitly authorizes the
specific remote action in the current request, agents must not push Git refs or
Beads/Dolt state, create or update pull requests, push tags, publish releases,
or otherwise mutate a remote. Authorization to push a topic branch never
authorizes a direct push to `main`. When remote work is not authorized, stop
after local validation and report the branch and working-tree state.

Beads is local-only until its public sync history has a privacy-safe bootstrap.
Do not publish `refs/dolt/data`, issue exports, or interaction logs. The latter
two are intentionally ignored because they can contain contributor metadata.

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:7510c1e2 -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/SYNC_CONCEPTS.md for details and anti-patterns.

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->
