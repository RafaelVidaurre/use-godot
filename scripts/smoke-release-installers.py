#!/usr/bin/env python3
"""Exercise the rendered release installer without touching live user state."""

from __future__ import annotations

import functools
import hashlib
import http.server
import os
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import threading
import zipfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, _format: str, *_args: object) -> None:
        pass


def run(command: list[str | Path], env: dict[str, str], *, succeeds: bool = True) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
        [str(part) for part in command],
        cwd=ROOT,
        env=env,
        text=True,
        capture_output=True,
        check=False,
    )
    if succeeds and completed.returncode != 0:
        raise SystemExit(
            f"command failed ({completed.returncode}): {' '.join(map(str, command))}\n"
            f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}"
        )
    if not succeeds and completed.returncode == 0:
        raise SystemExit(
            f"command unexpectedly succeeded: {' '.join(map(str, command))}\n"
            f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}"
        )
    return completed


def host_identity() -> tuple[str, str, str]:
    system = platform.system()
    machine = platform.machine().lower()
    if system == "Darwin" and machine in {"arm64", "aarch64"}:
        return "aarch64-apple-darwin", "ug", "tar"
    if system == "Darwin" and machine in {"x86_64", "amd64"}:
        return "x86_64-apple-darwin", "ug", "tar"
    if system == "Linux" and machine in {"arm64", "aarch64"}:
        return "aarch64-unknown-linux-gnu", "ug", "tar"
    if system == "Linux" and machine in {"x86_64", "amd64"}:
        return "x86_64-unknown-linux-gnu", "ug", "tar"
    if system == "Windows" and machine in {"amd64", "x86_64"}:
        return "x86_64-pc-windows-msvc", "ug.exe", "zip"
    raise SystemExit(f"unsupported smoke-test host: {system}/{machine}")


def build_fixture_archive(
    artifact_dir: Path, target: str, binary_name: str, archive_kind: str
) -> Path:
    binary = ROOT / "target" / "release" / binary_name
    if not binary.is_file():
        raise SystemExit(f"release binary is missing: {binary}")

    if archive_kind == "tar":
        archive = artifact_dir / f"use-godot-{target}.tar.xz"
        with tarfile.open(archive, "w:xz") as output:
            output.add(binary, arcname=f"use-godot-{target}/ug")
    else:
        archive = artifact_dir / f"use-godot-{target}.zip"
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as output:
            output.write(binary, arcname="ug.exe")

    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_name(f"{archive.name}.sha256").write_text(
        f"{digest} *{archive.name}\n", encoding="utf-8"
    )
    return archive


def windows_user_path() -> tuple[object, object] | None:
    if platform.system() != "Windows":
        return None
    import winreg

    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, "Environment") as key:
            return winreg.QueryValueEx(key, "Path")
    except FileNotFoundError:
        return None


def assert_profiles_unchanged(home: Path, expected: str) -> None:
    for name in (".profile", ".bashrc", ".zshrc"):
        value = (home / name).read_text(encoding="utf-8")
        if value != expected:
            raise SystemExit(f"installer modified {home / name}")


def assert_shell_rejections(rendered: Path, env: dict[str, str], root: Path) -> None:
    fake_bin = root / "fake-bin"
    fake_bin.mkdir()
    fake_uname = fake_bin / "uname"
    fake_ldd = fake_bin / "ldd"
    rejection_env = env.copy()
    rejection_env["PATH"] = f"{fake_bin}{os.pathsep}{env['PATH']}"

    fake_uname.write_text(
        "#!/bin/sh\n"
        "case \"$1\" in\n"
        "  -s) printf '%s\\n' Linux ;;\n"
        "  -m) printf '%s\\n' riscv64 ;;\n"
        "esac\n",
        encoding="utf-8",
    )
    fake_uname.chmod(0o755)
    unsupported = run(
        ["/bin/sh", rendered / "use-godot-installer.sh"],
        rejection_env,
        succeeds=False,
    )
    if "unsupported Linux architecture" not in unsupported.stderr:
        raise SystemExit("shell installer did not clearly reject an unsupported architecture")

    fake_uname.write_text(
        "#!/bin/sh\n"
        "case \"$1\" in\n"
        "  -s) printf '%s\\n' Linux ;;\n"
        "  -m) printf '%s\\n' x86_64 ;;\n"
        "esac\n",
        encoding="utf-8",
    )
    fake_ldd.write_text("#!/bin/sh\nprintf '%s\\n' 'musl libc (x86_64)'\n", encoding="utf-8")
    fake_ldd.chmod(0o755)
    musl = run(
        ["/bin/sh", rendered / "use-godot-installer.sh"],
        rejection_env,
        succeeds=False,
    )
    if "require glibc" not in musl.stderr:
        raise SystemExit("shell installer did not clearly reject an unsupported libc")


