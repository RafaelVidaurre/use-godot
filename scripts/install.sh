#!/bin/sh
set -eu

repo_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
bin_dir=${UG_BIN_DIR:-"${HOME:?HOME must be set}/.local/bin"}
destination="$bin_dir/ug"
temporary="$bin_dir/.ug.tmp.$$"

cleanup() {
    rm -f -- "$temporary"
}
trap cleanup EXIT HUP INT TERM

cd "$repo_dir"
cargo build --release --locked
mkdir -p -- "$bin_dir"
cp -- "target/release/ug" "$temporary"
chmod 755 "$temporary"
mv -f -- "$temporary" "$destination"
trap - EXIT HUP INT TERM

printf 'Installed ug at %s\n' "$destination"
printf 'No shell files or Godot paths were changed.\n'
printf 'Initialize your current shell with one of:\n'
printf "  zsh:  eval \"\$(%s shell init zsh)\"\n" "$destination"
printf "  bash: eval \"\$(%s shell init bash)\"\n" "$destination"
printf '  fish: %s shell init fish | source\n' "$destination"
