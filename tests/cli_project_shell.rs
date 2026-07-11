mod support;

use std::fs;

use predicates::prelude::*;
use tempfile::TempDir;

use support::{fake_godot, isolated_ug, shim_path, ug};

#[test]
fn shell_integration_is_explicit_for_zsh_bash_and_fish() {
    let root = TempDir::new().unwrap();
    ug(root.path())
        .args(["shell", "path"])
        .assert()
        .success()
        .stdout(format!("{}\n", root.path().join("shims").display()));

    for (shell, marker) in [
        ("zsh", "compdef"),
        ("bash", "complete"),
        ("fish", "fish_add_path"),
    ] {
        ug(root.path())
            .args(["shell", "init", shell])
            .assert()
            .success()
            .stdout(predicate::str::contains(
                root.path().join("shims").to_string_lossy().as_ref(),
            ))
            .stdout(predicate::str::contains(marker));
    }

    ug(root.path())
        .args(["shell", "init", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "if ! type compdef >/dev/null 2>&1",
        ));
}

#[test]
fn project_file_drives_install_use_which_and_exec() {
    let root = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    let sources = TempDir::new().unwrap();
    let child = project.path().join("game/levels");
    fs::create_dir_all(&child).unwrap();
    let double = fake_godot(&sources, "Godot-double");

    ug(root.path())
        .current_dir(project.path())
        .args(["pin", "4.7@double"])
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(project.path().join(".ugrc")).unwrap(),
        "4.7@double\n"
    );

    ug(root.path())
        .current_dir(&child)
        .args(["install", "--from"])
        .arg(&double)
        .assert()
        .success();
    ug(root.path())
        .current_dir(&child)
        .arg("use")
        .assert()
        .success();
    ug(root.path())
        .current_dir(&child)
        .arg("which")
        .assert()
        .success()
        .stdout(predicate::str::contains("Godot-double"));
    ug(root.path())
        .current_dir(&child)
        .args(["exec", "--", "--editor", "project.godot"])
        .assert()
        .success()
        .stdout("fake:--editor project.godot\n");
}

#[test]
fn relative_root_produces_an_absolute_working_shim() {
    let workspace = TempDir::new().unwrap();
    let source = fake_godot(&workspace, "Godot-relative");
    let mut command = isolated_ug(workspace.path(), workspace.path());
    command
        .args(["--root", "managed", "install", "4.7@double", "--from"])
        .arg(&source)
        .assert()
        .success();
    let mut command = isolated_ug(workspace.path(), workspace.path());
    command
        .args(["--root", "managed", "use", "4.7@double"])
        .assert()
        .success();
    let managed_root = workspace.path().join("managed");
    let shim = shim_path(&managed_root);
    #[cfg(unix)]
    assert!(fs::read_link(&shim).unwrap().is_absolute());
    #[cfg(windows)]
    assert!(shim.is_file());
    let output = std::process::Command::new(shim)
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"fake:--version\n");
}
