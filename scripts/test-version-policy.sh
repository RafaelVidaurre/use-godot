#!/bin/sh

set -eu

script_dir=$(
    CDPATH=''
    cd -P -- "$(dirname -- "$0")"
    pwd
)
checker=$script_dir/check-version-policy.sh
tmp=$(mktemp -d "${TMPDIR:-/tmp}/ug-version-policy.XXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

write_fixture() {
    fixture=$1
    version=${2:-1.2.3}
    lock_version=${3:-$version}
    heading=${4:-"## $version - 2026-07-11"}

    mkdir -p "$fixture"
    {
        echo '[package]'
        echo 'name = "use-godot"'
        echo "version = \"$version\""
    } > "$fixture/Cargo.toml"
    {
        echo 'version = 4'
        echo
        echo '[[package]]'
        echo 'name = "use-godot"'
        echo "version = \"$lock_version\""
    } > "$fixture/Cargo.lock"
    {
        echo '# Changelog'
        echo
        echo "$heading"
    } > "$fixture/CHANGELOG.md"
}

expect_pass() {
    name=$1
    shift
    if ! output=$("$checker" "$@" 2>&1); then
        echo "not ok - $name" >&2
        echo "$output" >&2
        exit 1
    fi
    echo "ok - $name"
}

expect_fail() {
    name=$1
    expected=$2
    shift 2
    if output=$("$checker" "$@" 2>&1); then
        echo "not ok - $name unexpectedly passed" >&2
        exit 1
    fi
    case "$output" in
        *"$expected"*) ;;
        *)
            echo "not ok - $name returned the wrong diagnostic" >&2
            echo "$output" >&2
            exit 1
            ;;
    esac
    echo "ok - $name"
}

valid=$tmp/valid
write_fixture "$valid"
expect_pass "consistent files" --root "$valid"
expect_pass "matching release tag" --root "$valid" --tag v1.2.3

prerelease=$tmp/prerelease
write_fixture "$prerelease" '1.2.3-rc.1'
expect_pass "SemVer prerelease" --root "$prerelease" --tag v1.2.3-rc.1

lock_mismatch=$tmp/lock-mismatch
write_fixture "$lock_mismatch" 1.2.3 1.2.2
expect_fail "lockfile mismatch" "Cargo.lock has use-godot 1.2.2, expected 1.2.3" --root "$lock_mismatch"

bad_heading=$tmp/bad-heading
write_fixture "$bad_heading" 1.2.3 1.2.3 '## 1.2.3'
expect_fail "missing changelog date" "exactly one" --root "$bad_heading"

bad_date=$tmp/bad-date
write_fixture "$bad_date" 1.2.3 1.2.3 '## 1.2.3 - July 11, 2026'
expect_fail "malformed changelog date" "exactly one" --root "$bad_date"

duplicate=$tmp/duplicate
write_fixture "$duplicate"
echo '## 1.2.3 - 2026-07-12' >> "$duplicate/CHANGELOG.md"
expect_fail "duplicate changelog release" "exactly one" --root "$duplicate"

bad_version=$tmp/bad-version
write_fixture "$bad_version" 1.2
expect_fail "incomplete package version" "major.minor.patch" --root "$bad_version"

leading_zero=$tmp/leading-zero
write_fixture "$leading_zero" 1.02.3
expect_fail "leading zero" "invalid package version" --root "$leading_zero"

empty_prerelease=$tmp/empty-prerelease
write_fixture "$empty_prerelease" 1.2.3-
expect_fail "empty prerelease" "invalid package version" --root "$empty_prerelease"

numeric_prerelease=$tmp/numeric-prerelease
write_fixture "$numeric_prerelease" 1.2.3-01
expect_fail "numeric prerelease leading zero" "invalid package version" --root "$numeric_prerelease"

empty_build=$tmp/empty-build
write_fixture "$empty_build" 1.2.3+
expect_fail "empty build metadata" "invalid package version" --root "$empty_build"

expect_fail "mismatched release tag" "must equal 'v1.2.3'" --root "$valid" --tag 1.2.3
