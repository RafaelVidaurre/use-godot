# Testing

Run the complete local gate:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

## Test layers

Unit tests live beside the code they exercise. Use them for parsers, resolution,
ordering, and small state transitions. Integration tests under `tests/` execute
the compiled `ug` binary and are grouped by behavior:

- `cli_install.rs` covers imports, official downloads, integrity, and extraction;
- `cli_state.rs` covers selection, aliases, defaults, recovery, and uninstall;
- `cli_project_shell.rs` covers `.ugrc`, shims, and shell output;
- `cli_contract.rs` covers argument and output contracts.

Reusable process, executable, archive, and HTTP fixtures belong in
`tests/support/`. Name tests after an observable behavior, not an implementation
function.

Every CLI test must use a temporary managed root. The shared command fixture also
sets temporary `HOME` and XDG directories and removes inherited `UG_ROOT` and
`UG_RELEASE_API` values. Tests must not read or write shell startup files,
application directories, pre-existing version-manager state, or system command
links. Platform-specific filesystem assertions must be guarded with `cfg`; shared
behavior should remain runnable on every supported CI platform.

## Coverage

CI measures line coverage in a separate workflow with `cargo-llvm-cov` 0.6.21.
The summary is written to the workflow summary, and both the text report and LCOV
data are uploaded as the `coverage` artifact.

Install the pinned local tool and run the same measurement with:

```sh
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --version 0.6.21 --locked
cargo llvm-cov --workspace --all-features --all-targets --locked --summary-only
```

The initial baseline measured with the pinned Rust 1.85 toolchain was 75.63% lines
on 2026-07-11. CI enforces 75%, rounded down to leave a small margin for platform
instrumentation differences. The inspectable threshold lives in
`.github/coverage-threshold`.
Raise it as coverage improves. Lowering it requires a PR that explains the lost
coverage and why restoring it in the same change is not practical.

Unit tests cover release tags, variant identity, semantic ordering, channel
resolution, alias cycles, ambiguity, import parsing, project-file discovery,
and download progress accounting.

Integration tests execute the compiled CLI against temporary roots and cover:

- independent variants, aliases, defaults, active selection, one-shot execution,
  uninstall refusal, and forced reference cleanup;
- `.ugrc` pinning and parent discovery across install/use/which/exec;
- all non-official identity families (`double`, `godotjs`, `custom:name`);
- a mocked official release API and ZIP download with a valid SHA-256 digest;
- checksum rejection with no canonical install or partial download left behind;
- corrupt archives, path traversal, SHA-512 checksum fallback, and temporary
  download cleanup;
- relative roots, manifest containment, managed symlinks, and activation/
  uninstall journal recovery;
- interrupted staging visibility in `doctor`;
- generated zsh, bash, and fish integration from isolated roots.

The hidden `UG_RELEASE_API`/`--api-base` injection exists for deterministic
testing. Production defaults to the official `godotengine/godot-builds` API.
