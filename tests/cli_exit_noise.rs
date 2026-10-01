mod support;

use std::fs;

use predicates::prelude::*;
use tempfile::{TempDir, tempdir};

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
        .stdout(predicate::str::contains("ug.toml"));

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
    assert!(value["project"]["sources"].as_array().unwrap().is_empty());
}

#[test]
fn ug_toml_overrides_machine_and_child_overrides_parent() {
    let root = tempdir().unwrap();
    let project = TempDir::new().unwrap();
    let child = project.path().join("nested");
    fs::create_dir_all(&child).unwrap();

    ug(root.path())
        .args(["--quiet", "config", "set", "tolerate-exit-noise", "false"])
        .assert()
        .success();

    fs::write(
        project.path().join("ug.toml"),
        "tolerate-exit-noise = true\nexperimental-exit-noise-rules = true\n",
    )
    .unwrap();
    fs::write(child.join("ug.toml"), "tolerate-exit-noise = false\n").unwrap();

    let parent_out = ug(root.path())
        .current_dir(project.path())
        .args(["--json", "config", "get", "--effective"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parent: serde_json::Value = serde_json::from_slice(&parent_out).unwrap();
    assert_eq!(
        parent["configured"]["tolerate_exit_noise"].as_bool(),
        Some(false)
    );
    assert_eq!(
        parent["project"]["tolerate_exit_noise"].as_bool(),
        Some(true)
    );
    assert_eq!(
        parent["project"]["experimental_exit_noise_rules"].as_bool(),
        Some(true)
    );
    assert_eq!(
        parent["effective"]["tolerate_exit_noise"].as_bool(),
        Some(true)
    );
    assert_eq!(
        parent["effective"]["allow_experimental_rules"].as_bool(),
        Some(true)
    );
    assert_eq!(parent["project"]["sources"].as_array().unwrap().len(), 1);

    let child_out = ug(root.path())
        .current_dir(&child)
        .args(["--json", "config", "get", "--effective"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let child_value: serde_json::Value = serde_json::from_slice(&child_out).unwrap();
    assert_eq!(
        child_value["project"]["tolerate_exit_noise"].as_bool(),
        Some(false)
    );
    assert_eq!(
        child_value["project"]["experimental_exit_noise_rules"].as_bool(),
        Some(true)
    );
    assert_eq!(
        child_value["effective"]["tolerate_exit_noise"].as_bool(),
        Some(false)
    );
    assert_eq!(
        child_value["effective"]["allow_experimental_rules"].as_bool(),
        Some(true)
    );
    assert_eq!(
        child_value["project"]["sources"].as_array().unwrap().len(),
        2
    );
}

#[cfg(unix)]
#[test]
fn ug_toml_enables_tolerate_on_exec() {
    let root = tempdir().unwrap();
    let project = TempDir::new().unwrap();
    let sources = tempdir().unwrap();
    let source = support::fake_godot_signal(&sources, "Godot-abort-quit-toml", 6);
    ug(root.path())
        .args(["--quiet", "install", "4.7@double", "--from"])
        .arg(&source)
        .assert()
        .success();

    fs::write(
        project.path().join("ug.toml"),
        "tolerate-exit-noise = true\n",
    )
    .unwrap();

    ug(root.path())
        .current_dir(project.path())
        .args(["--quiet", "exec", "4.7@double", "--", "--quit"])
        .assert()
        .success();
}

#[test]
fn ug_toml_unknown_key_errors() {
    let root = tempdir().unwrap();
    let project = TempDir::new().unwrap();
    fs::write(project.path().join("ug.toml"), "nope = true\n").unwrap();
    ug(root.path())
        .current_dir(project.path())
        .args(["config", "get", "--effective"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("parse"));
}

#[test]
fn config_reads_are_non_mutating_and_config_set_migrates_legacy_file() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path()).unwrap();
    fs::write(
        root.path().join("config.json"),
        r#"{"tolerate_exit_noise":true,"experimental_exit_noise_rules":false}"#,
    )
    .unwrap();
    fs::write(root.path().join("pending-operation.json"), b"{}\n").unwrap();

    ug(root.path())
        .args(["config", "path"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ug.toml"));
    ug(root.path())
        .args(["config", "get"])
        .assert()
        .success()
        .stdout(predicate::str::contains("tolerate-exit-noise: true"));

    assert!(root.path().join("config.json").is_file());
    assert!(!root.path().join("ug.toml").exists());
    assert!(root.path().join("pending-operation.json").is_file());
    assert!(!root.path().join("state.lock").exists());

    fs::remove_file(root.path().join("pending-operation.json")).unwrap();
    ug(root.path())
        .args(["config", "set", "experimental-exit-noise-rules", "true"])
        .assert()
        .success();

    assert!(root.path().join("ug.toml").is_file());
    assert!(!root.path().join("config.json").exists());
    let body = fs::read_to_string(root.path().join("ug.toml")).unwrap();
    assert!(body.contains("tolerate-exit-noise = true"));
    assert!(body.contains("experimental-exit-noise-rules = true"));
}

#[cfg(unix)]
#[test]
fn env_zero_forces_tolerate_off_over_config_and_project() {
    let root = tempdir().unwrap();
    let project = TempDir::new().unwrap();
    let sources = tempdir().unwrap();
    let source = support::fake_godot_signal(&sources, "Godot-env-off", 6);
    ug(root.path())
        .args(["--quiet", "install", "4.7@double", "--from"])
        .arg(&source)
        .assert()
        .success();
    ug(root.path())
        .args(["--quiet", "config", "set", "tolerate-exit-noise", "true"])
        .assert()
        .success();
    fs::write(
        project.path().join("ug.toml"),
        "tolerate-exit-noise = true\n",
    )
    .unwrap();

    let mut command = support::ug_process(root.path());
    command
        .current_dir(project.path())
        .env("UG_TOLERATE_EXIT_NOISE", "0")
        .args(["exec", "4.7@double", "--", "--quit"]);
    let output = command.output().unwrap();
    assert!(
        !output.status.success(),
        "env 0 must force wrap off so SIGABRT stays non-success, got {:?}",
        output.status
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

#[cfg(windows)]
#[test]
fn windows_shutdown_output_policy_preserves_failures_and_unrelated_output() {
    use std::{path::PathBuf, process::Command};

    let root = tempdir().unwrap();
    let sources = tempdir().unwrap();
    // Allows native execution of cross-compiled test artifacts on a host without
    // rustc. CI normally compiles the fixture using its native toolchain.
    let source = if let Some(path) = std::env::var_os("UG_TEST_SHUTDOWN_CHILD") {
        PathBuf::from(path)
    } else {
        let path = sources.path().join("shutdown-child.exe");
        assert!(
            Command::new("rustc")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/shutdown_child.rs"
                ))
                .arg("-o")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        path
    };
    ug(root.path())
        .args(["--quiet", "install", "4.7@double", "--from"])
        .arg(source)
        .assert()
        .success();

    let run = |flags: &[&str], code: &str, mode: &str| {
        let mut command = support::ug_process(root.path());
        command
            .args(flags)
            .args(["exec", "4.7@double", "--", code, mode]);
        command.output().unwrap()
    };
    let raw = run(&[], "0", "normal");
    assert!(raw.status.success());
    assert!(String::from_utf8_lossy(&raw.stdout).contains("were leaked at exit"));
    assert!(String::from_utf8_lossy(&raw.stderr).contains("were leaked at exit"));
    let filtered = run(&["--tolerate-exit-noise"], "0", "normal");
    assert!(filtered.status.success());
    assert_eq!(filtered.stdout, b"stdout marker\r\n");
    let err = String::from_utf8_lossy(&filtered.stderr);
    assert!(err.starts_with("SCRIPT ERROR: unrelated failure remains visible\n"));
    assert!(err.contains("godot-windows-shutdown-leaks (30 diagnostics, raw status 0)"));
    assert!(!err.contains("were leaked"));
    let quiet = run(&["--quiet", "--tolerate-exit-noise"], "0", "normal");
    assert_eq!(quiet.stdout, filtered.stdout);
    assert_eq!(
        quiet.stderr,
        b"SCRIPT ERROR: unrelated failure remains visible\n"
    );
    // Exit 7, access violation, and a nonzero NTSTATUS with low byte zero.
    for (code, expected) in [("7", 7), ("-1073741819", 5), ("-1073741824", 1)] {
        let failure = run(&["--tolerate-exit-noise"], code, "normal");
        assert_eq!(failure.status.code(), Some(expected));
        assert_eq!(failure.stdout, raw.stdout);
        assert_eq!(failure.stderr, raw.stderr);
    }
    let after = run(&["--tolerate-exit-noise"], "0", "after");
    let after_raw = run(&[], "0", "after");
    assert_eq!(after.stdout, after_raw.stdout);
    assert_eq!(after.stderr, after_raw.stderr);
    let large = run(&["--quiet", "--tolerate-exit-noise"], "0", "large");
    assert!(large.status.success());
    assert_eq!(large.stdout.len(), 2048 * 129 + quiet.stdout.len());
    assert_eq!(large.stderr.len(), 2048 * 129 + quiet.stderr.len());

    ug(root.path())
        .args(["config", "set", "tolerate-exit-noise", "true"])
        .assert()
        .success();
    assert_eq!(run(&["--quiet"], "0", "normal").stdout, quiet.stdout);
    let off = run(&["--no-tolerate-exit-noise"], "0", "normal");
    assert_eq!(off.stdout, raw.stdout);
    assert_eq!(off.stderr, raw.stderr);
    let env_off = support::ug_process(root.path())
        .env("UG_TOLERATE_EXIT_NOISE", "0")
        .args(["exec", "4.7@double", "--", "0"])
        .output()
        .unwrap();
    assert_eq!(env_off.stdout, raw.stdout);
    assert_eq!(env_off.stderr, raw.stderr);
}
