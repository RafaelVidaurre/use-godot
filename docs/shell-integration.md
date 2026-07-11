# Shell integration

`ug` works without shell integration. Commands such as `install`, `list`,
`which`, and `exec` are available as soon as `ug` is installed:

```sh
ug exec 4.7 -- --editor project.godot
```

Shell integration adds two optional conveniences:

1. It puts the managed shim on `PATH`, so the build selected by `ug use` can be
   run as `godot`.
2. It enables tab completion for `ug` commands and options.

`ug use` updates the managed shim. It does not modify `PATH`, shell startup
files, or the parent shell's environment.

## Enable the `godot` command

`ug shell path` prints the directory containing the managed `godot` shim. Add
that directory to `PATH` in the current session.

For bash, zsh, and other POSIX-style shells:

```sh
export PATH="$(ug shell path):$PATH"
```

For fish:

```fish
fish_add_path --prepend (ug shell path)
```

After selecting an installed build, `godot` resolves through the shim:

```sh
ug use 4.7
godot --version
```

To make this persistent, place the matching PATH command in the startup file
you already use for that shell. `ug` does not choose or edit one for you.

## Enable completions

Completion scripts are independent of the managed `godot` shim.

For zsh:

```zsh
autoload -Uz compinit && compinit
source <(ug shell completions zsh)
```

If a framework already initialized zsh completions, omit the `compinit` line.

For bash:

```bash
source <(ug shell completions bash)
```

For fish:

```fish
ug shell completions fish | source
```

Scripts can also be generated for PowerShell and Elvish:

```sh
ug shell completions powershell
ug shell completions elvish
```

Where permanent completion files belong depends on the shell and operating
system, so `ug` writes the script to standard output.

## Combined setup for the current session

`ug shell init SHELL` combines PATH setup and completions. It only prints shell
code, so the current shell must evaluate or source that output:

```sh
# zsh
eval "$(ug shell init zsh)"

# bash
eval "$(ug shell init bash)"

# fish
ug shell init fish | source
```

You can inspect the generated code by running `ug shell init SHELL` without
`eval` or `source`. The command does not read or write startup files.
