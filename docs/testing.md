# Testing

Run the complete local gate:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

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
- interrupted staging visibility in `doctor`;
- generated zsh, bash, and fish integration from isolated roots.

The hidden `UG_RELEASE_API`/`--api-base` injection exists for deterministic
testing. Production defaults to the official `godotengine/godot-builds` API.