def main() -> None:
    target, binary_name, archive_kind = host_identity()
    with tempfile.TemporaryDirectory(prefix="ug-distribution-smoke-") as directory:
        root = Path(directory)
        artifacts = root / "artifacts"
        rendered = root / "rendered"
        home = root / "home"
        local_app_data = root / "local-app-data"
        artifacts.mkdir()
        home.mkdir()
        local_app_data.mkdir()

        profile_sentinel = "profile must remain unchanged\n"
        for name in (".profile", ".bashrc", ".zshrc"):
            (home / name).write_text(profile_sentinel, encoding="utf-8")

        archive = build_fixture_archive(artifacts, target, binary_name, archive_kind)
        env = os.environ.copy()
        env.update(
            {
                "HOME": str(home),
                "USERPROFILE": str(home),
                "LOCALAPPDATA": str(local_app_data),
                "XDG_CONFIG_HOME": str(root / "xdg-config"),
            }
        )
        env.pop("UG_BIN_DIR", None)
        env.pop("UG_INSTALLER_BASE_URL", None)

        run(
            [
                sys.executable,
                ROOT / "scripts" / "render-release-installers.py",
                "--output-dir",
                rendered,
            ],
            env,
        )

        handler = functools.partial(QuietHandler, directory=str(artifacts))
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        env["UG_INSTALLER_BASE_URL"] = f"http://127.0.0.1:{server.server_port}"

        before_user_path = windows_user_path()
        if platform.system() == "Windows":
            installer_command: list[str | Path] = [
                shutil.which("pwsh") or shutil.which("powershell") or "pwsh",
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                rendered / "use-godot-installer.ps1",
            ]
            installed = local_app_data / "Programs" / "ug" / "bin" / "ug.exe"
        else:
            installer_command = ["/bin/sh", rendered / "use-godot-installer.sh"]
            installed = home / ".local" / "bin" / "ug"
            assert_shell_rejections(rendered, env, root)

        try:
            run(installer_command, env)
            run(installer_command, env)
            if not installed.is_file():
                raise SystemExit(f"installer did not create {installed}")

            run([installed, "--version"], env)
            managed_root = root / "managed"
            selector = "4.7@custom:distribution-smoke"
            run(
                [installed, "--root", managed_root, "install", selector, "--from", installed],
                env,
            )
            run([installed, "--root", managed_root, "use", selector], env)
            run([installed, "--root", managed_root, "which", selector], env)
            run([installed, "--root", managed_root, "doctor"], env)

            installed_digest = hashlib.sha256(installed.read_bytes()).hexdigest()
            with archive.open("ab") as corrupt:
                corrupt.write(b"corrupt-after-valid-archive")
            run(installer_command, env, succeeds=False)
            if hashlib.sha256(installed.read_bytes()).hexdigest() != installed_digest:
                raise SystemExit("failed verification replaced the installed binary")
        finally:
            server.shutdown()
            server.server_close()
            thread.join()

        assert_profiles_unchanged(home, profile_sentinel)
        if windows_user_path() != before_user_path:
            raise SystemExit("PowerShell installer modified the user PATH registry value")
        print(f"native installer smoke passed for {target}")


if __name__ == "__main__":
    main()
