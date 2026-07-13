#!/usr/bin/env python3
"""Render versioned, non-invasive release installers from tracked templates."""

from __future__ import annotations

import argparse
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
TEMPLATES = {
    "use-godot-installer.sh": ROOT / "installers" / "use-godot-installer.sh.in",
    "use-godot-installer.ps1": ROOT / "installers" / "use-godot-installer.ps1.in",
}
VERSION_TOKEN = "@@VERSION@@"


def package_version() -> str:
    manifest = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    package_match = re.search(
        r"(?ms)^\[package\]\s*$\n(?P<body>.*?)(?=^\[|\Z)", manifest
    )
    if package_match is None:
        raise SystemExit("Cargo.toml has no [package] section")
    version_match = re.search(
        r'^version\s*=\s*"(?P<version>[^"]+)"\s*$',
        package_match.group("body"),
        re.MULTILINE,
    )
    if version_match is None:
        raise SystemExit("Cargo.toml [package] has no literal version")
    return version_match.group("version")


def render(output_dir: Path, version: str) -> list[Path]:
    output_dir.mkdir(parents=True, exist_ok=True)
    rendered: list[Path] = []
    for output_name, template_path in TEMPLATES.items():
        template = template_path.read_text(encoding="utf-8")
        if template.count(VERSION_TOKEN) != 1:
            raise SystemExit(
                f"{template_path} must contain exactly one {VERSION_TOKEN} token"
            )
        output = output_dir / output_name
        output.write_text(template.replace(VERSION_TOKEN, version), encoding="utf-8")
        if output.suffix == ".sh":
            output.chmod(0o755)
        rendered.append(output)
    return rendered


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=ROOT / "target" / "distrib",
        help="directory for rendered installer artifacts",
    )
    parser.add_argument(
        "--version",
        help="release version override used by isolated smoke tests",
    )
    args = parser.parse_args()

    version = args.version or package_version()
    for output in render(args.output_dir, version):
        print(output)


if __name__ == "__main__":
    main()
