#!/bin/sh

set -eu

usage() {
    echo "usage: $0 [--root PATH] [--tag vMAJOR.MINOR.PATCH]" >&2
    exit 2
}

root=.
tag=

while [ "$#" -gt 0 ]; do
    case "$1" in
        --root)
            [ "$#" -ge 2 ] || usage
            root=$2
            shift 2
            ;;
        --tag)
            [ "$#" -ge 2 ] || usage
            tag=$2
            shift 2
            ;;
        *)
            usage
            ;;
    esac
done

manifest=$root/Cargo.toml
lockfile=$root/Cargo.lock
changelog=$root/CHANGELOG.md

for file in "$manifest" "$lockfile" "$changelog"; do
    if [ ! -f "$file" ]; then
        echo "version policy: missing $file" >&2
        exit 1
    fi
done

package_field() {
    field=$1
    awk -v field="$field" '
        /^\[package\][[:space:]]*$/ { package = 1; next }
        /^\[/ { package = 0 }
        package && $0 ~ "^[[:space:]]*" field "[[:space:]]*=" {
            value = $0
            sub("^[^=]*=[[:space:]]*\"", "", value)
            sub("\"[[:space:]]*$", "", value)
            print value
            exit
        }
    ' "$manifest"
}

package_name=$(package_field name)
version=$(package_field version)

if [ -z "$package_name" ] || [ -z "$version" ]; then
    echo "version policy: Cargo.toml must define literal package name and version fields" >&2
    exit 1
fi

# Validate SemVer without requiring Node, Python, or non-POSIX regular-expression
# modes. Cargo performs another definitive parse when it reads the manifest.
case "$version" in
    *[!0-9A-Za-z.+-]* | .* | *..* | *. | *+*+*)
        echo "version policy: invalid package version '$version'" >&2
        exit 1
        ;;
esac

without_build=${version%%+*}
if [ "$without_build" != "$version" ]; then
    build=${version#*+}
    if ! LC_ALL=C awk -v identifiers="$build" '
        BEGIN {
            count = split(identifiers, parts, ".")
            if (identifiers == "" || count == 0) exit 1
            for (i = 1; i <= count; i++) {
                if (parts[i] !~ /^[0-9A-Za-z-]+$/) exit 1
            }
        }
    '; then
        echo "version policy: invalid package version '$version'" >&2
        exit 1
    fi
fi

core=${without_build%%-*}
if [ "$core" != "$without_build" ]; then
    prerelease=${without_build#*-}
    if ! LC_ALL=C awk -v identifiers="$prerelease" '
        BEGIN {
            count = split(identifiers, parts, ".")
            if (identifiers == "" || count == 0) exit 1
            for (i = 1; i <= count; i++) {
                if (parts[i] !~ /^[0-9A-Za-z-]+$/) exit 1
                if (parts[i] ~ /^[0-9]+$/ && parts[i] ~ /^0[0-9]+$/) exit 1
            }
        }
    '; then
        echo "version policy: invalid package version '$version'" >&2
        exit 1
    fi
fi

major=${core%%.*}
remainder=${core#*.}
minor=${remainder%%.*}
patch=${remainder#*.}
if [ "$remainder" = "$core" ] || [ "$patch" = "$remainder" ]; then
    echo "version policy: package version must contain major.minor.patch" >&2
    exit 1
fi
case "$patch" in
    *.*)
        echo "version policy: package version must contain major.minor.patch" >&2
        exit 1
        ;;
esac
for component in "$major" "$minor" "$patch"; do
    case "$component" in
        '' | *[!0-9]* | 0[0-9]*)
            echo "version policy: invalid package version '$version'" >&2
            exit 1
            ;;
    esac
done

lock_version=$(
    awk -v wanted="$package_name" '
        function emit() {
            if (name == wanted) {
                count++
                found = version
            }
        }
        /^\[\[package\]\][[:space:]]*$/ {
            if (in_package) emit()
            in_package = 1
            name = ""
            version = ""
            next
        }
        in_package && /^[[:space:]]*name[[:space:]]*=/ {
            name = $0
            sub("^[^=]*=[[:space:]]*\"", "", name)
            sub("\"[[:space:]]*$", "", name)
            next
        }
        in_package && /^[[:space:]]*version[[:space:]]*=/ {
            version = $0
            sub("^[^=]*=[[:space:]]*\"", "", version)
            sub("\"[[:space:]]*$", "", version)
            next
        }
        END {
            if (in_package) emit()
            if (count == 1) print found
        }
    ' "$lockfile"
)

if [ -z "$lock_version" ]; then
    echo "version policy: Cargo.lock must contain exactly one '$package_name' package" >&2
    exit 1
fi

if [ "$lock_version" != "$version" ]; then
    echo "version policy: Cargo.lock has $package_name $lock_version, expected $version" >&2
    exit 1
fi

heading_status=$(
    awk -v version="$version" '
        index($0, "## " version " ") == 1 {
            count++
            expected = "## " version " - "
            if (index($0, expected) == 1) {
                date = substr($0, length(expected) + 1)
                if (date ~ /^[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]$/) valid++
            }
        }
        END { print count ":" valid }
    ' "$changelog"
)

if [ "$heading_status" != "1:1" ]; then
    echo "version policy: CHANGELOG.md must contain exactly one '## $version - YYYY-MM-DD' heading" >&2
    exit 1
fi

if [ -n "$tag" ] && [ "$tag" != "v$version" ]; then
    echo "version policy: release tag '$tag' must equal 'v$version'" >&2
    exit 1
fi

echo "version policy: $package_name $version is consistent"
