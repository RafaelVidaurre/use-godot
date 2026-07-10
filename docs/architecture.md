# Architecture and decisions

## Identity and resolution

An installation identity is the tuple:

```text
(semantic version, release channel, variant, platform, architecture)
```

Its stable storage key resembles
`4.7.0-stable@mono+macos-universal`. `standard`, `mono`, `double`, `godotjs`, and
`custom:name` are never collapsed into one version. A selector is resolved
semantically, not lexically; `4.10` therefore sorts after `4.9`. Stable is the
implicit channel. Prereleases require a channel selector.

Aliases resolve to canonical installed identities rather than floating strings.
This makes an alias update explicit and prevents a future remote release from
silently changing an established environment.

## Modules

- `model`: version/channel/variant identities and persisted manifests.
- `resolve`: installed selector and alias resolution with cycle/ambiguity checks.
- `remote`: cached GitHub release discovery and official asset mapping.
- `install`: streaming hash verification, secure extraction, local import, and
  atomic installation commit.
- `state` / `atomic`: JSON persistence, process locking, and symlink replacement.
- `migration`: preview and narrowly-scoped, backed-up zsh integration.
- `main`: CLI policy, structured output, execution, doctor, and shell support.

The platform, architecture, release API, and entire managed filesystem root are
inputs. Tests therefore exercise real command paths without machine state.

## Authoritative upstream decisions

Decisions were checked on 2026-07-10:

1. Godot loosely follows `major.minor.patch`, treats minor versions as feature
   releases, patch versions as compatible maintenance releases, and labels
   prereleases as non-production. Resolution follows those components and keeps
   stable/RC/beta/dev channels explicit. Source: [Godot release policy](https://docs.godotengine.org/en/stable/about/release_policy.html).
2. Godot's macOS download is a Universal binary supporting Apple Silicon and
   Intel, and standard and `.NET` are separate downloads. Those become
   `standard` and `mono` identities with `macos-universal`. Source:
   [official macOS downloads](https://godotengine.org/download/macos/).
3. Published build artifacts come from the
   [`godotengine/godot-builds` releases](https://github.com/godotengine/godot-builds/releases).
   `ug` consumes its REST release representation instead of scraping HTML.
4. GitHub's release-asset schema includes the uploaded byte size and a
   `sha256:...` digest. `ug` accepts that digest, falling back only to the
   release's published `SHA512-SUMS.txt`, and fails closed otherwise. Source:
   [GitHub REST releases documentation](https://docs.github.com/en/rest/releases/releases).
5. Double precision changes Godot's build ABI and is a compile-time choice; it
   cannot be inferred from the same semantic version. It is consequently a
   distinct imported identity. Sources: [Godot .NET compilation and double
   precision](https://docs.godotengine.org/en/stable/engine_details/development/compiling/compiling_with_dotnet.html)
   and [custom engine/GDExtension compatibility](https://docs.godotengine.org/en/latest/tutorials/scripting/cpp/about_godot_cpp.html).

GodotJS is likewise modeled explicitly, but `ug` does not invent an official
download URL or checksum for a project outside the official editor asset set.

## Atomicity and recovery

Downloads use a `.partial-PID` file. Extraction occurs in
`versions/.staging-*`; archive entries must remain beneath that directory. The
manifest is fsynced before the staging directory is renamed to its canonical
name. Failed operations drop staging and partial data.

JSON files are written to same-directory temporary files, fsynced, and renamed.
The managed `shims/godot` symlink is prepared under a temporary name then
renamed. A filesystem lock serializes all state mutation. Uninstall first
renames the installation to `.trash-*`, then updates references, then removes
the trash. A crash can therefore leave hidden staging/trash evidence, never a
half-populated canonical installation; `doctor` reports it.

No automated path points at `/Applications`, `/usr/local/bin`, or shell startup
files. Those appear only as read-only defaults in `doctor`/migration planning,
or as explicit arguments to confirmed migration.

## Portability boundaries

The core and release mapping include macOS, Linux, and Windows naming. macOS is
the production-tested extraction target in 0.1. Windows symlink replacement and
platform-specific executable discovery need dedicated CI before claiming
production support. Shell integration is generated for zsh; completion
generation already uses a shell-neutral command model.

