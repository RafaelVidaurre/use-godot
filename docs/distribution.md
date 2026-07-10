# Distribution

Tagged releases are built by `cargo-dist` using the committed configuration in
`dist-workspace.toml` and `.github/workflows/release.yml`.

## Release process

1. Update the version in `Cargo.toml` and `CHANGELOG.md`.
2. Run the complete validation gate documented in `docs/testing.md`.
3. Regenerate release automation with the configured `cargo-dist` version.
4. Commit the release changes and push a matching `vMAJOR.MINOR.PATCH` tag.

The release workflow builds the supported native archive, source archive,
checksums, shell installer, and Homebrew formula. It uploads them to GitHub
Releases, then publishes the formula to the configured tap.

Homebrew users install or upgrade with:

```sh
brew install RafaelVidaurre/tap/ug
brew upgrade RafaelVidaurre/tap/ug
```

The tap credential is stored as the `HOMEBREW_TAP_TOKEN` repository secret and
must have write access to the tap repository.
