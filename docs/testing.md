# Testing

Run the complete local gate:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked
python3 scripts/render-release-installers.py --output-dir target/distrib
shellcheck scripts/*.sh target/distrib/use-godot-installer.sh
python3 scripts/smoke-release-installers.py
dist generate --check
cargo package --locked --no-verify
```

Use the `dist` version pinned in `dist-workspace.toml`. Rendering and installer
smoke tests write only below `target/` and temporary directories. The smoke test
uses temporary `HOME`, `LOCALAPPDATA`, `USERPROFILE`, managed-root, and install
paths; it never changes live profiles, the Windows user PATH, or system links.

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

Every CLI test must use a temporary managed root. The shared command fixture
also sets temporary `HOME`, XDG, `LOCALAPPDATA`, and `USERPROFILE` directories
and removes inherited `UG_ROOT` and `UG_RELEASE_API` values. Tests must not read
or write shell startup files, application directories, pre-existing
version-manager state, or system command links. Platform-specific filesystem
assertions must be guarded with `cfg`; shared behavior should remain runnable on
every supported CI platform.

CI runs these tests natively on Linux x86_64, Linux arm64, macOS Apple Silicon,
macOS Intel, and Windows x86_64. Each runner also builds the release binary and
runs `scripts/smoke-release-installers.py`. That smoke installs twice to cover
atomic replacement, runs `ug --version`, performs a fixture-backed local
install/use/which flow, checks `doctor`, and proves that a corrupt archive cannot
replace the verified executable.

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

The current baseline measured with the pinned Rust 1.85 toolchain is 88.18% lines
as of 2026-07-11. CI enforces 87%, rounded down to leave a small margin for
platform instrumentation differences. The inspectable threshold lives in
`.github/coverage-threshold`.
Raise it as coverage improves. Lowering it requires a PR that explains the lost
coverage and why restoring it in the same change is not practical.

Unit tests cover release tags, variant identity, semantic ordering, channel
resolution, alias cycles, ambiguity, import parsing, project-file discovery,
download progress accounting, archive resource limits, atomic publication, and
failure/recovery after every durable activation and uninstall step.

`tests/properties.rs` uses bounded, reproducible property tests for identity
encoding, parser round trips, selector and alias invariants, and modeled state
transitions. Minimized failures are kept in `proptest-regressions/` and must
remain in version control.

Integration tests execute the compiled CLI against temporary roots and cover:

- independent variants, aliases, defaults, active selection, one-shot execution,
  uninstall refusal, and forced reference cleanup;
- `.ugrc` pinning and parent discovery across install/use/which/exec;
- all non-official identity families (`double`, `godotjs`, `custom:name`);
- a mocked official release API and ZIP download with a valid SHA-256 digest;
- checksum rejection with no canonical install or partial download left behind;
- corrupt archives, path traversal, SHA-512 checksum fallback, and temporary
  download cleanup;
- authoritative and absolute download ceilings, archive entry/expanded-size
  ceilings, compression ratio and path-depth limits, and duplicate output-path
  rejection using small injected test policies and compressed fixtures;
- relative roots, manifest containment, managed symlinks, and activation/
  uninstall journal recovery;
- serialization of concurrent mutating processes through the state lock;
- interrupted staging visibility in `doctor`;
- generated zsh, bash, and fish integration from isolated roots.

The hidden `UG_RELEASE_API`/`--api-base` injection exists for deterministic
testing. Production defaults to the official `godotengine/godot-builds` API.
