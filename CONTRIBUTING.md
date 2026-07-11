# Contributing to use-godot

Thank you for helping improve `ug`. Bug reports and focused pull requests are
welcome. Open an issue before starting a large change so its scope and design
can be discussed.

## Development setup

Install Rust 1.85 or newer, clone the repository, and build the project:

```sh
git clone https://github.com/RafaelVidaurre/use-godot.git
cd use-godot
cargo build --locked
```

Tests must use isolated temporary roots. They must not modify shell startup
files, application directories, existing version managers, or system command
links.

## Branches and commits

Create a topic branch from the latest `origin/main`:

```sh
git fetch origin
git switch --create feat/ug-123-channel-aliases --no-track origin/main
```

Use `<type>/<short-kebab-summary>` or
`<type>/<beads-id>-<short-kebab-summary>`. Common types are `feat`, `fix`,
`docs`, `test`, `refactor`, `perf`, `ci`, `build`, `chore`, and `release`.

Commit subjects follow [Conventional Commits](https://www.conventionalcommits.org/):

```text
feat: add channel aliases
fix: recover an interrupted download
docs: clarify shell integration
```

Keep commits focused and independently understandable. Use `!` and a
`BREAKING CHANGE:` footer when a commit intentionally breaks a public
interface.

## Validate changes

Run the complete local gate before requesting review:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked
cargo package --locked --no-verify
shellcheck scripts/*.sh
```

Add or update tests when behavior changes. See [Testing](docs/testing.md) for
the current coverage and fixture strategy.

## Prepare a pull request

Update your topic branch by rebasing it onto the latest `main`:

```sh
git fetch origin
git rebase origin/main
```

Resolve conflicts and rerun the relevant gates. Do not merge `main` into the
topic branch. If the topic branch was already pushed, update it with
`git push --force-with-lease`; never force-push `main`.

Open a pull request against `main`, complete the pull request checklist, and
address review feedback. Required checks and approvals must pass. Pull requests
are integrated with GitHub's rebase-merge option; merge commits and squash
merges are not used. Delete the topic branch after it is merged.

Never push directly to `main`.
