# use-godot (`ug`)

`ug` is intended to become a practical Godot version manager: fast enough for
everyday use, safe enough to replace a hand-written symlink switcher, and
pleasant enough to feel like NVM for Godot.

The current machine command is a zsh alias:

```text
ug -> ~/scripts/switch_godot_version.sh
```

That legacy script discovers Godot applications under `/Applications` and
atomically repoints `/usr/local/bin/godot`. It must keep working until the new
CLI has been implemented, tested, and explicitly installed.

## Product brief

The first useful release should support:

- discovering installed Godot builds and identifying standard, double-precision,
  custom, mono/.NET, and GodotJS variants without conflating them;
- downloading and installing official Godot versions, with architecture and
  release-channel awareness and integrity verification;
- selecting exact versions or useful prefixes such as `4` and `4.7`;
- declaring, listing, resolving, updating, and removing named aliases through
  the CLI;
- choosing and displaying a default version;
- running a project or one command with a selected version without necessarily
  changing the default;
- clear `list`, `current`, `which`, `doctor`, `uninstall`, and help workflows;
- shell integration for zsh initially, designed so bash/fish can follow;
- safe migration from the existing `ug` alias and `/usr/local/bin/godot`
  symlink;
- atomic installs/switches, actionable errors, completions, and automated tests.

macOS on Apple Silicon is the immediate target, but the design should avoid
needlessly preventing later Linux, Intel macOS, and Windows support.

The implementation owner should validate the current machine state and turn
this brief into a coherent CLI design before replacing any live shell setup.

