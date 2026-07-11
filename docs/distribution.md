# Distribution

Tagged releases are built by `cargo-dist` using the committed configuration in
`dist-workspace.toml` and `.github/workflows/release.yml`.

The strict version/tag guard lives in the reusable
`.github/workflows/release-policy.yml` workflow and is registered through
`cargo-dist`'s `plan-jobs` configuration. This keeps `release.yml` generated and
regeneratable; do not insert hand-written steps into it.

## Release process

1. Choose the version using the compatibility rules in
   [Versioning](versioning.md).
2. Update the version in `Cargo.toml` and `Cargo.lock`, and move the relevant
   `Unreleased` entries to a dated `CHANGELOG.md` heading.
3. Run `./scripts/check-version-policy.sh` and the complete validation gate
   documented in `docs/testing.md`.
4. Regenerate release automation with the configured `cargo-dist` version.
5. Open and merge the release pull request.
6. Create and push an annotated `vMAJOR.MINOR.PATCH` tag on the release commit.

The release workflow rejects a tag unless it exactly matches the Cargo package
version, lockfile entry, and dated changelog heading. No release command pushes
branches or tags on a contributor's behalf.

The release workflow builds the supported native archive, source archive,
checksums, shell installer, and Homebrew formula. It uploads them to GitHub
Releases, then publishes the formula to the configured tap.

Homebrew users install or upgrade with:

```sh
brew install RafaelVidaurre/tap/ug
brew upgrade RafaelVidaurre/tap/ug
```

The generated shell installer modifies `PATH` unless told otherwise. Published
documentation invokes it with `USE_GODOT_NO_MODIFY_PATH=1` so installation is
non-invasive and shell-independent. The resulting binary is
`$HOME/.cargo/bin/ug` unless the installer is given another prefix.

The tap credential is stored as the `HOMEBREW_TAP_TOKEN` repository secret and
must have write access to the tap repository.
