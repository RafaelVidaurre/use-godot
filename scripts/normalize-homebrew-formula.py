#!/usr/bin/env python3
"""Normalize cargo-dist's Homebrew formula without editing it by hand."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import re
import stat
import sys
import tempfile


SEMVER = re.compile(
    r"^(0|[1-9][0-9]*)\."
    r"(0|[1-9][0-9]*)\."
    r"(0|[1-9][0-9]*)"
    r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
)
VERSION_LINE = re.compile(r'^  version "[^"\n]+"\n', re.MULTILINE)
HOMEPAGE_LINE = re.compile(r'^  homepage "[^"\n]+"\n', re.MULTILINE)
CLASS_LINE = "class Ug < Formula"
FORMULA_COMMENT = "# Formula for the ug Godot version manager."
TEST_BLOCK = (
    "\n  test do\n"
    '    assert_match "ug #{version}", shell_output("#{bin}/ug --version")\n'
    "  end\n"
)


class NormalizationError(Exception):
    """Raised when the generated formula does not have the expected shape."""


def normalize(contents: str, version: str) -> str:
    if SEMVER.fullmatch(version) is None:
        raise NormalizationError(f"invalid release version: {version!r}")

    if contents.count(CLASS_LINE) != 1:
        raise NormalizationError("formula must contain exactly one Ug class")
    if len(HOMEPAGE_LINE.findall(contents)) != 1:
        raise NormalizationError("formula must contain exactly one homepage")

    contents = VERSION_LINE.sub("", contents)
    contents = contents.replace(f"{FORMULA_COMMENT}\n", "")
    contents = contents.replace(CLASS_LINE, f"{FORMULA_COMMENT}\n{CLASS_LINE}")
    contents, replacements = HOMEPAGE_LINE.subn(
        lambda match: f'{match.group(0)}  version "{version}"\n',
        contents,
        count=1,
    )
    if replacements != 1:
        raise NormalizationError("could not insert the release version")

    if "  test do\n" not in contents:
        if not contents.endswith("\nend\n"):
            raise NormalizationError("formula must end with its class terminator")
        contents = f"{contents[:-5]}{TEST_BLOCK}end\n"

    return contents


def write_atomic(path: Path, contents: str) -> None:
    original_mode = stat.S_IMODE(path.stat().st_mode)
    descriptor, temporary_name = tempfile.mkstemp(
        prefix=f".{path.name}.",
        dir=path.parent,
        text=True,
    )
    temporary_path = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="") as handle:
            handle.write(contents)
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(temporary_path, original_mode)
        os.replace(temporary_path, path)
    except BaseException:
        temporary_path.unlink(missing_ok=True)
        raise


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--formula", type=Path, required=True)
    parser.add_argument("--version", required=True)
    args = parser.parse_args()

    try:
        original = args.formula.read_text(encoding="utf-8")
        normalized = normalize(original, args.version)
        if normalized != original:
            write_atomic(args.formula, normalized)
    except (OSError, UnicodeError, NormalizationError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
