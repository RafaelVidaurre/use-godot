# Shell integration

Shell integration is optional. It is not required to run `ug`; it exposes the
managed `godot` shim and command completions in the current shell.

`ug` cannot modify the environment of its parent shell directly, so it emits
shell code for explicit evaluation. It does not inspect or edit startup files.

## Current session

After installing the binary, choose the command matching the current shell:

```sh
# zsh
eval "$("$HOME/.local/bin/ug" shell init zsh)"

# bash
eval "$("$HOME/.local/bin/ug" shell init bash)"

# fish
"$HOME/.local/bin/ug" shell init fish | source
```

Initialization prepends the managed shim directory and the directory containing
the running `ug` executable to `PATH`, then defines completions for that shell.
It affects only the current process unless the user deliberately places the
command in their own shell configuration.

## Standalone completions

Completion scripts can be generated independently:

```sh
ug shell completions zsh
ug shell completions bash
ug shell completions fish
ug shell completions powershell
ug shell completions elvish
```

The destination and loading mechanism are shell- and operating-system-specific,
so `ug` writes the script to standard output rather than choosing a location.

## Safety

- No shell startup file is read or written.
- No preferred shell is inferred from environment variables or process state.
- Paths are derived from the configured managed root and running executable.
- Shell initialization and completion generation do not change managed state.
