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

The `-` between platform and architecture is structural. Literal `%` and `-`
characters inside either target component are percent-encoded as `%25` and
`%2D` before the storage key is assembled. Without that escaping,
`(platform=a, architecture=b-c)` and `(platform=a-b, architecture=c)` would
produce the same key. Built-in identities such as `macos-universal`,
`linux-x86_64`, and `windows-arm64` contain no escaped characters and remain
unchanged.

Aliases resolve to canonical installed identities rather than floating strings.
This makes an alias update explicit and prevents a future remote release from
silently changing an established environment.

## Modules

- `model`: version/channel/variant identities and persisted manifests.
- `resolve`: installed selector and alias resolution with cycle/ambiguity checks.
- `project`: atomic `.ugrc` pin writes, nearest-parent pin discovery, and
  hierarchical `ug.toml` settings (child overrides parent).
- `remote`: cached GitHub release discovery and official asset mapping.
- `install`: streaming hash verification, secure extraction, local import, and
  atomic installation commit.
- `state` / `atomic`: JSON persistence, process locking, and symlink replacement.
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

Downloads use RAII-managed temporary files that are removed on every success or
failure path. Extraction occurs in `versions/.staging-*`; archive entries must
remain beneath that directory. The manifest is fsynced before the staging
directory is renamed to its canonical name. Failed operations drop staging and
partial data.

Official artifacts are also subject to fixed resource ceilings. A download may
not exceed its authoritative nonzero asset size or 2 GiB, whichever is lower;
an asset declared larger than 2 GiB is rejected before downloading. ZIP
metadata is validated before any entry is extracted. An archive may contain at
most 100,000 entries, each entry may expand to at most 2 GiB, and all entries
may expand to at most 8 GiB. Paths are limited to 64 components. Duplicate
output paths and entries with an uncompressed-to-compressed ratio above 1000:1
are rejected; symlink targets are limited to 4 KiB. Extraction also verifies
that actual bytes match the bounded sizes declared in ZIP metadata. The ratio
is deliberately a permissive backstop for repetitive text and managed
assemblies; the absolute expanded-size ceilings remain the primary disk bound.
These limits are compile-time policy, not inputs controlled by release
metadata.

JSON files are written to same-directory temporary files, fsynced, and renamed.
The managed Godot shim is prepared under a temporary name and atomically
published: a symlink on Unix and a privilege-free hard link on Windows. A
filesystem lock serializes all state mutation. Activation and
uninstall write a durable intent journal before touching multiple files; the
next mutating command completes an interrupted operation idempotently.
Uninstall first renames the installation to `.trash-*`, then updates references,
then removes the trash. A crash can therefore leave recoverable intent or
hidden staging/trash evidence, never a half-populated canonical installation;
`doctor` reports it.

Automated operations remain inside the injected managed root. Shell integration
is emitted to standard output and never assumes or edits a startup file.

## Portability boundaries

The core and release mapping include macOS, Linux, and Windows naming. Native CI
exercises the CLI, managed shim, release-mode binary, and standalone installer
on macOS arm64/x86_64, glibc Linux arm64/x86_64, and Windows x86_64. Platforms
outside that matrix are rejected by the installer rather than receiving an
untested archive. Shell integration is generated for zsh, bash, and fish from
one command model; standalone completions support additional shells.

## One-shot execution

The managed `godot` shim is always a direct filesystem link to the selected
binary; invoking it does not run `ug`. On Unix, `ug exec` uses `exec(2)` so the
Godot process replaces `ug` and keeps the same PID, terminal, signals, and job
control. Windows has no equivalent process-replacement primitive, so `ug exec`
starts Godot, waits, and returns its exit status. This Windows parent process is
the portability cost of preserving synchronous command and exit-code behavior;
normal `godot.exe` shim use remains a direct hard link there as well.

### Exit-noise tolerance (opt-in)

When `tolerate-exit-noise` is enabled, `ug exec` **wraps** Godot as a child on
all platforms, applies built-in fail-closed rules to the wait status, and may
rewrite a matched known false-crash exit to `0`. Default is **off**; unmatched
exits and crash presentation (Godot crash handler, OS Problem Report) are
unchanged.

Resolution order (first set wins):

1. CLI `--tolerate-exit-noise` / `--no-tolerate-exit-noise`
2. `UG_TOLERATE_EXIT_NOISE` (and `UG_EXIT_NOISE_EXPERIMENTAL` for experimental rules)
3. Project `ug.toml` chain from the filesystem root down to the current directory
   (closer files override farther ones per key; omitted keys leave parent/machine
   values). Files under `$UG_ROOT` are skipped so machine config is not reapplied.
4. Machine `$UG_ROOT/ug.toml` (`ug config get|set`), same **keys** as project
   files but a **total** document (missing key → false). Project files are sparse
   overlays (missing key → inherit). Separate from `state.json`.
5. Default off

Legacy `$UG_ROOT/config.json` is still readable. Migration to `ug.toml` runs under
the state lock on `ug config path|get|set` (not on unlocked `ug exec` loads). If
both files exist and disagree, load fails closed.

**Shipped vs deferred (design doc):** this release implements opt-in wrap for
`ug exec`, machine/project `ug.toml`, and the stable headless / stack-chk rules.
Not yet shipped: multi-call managed runtime / shim rebind, Unix signal forwarding
from wrapper to Godot, and doctor config↔shim checks. Wrap mode is spawn+wait only;
signals delivered solely to the wrapper PID are not forwarded (tracked follow-up).

On unmatched Unix SIGABRT, wrap may poll macOS Diagnostic Reports for up to ~2s
before returning. Correlator evidence requires a **PID match** in the report text.

`.ugrc` remains a version pin only and is not a settings file. See
`docs/designs/tolerate-exit-noise.md`.
