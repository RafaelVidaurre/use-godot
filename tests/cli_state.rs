mod support;

use std::fs;

use predicates::prelude::*;
use tempfile::TempDir;

use support::{assert_shim_targets, fake_godot, shim_path, ug};

#[test]
fn variants_alias_default_exec_and_uninstall_are_end_to_end() {
    let root = TempDir::new().unwrap();
    let sources = TempDir::new().unwrap();
    let standard = fake_godot(&sources, "Godot-standard");
    let double = fake_godot(&sources, "Godot-double");

    ug(root.path())
        .args(["install", "4.7", "--variant", "custom:local", "--from"])
        .arg(&standard)
        .assert()
        .success();
    ug(root.path())
        .args(["install", "4.7", "--variant", "double", "--from"])
        .arg(&double)
        .assert()
        .success();
    ug(root.path())
        .args(["use", "4.7"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ambiguous"));
    ug(root.path())
        .args(["alias", "set", "physics", "4.7@double"])
        .assert()
        .success();
    ug(root.path())
        .args(["use", "physics"])
        .assert()
        .success()
        .stdout(predicate::str::contains("4.7.0-stable@double"));
    ug(root.path())
        .args(["default", "physics"])
        .assert()
        .success();
    ug(root.path())
        .arg("current")
        .assert()
        .success()
        .stdout("4.7.0-stable@double\n");
    ug(root.path())
        .args(["exec", "physics", "--", "--editor", "project.godot"])
        .assert()
        .success()
        .stdout("fake:--editor project.godot\n");
    ug(root.path())
        .args(["uninstall", "physics"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("active or default"));
    ug(root.path())
        .args(["uninstall", "physics", "--force"])
        .assert()
        .success();
    ug(root.path())
        .arg("current")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no active Godot"));
    assert!(!shim_path(root.path()).exists());
}

#[test]
fn doctor_reports_interrupted_staging_without_touching_it() {
    let root = TempDir::new().unwrap();
    let staging = root.path().join("versions/.staging-interrupted");
    fs::create_dir_all(&staging).unwrap();
    ug(root.path())
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("1 recoverable staging/trash"));
    assert!(staging.exists());
}

#[test]
fn default_activates_the_selected_build() {
    let root = TempDir::new().unwrap();
    let sources = TempDir::new().unwrap();
    let source = fake_godot(&sources, "Godot-default");
    ug(root.path())
        .args(["install", "4.7@double", "--from"])
        .arg(source)
        .assert()
        .success();
    ug(root.path())
        .args(["default", "4.7@double"])
        .assert()
        .success();
    ug(root.path())
        .arg("current")
        .assert()
        .success()
        .stdout("4.7.0-stable@double\n");
    #[cfg(unix)]
    assert!(shim_path(root.path()).is_symlink());
    #[cfg(windows)]
    assert!(shim_path(root.path()).is_file());
    ug(root.path())
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("shim").and(predicate::str::contains("ok")));
}

#[test]
fn activating_a_second_build_atomically_replaces_the_shim() {
    let root = TempDir::new().unwrap();
    let sources = TempDir::new().unwrap();
    for (version, name) in [("4.7", "Godot-first"), ("4.8", "Godot-second")] {
        let source = fake_godot(&sources, name);
        ug(root.path())
            .args(["install", version, "--variant", "double", "--from"])
            .arg(source)
            .assert()
            .success();
    }

    ug(root.path())
        .args(["use", "4.7@double"])
        .assert()
        .success();
    let first = which(root.path(), "4.7@double");
    assert_shim_targets(root.path(), &first);

    ug(root.path())
        .args(["use", "4.8@double"])
        .assert()
        .success();
    let second = which(root.path(), "4.8@double");
    assert_ne!(first, second);
    assert_shim_targets(root.path(), &second);
}

#[test]
fn pending_activation_is_recovered_by_next_mutation() {
    let root = TempDir::new().unwrap();
    let sources = TempDir::new().unwrap();
    let source = fake_godot(&sources, "Godot-recovery");
    ug(root.path())
        .args(["install", "4.7@double", "--from"])
        .arg(source)
        .assert()
        .success();
    let canonical = installed_directory_name(&root);
    fs::write(
        root.path().join("pending-operation.json"),
        serde_json::to_vec(&serde_json::json!({
            "operation": "activate",
            "canonical": canonical,
            "set_default": true
        }))
        .unwrap(),
    )
    .unwrap();
    ug(root.path()).args(["alias", "list"]).assert().success();
    assert!(!root.path().join("pending-operation.json").exists());
    ug(root.path())
        .arg("current")
        .assert()
        .success()
        .stdout("4.7.0-stable@double\n");
}

#[test]
fn pending_uninstall_is_recovered_by_next_mutation() {
    let root = TempDir::new().unwrap();
    let sources = TempDir::new().unwrap();
    let source = fake_godot(&sources, "Godot-uninstall-recovery");
    ug(root.path())
        .args(["install", "4.7@double", "--from"])
        .arg(source)
        .assert()
        .success();
    let canonical = installed_directory_name(&root);
    fs::write(
        root.path().join("pending-operation.json"),
        serde_json::to_vec(&serde_json::json!({
            "operation": "uninstall",
            "canonical": canonical
        }))
        .unwrap(),
    )
    .unwrap();
    ug(root.path()).args(["alias", "list"]).assert().success();
    assert!(!root.path().join("pending-operation.json").exists());
    assert_eq!(
        fs::read_dir(root.path().join("versions")).unwrap().count(),
        0
    );
}

fn installed_directory_name(root: &TempDir) -> String {
    fs::read_dir(root.path().join("versions"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .file_name()
        .to_string_lossy()
        .into_owned()
}

fn which(root: &std::path::Path, selector: &str) -> std::path::PathBuf {
    let output = ug(root).args(["which", selector]).output().unwrap();
    assert!(output.status.success());
    std::path::PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
}
