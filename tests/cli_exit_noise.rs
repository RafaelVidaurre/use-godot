mod support;

use predicates::prelude::*;
use tempfile::tempdir;

use support::ug;

#[test]
fn config_default_is_off_and_toggleable() {
    let root = tempdir().unwrap();

    ug(root.path())
        .args(["config", "get"])
        .assert()
        .success()
        .stdout(predicate::str::contains("tolerate-exit-noise: false"));

    ug(root.path())
        .args(["--quiet", "config", "set", "tolerate-exit-noise", "true"])
        .assert()
        .success();

    ug(root.path())
        .args(["config", "get"])
        .assert()
        .success()
        .stdout(predicate::str::contains("tolerate-exit-noise: true"));

    ug(root.path())
        .args(["--quiet", "config", "set", "tolerate-exit-noise", "false"])
        .assert()
        .success();

    ug(root.path())
        .args(["config", "get"])
        .assert()
        .success()
        .stdout(predicate::str::contains("tolerate-exit-noise: false"));
}

#[test]
fn config_set_unknown_key_errors() {
    let root = tempdir().unwrap();
    ug(root.path())
        .args(["config", "set", "not-a-real-key", "true"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown config key"));
}

#[test]
fn config_get_json_and_path() {
    let root = tempdir().unwrap();
    ug(root.path())
        .args(["--json", "config", "path"])
        .assert()
        .success()
        .stdout(predicate::str::contains("config.json"));

    ug(root.path())
        .args(["--quiet", "config", "set", "tolerate-exit-noise", "1"])
        .assert()
        .success();

    let output = ug(root.path())
        .args(["--json", "config", "get"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        value["configured"]["tolerate_exit_noise"].as_bool(),
        Some(true)
    );
}

#[cfg(unix)]
#[test]
fn tolerate_rewrites_headless_quit_sigabrt_to_zero() {
    let root = tempdir().unwrap();
    let sources = tempdir().unwrap();
    let source = support::fake_godot_signal(&sources, "Godot-abort-quit", 6);
    ug(root.path())
        .args(["--quiet", "install", "4.7@double", "--from"])
        .arg(&source)
        .assert()
        .success();

    // Without tolerate: Unix exec(2) + SIGABRT is not a clean exit for assert_cmd;
    // verify the process does not succeed.
    {
        let mut command = support::ug_process(root.path());
        command.args(["exec", "4.7@double", "--", "--quit"]);
        let output = command.output().unwrap();
        assert!(
            !output.status.success(),
            "expected non-success without tolerate, got {:?}",
            output.status
        );
    }

    // CLI opt-in.
    ug(root.path())
        .args([
            "--tolerate-exit-noise",
            "exec",
            "4.7@double",
            "--",
            "--path",
            "proj",
            "--quit",
        ])
        .assert()
        .code(0)
        .stderr(predicate::str::contains("godot-headless-quit-sigabrt"));

    // Config opt-in.
    ug(root.path())
        .args(["--quiet", "config", "set", "tolerate-exit-noise", "true"])
        .assert()
        .success();
    ug(root.path())
        .args(["--quiet", "exec", "4.7@double", "--", "--quit"])
        .assert()
        .code(0)
        .stderr(predicate::str::is_empty());

    // CLI override off wins over config (wrap off → child SIGABRT again).
    {
        let mut command = support::ug_process(root.path());
        command.args([
            "--no-tolerate-exit-noise",
            "exec",
            "4.7@double",
            "--",
            "--quit",
        ]);
        let output = command.output().unwrap();
        assert!(
            !output.status.success(),
            "expected non-success with --no-tolerate-exit-noise, got {:?}",
            output.status
        );
    }
}

#[cfg(unix)]
#[test]
fn tolerate_does_not_rewrite_sigabrt_without_quit_or_report() {
    let root = tempdir().unwrap();
    let sources = tempdir().unwrap();
    let source = support::fake_godot_signal(&sources, "Godot-abort-editor", 6);
    ug(root.path())
        .args(["--quiet", "install", "4.7@double", "--from"])
        .arg(&source)
        .assert()
        .success();

    ug(root.path())
        .args([
            "--tolerate-exit-noise",
            "exec",
            "4.7@double",
            "--",
            "--editor",
        ])
        .assert()
        .code(predicate::ne(0));
}

#[cfg(unix)]
#[test]
fn tolerate_does_not_rewrite_plain_exit_codes() {
    let root = tempdir().unwrap();
    let sources = tempdir().unwrap();
    let source = support::fake_godot_with_exit(&sources, "Godot-exit-7", 7);
    ug(root.path())
        .args(["--quiet", "install", "4.7@double", "--from"])
        .arg(&source)
        .assert()
        .success();

    ug(root.path())
        .args(["--tolerate-exit-noise", "exec", "4.7@double", "--", "x"])
        .assert()
        .code(7);
}

#[cfg(unix)]
#[test]
fn env_ug_tolerate_exit_noise_enables_wrap() {
    let root = tempdir().unwrap();
    let sources = tempdir().unwrap();
    let source = support::fake_godot_signal(&sources, "Godot-env-abort", 6);
    ug(root.path())
        .args(["--quiet", "install", "4.7@double", "--from"])
        .arg(&source)
        .assert()
        .success();

    ug(root.path())
        .env("UG_TOLERATE_EXIT_NOISE", "1")
        .args(["--quiet", "exec", "4.7@double", "--", "--quit"])
        .assert()
        .code(0);
}

#[cfg(unix)]
#[test]
fn wrap_mode_is_parent_when_tolerate_on() {
    let root = tempdir().unwrap();
    let sources = tempdir().unwrap();
    let source = support::fake_godot_reporting_pid(&sources, "Godot-wrap-pid");
    ug(root.path())
        .args(["--quiet", "install", "4.7@double", "--from"])
        .arg(&source)
        .assert()
        .success();

    let mut command = support::ug_process(root.path());
    command
        .args(["--tolerate-exit-noise", "exec", "4.7@double", "--", "pid"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let child = command.spawn().unwrap();
    let ug_pid = child.id();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let reported: u32 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .expect("pid");
    assert_ne!(
        reported, ug_pid,
        "when tolerate is on, Godot must be a child (different PID)"
    );
}
