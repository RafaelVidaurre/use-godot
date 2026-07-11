# Beads

This repository uses [`bd`](https://github.com/gastownhall/beads) for issue
tracking. Issues are stored in a Dolt database and synchronized through the
repository's `refs/dolt/data` ref.

JSONL exports and interaction logs are local-only because they can contain
issue-owner or contributor metadata. Do not commit them.

After cloning the repository:

```sh
bd bootstrap
bd hooks install
bd dolt pull
```

Common commands:

```sh
bd ready
bd create "Issue title" --type task --priority 2
bd show <id>
bd update <id> --claim
bd close <id>
bd dolt push
```

Run `bd prime` for the full project workflow.
