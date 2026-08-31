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
    url "https://github.com/RafaelVidaurre/use-godot/releases/download/v0.2.0/use-godot-x86_64-unknown-linux-gnu.tar.xz"
    sha256 "deadbeef"
  end

  def install
    if OS.mac? && Hardware::CPU.arm?
      bin.install "ug"
    end
    if OS.mac? && Hardware::CPU.intel?
      bin.install "ug"
    end
    if OS.linux? && Hardware::CPU.arm?
      bin.install "ug"
    end
    if OS.linux? && Hardware::CPU.intel?
      bin.install "ug"
    end
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

    def test_inserts_comment_and_test(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            formula = Path(directory) / "ug.rb"
            formula.write_text(GENERATED_FORMULA, encoding="utf-8")

            result = self.run_normalizer(formula, "0.2.0")

            self.assertEqual(result.returncode, 0, result.stderr)
            normalized = formula.read_text(encoding="utf-8")
            self.assertIn(
                '  homepage "https://github.com/RafaelVidaurre/use-godot"\n',
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

    def test_removes_stale_version_and_is_idempotent(self) -> None:
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
            self.assertNotIn(b'  version "0.2.0"', once)
            self.assertNotIn(b'  version "0.1.0"\n', once)

    def test_omits_version_detectable_from_release_urls(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            formula = Path(directory) / "ug.rb"
            formula.write_text(GENERATED_FORMULA, encoding="utf-8")

            result = self.run_normalizer(formula, "0.2.0")

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertNotIn(
                '  version "0.2.0"',
                formula.read_text(encoding="utf-8"),
            )

    def test_normalizes_guarded_install_statements_for_homebrew_style(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            formula = Path(directory) / "ug.rb"
            formula.write_text(GENERATED_FORMULA, encoding="utf-8")

            result = self.run_normalizer(formula, "0.2.0")

            self.assertEqual(result.returncode, 0, result.stderr)
            normalized = formula.read_text(encoding="utf-8")
            for guard in (
                "OS.mac? && Hardware::CPU.arm?",
                "OS.mac? && Hardware::CPU.intel?",
                "OS.linux? && Hardware::CPU.arm?",
                "OS.linux? && Hardware::CPU.intel?",
            ):
                self.assertIn(f'    bin.install "ug" if {guard}\n', normalized)
                self.assertNotIn(f"    if {guard}\n", normalized)

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
