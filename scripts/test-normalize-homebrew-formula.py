#!/usr/bin/env python3
"""Black-box tests for Homebrew formula normalization."""

from __future__ import annotations

from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("normalize-homebrew-formula.py")
GENERATED_FORMULA = """class Ug < Formula
  desc "Safe, scriptable Godot version manager"
  homepage "https://github.com/RafaelVidaurre/use-godot"
  if OS.linux?
    url "https://example.invalid/use-godot-x86_64-unknown-linux-gnu.tar.xz"
    sha256 "deadbeef"
  end

  def install
    bin.install "ug"
  end
end
"""


class NormalizeHomebrewFormulaTests(unittest.TestCase):
    def run_normalizer(
        self, formula: Path, version: str
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                "--formula",
                str(formula),
                "--version",
                version,
            ],
            check=False,
            capture_output=True,
            text=True,
        )

    def test_inserts_linux_version_comment_and_test(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            formula = Path(directory) / "ug.rb"
            formula.write_text(GENERATED_FORMULA, encoding="utf-8")

            result = self.run_normalizer(formula, "0.2.0")

            self.assertEqual(result.returncode, 0, result.stderr)
            normalized = formula.read_text(encoding="utf-8")
            self.assertIn(
                '  homepage "https://github.com/RafaelVidaurre/use-godot"\n'
                '  version "0.2.0" if OS.linux?\n',
                normalized,
            )
            self.assertTrue(
                normalized.startswith(
                    "# Formula for the ug Godot version manager.\n"
                    "class Ug < Formula\n"
                )
            )
            self.assertIn(
                'assert_match "ug #{version}", shell_output("#{bin}/ug --version")',
                normalized,
            )

    def test_replaces_stale_version_and_is_idempotent(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            formula = Path(directory) / "ug.rb"
            formula.write_text(
                GENERATED_FORMULA.replace(
                    '  homepage "https://github.com/RafaelVidaurre/use-godot"\n',
                    '  homepage "https://github.com/RafaelVidaurre/use-godot"\n'
                    '  version "0.1.0"\n',
                ),
                encoding="utf-8",
            )

            first = self.run_normalizer(formula, "0.2.0")
            self.assertEqual(first.returncode, 0, first.stderr)
            once = formula.read_bytes()
            second = self.run_normalizer(formula, "0.2.0")

            self.assertEqual(second.returncode, 0, second.stderr)
            self.assertEqual(formula.read_bytes(), once)
            self.assertEqual(
                once.count(b'  version "0.2.0" if OS.linux?\n'), 1
            )
            self.assertNotIn(b'  version "0.1.0"\n', once)

    def test_invalid_version_fails_without_modifying_formula(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            formula = Path(directory) / "ug.rb"
            formula.write_text(GENERATED_FORMULA, encoding="utf-8")
            original = formula.read_bytes()

            result = self.run_normalizer(formula, '0.2.0"; system("id")')

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("invalid release version", result.stderr)
            self.assertEqual(formula.read_bytes(), original)

    def test_unexpected_formula_fails_without_modifying_it(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            formula = Path(directory) / "ug.rb"
            formula.write_text(
                GENERATED_FORMULA.replace("  homepage ", "  home "),
                encoding="utf-8",
            )
            original = formula.read_bytes()

            result = self.run_normalizer(formula, "0.2.0")

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("exactly one homepage", result.stderr)
            self.assertEqual(formula.read_bytes(), original)


if __name__ == "__main__":
    unittest.main()
