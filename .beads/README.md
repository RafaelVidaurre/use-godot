# Beads

This repository uses [`bd`](https://github.com/gastownhall/beads) for local
maintainer issue tracking. Public Dolt synchronization is disabled until a
privacy-safe database can be bootstrapped without historical contributor
metadata.

JSONL exports and interaction logs are local-only because they can contain
issue-owner or contributor metadata. Do not commit them.

After cloning the repository:

```sh
bd bootstrap
bd hooks install
```

Common commands:

```sh
bd ready
bd create "Issue title" --type task --priority 2
bd show <id>
bd update <id> --claim
bd close <id>
```

Run `bd prime` for the full project workflow.
