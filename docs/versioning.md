# Versioning policy

`ug` uses Semantic Versioning for its CLI, persisted formats, installers, and
public Rust API. Releases use annotated tags named `vMAJOR.MINOR.PATCH`, with
optional SemVer prerelease identifiers.

## Compatibility before 1.0

Cargo treats the left-most non-zero component as the compatibility boundary.
For a `0.y.z` release, `y` therefore acts like the major version and `z` is the
compatible release component. While `ug` is on `0.1.x`:

- breaking changes require `0.2.0`;
- compatible features and fixes use the next `0.1.z` version.

This follows the [Cargo SemVer compatibility guidance][cargo-semver]. It is
more specific than treating every pre-1.0 release as freely incompatible.

[cargo-semver]: https://doc.rust-lang.org/stable/cargo/reference/semver.html

## What is public

Compatibility covers more than the Rust library surface:

- command and option names, selector syntax, documented environment variables,
  and exit-status meanings;
- documented JSON fields and project configuration such as `.ugrc` and `ug.toml`
  (including machine `$UG_ROOT/ug.toml` and legacy `config.json` migration);
- managed manifests and state that a newer `ug` must still read safely;
- installer names and documented non-interactive behavior;
- public items exported by the `use_godot` Rust library.

Removing or changing an established behavior incompatibly requires a breaking
version. Additive commands, options, and fields are normally compatible. Help
wording, diagnostics intended only for people, and undocumented implementation
details may change in a compatible release.

[`cargo-semver-checks`][semver-checks] protects the Rust API. CLI and
persisted-format changes still require review and focused compatibility tests
because Rust API tooling cannot evaluate them.

[semver-checks]: https://github.com/obi1kenobi/cargo-semver-checks

## Commits and release checks

Every non-merge commit uses [Conventional Commits 1.0.0][conventional]:

```text
type(optional-scope): imperative summary
```

Use `feat` for a compatible capability, `fix` for a correction, and `!` or a
`BREAKING CHANGE:` footer for a known compatibility break. `docs`, `test`,
`refactor`, `perf`, `build`, `ci`, `chore`, and `revert` describe other common
changes. The commit marker records intent; it does not replace compatibility
review.

[conventional]: https://www.conventionalcommits.org/en/v1.0.0/

The policy workflow:

1. checks every commit since the latest release with Cocogitto;
2. verifies `Cargo.toml`, `Cargo.lock`, and the dated `CHANGELOG.md` release
   heading agree;
3. compares the public Rust API with the latest release tag;
4. requires a pushed release tag to equal `v` plus the Cargo package version.

Run the repository-owned checks locally with:

```sh
./scripts/test-version-policy.sh
./scripts/check-version-policy.sh
```
