#!/usr/bin/env python3
"""Exercise a supplied ug and Godot on Windows using only an isolated work dir.

Usage: python smoke-windows-exit-noise.py UG_EXE GODOT_EXE EVIDENCE_DIR
The caller owns admission and the evidence directory. No live config is changed.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def main():
    ug, godot, evidence = (Path(arg).resolve() for arg in sys.argv[1:])
    evidence.mkdir(parents=True, exist_ok=False)
    env = os.environ.copy()
    for name in ["UG_ROOT", "UG_RELEASE_API", "UG_TOLERATE_EXIT_NOISE",
                 "UG_EXIT_NOISE_EXPERIMENTAL", "UG_EXIT_NOISE_DEBUG"]:
        env.pop(name, None)
    for name in ["HOME", "USERPROFILE", "LOCALAPPDATA", "APPDATA",
                 "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"]:
        path = evidence / name.lower()
        path.mkdir()
        env[name] = str(path)
    project = evidence / "project"
    project.mkdir()
    (project / "project.godot").write_text('config_version=5\n[application]\nconfig/name="ug shutdown smoke"\n')
    # A real leaked Node exercises the engine's own ObjectDB cleanup diagnostic.
    (project / "leak.gd").write_text('''extends SceneTree
func _initialize():
    Node.new()
    print("ug-smoke: normal output")
    quit(0)
''')
    (project / "failure.gd").write_text('''extends SceneTree
func _initialize():
    Node.new()
    printerr("ug-smoke: real failure")
    quit(7)
''')
    root = evidence / "managed"
    commands = []

    def run(name, args):
        cmd = [str(ug), "--root", str(root), *args]
        result = subprocess.run(cmd, cwd=project, env=env, capture_output=True, timeout=45)
        (evidence / (name + ".stdout")).write_bytes(result.stdout)
        (evidence / (name + ".stderr")).write_bytes(result.stderr)
        commands.append({"name": name, "argv": cmd, "exit_code": result.returncode})
        return result

    try:
        assert run("install", ["--quiet", "install", "4.7@double", "--from", str(godot)]).returncode == 0
        base = ["exec", "4.7@double", "--", "--headless", "--path", str(project), "--script"]
        raw = run("off", [*base, "leak.gd"])
        filtered = run("on", ["--tolerate-exit-noise", *base, "leak.gd"])
        quiet = run("quiet", ["--quiet", "--tolerate-exit-noise", *base, "leak.gd"])
        assert raw.returncode == filtered.returncode == quiet.returncode == 0
        assert b"ObjectDB" in raw.stdout + raw.stderr, "real Godot did not emit the expected leak"
        for output in (filtered, quiet):
            assert b"ObjectDB" not in output.stdout + output.stderr
            assert b"ug-smoke: normal output" in output.stdout
        assert b"godot-windows-shutdown-leaks" in filtered.stderr
        assert b"godot-windows-shutdown-leaks" not in quiet.stderr
        failure = run("failure", ["--tolerate-exit-noise", *base, "failure.gd"])
        assert failure.returncode == 7
        assert b"ObjectDB" in failure.stdout + failure.stderr
        assert b"ug-smoke: real failure" in failure.stdout + failure.stderr
    finally:
        (evidence / "receipt.json").write_text(json.dumps({
            "ug_sha256": hashlib.sha256(ug.read_bytes()).hexdigest(),
            "engine_sha256": hashlib.sha256(godot.read_bytes()).hexdigest(),
            "commands": commands,
        }, indent=2))
    print("PASS: real Windows Godot shutdown filtering, quiet mode, and failure preservation")


if __name__ == "__main__":
    main()
