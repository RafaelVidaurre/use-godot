use std::{
    fs,
    io::{Cursor, Write},
    path::Path,
    sync::Arc,
    thread,
};

use assert_cmd::Command;
use predicates::prelude::*;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tiny_http::{Header, Response, Server};
use zip::write::SimpleFileOptions;

fn ug(root: &Path) -> Command {
    let mut command = Command::cargo_bin("ug").unwrap();
    command.arg("--root").arg(root);
    command
}

fn fake_godot(temp: &TempDir, name: &str) -> std::path::PathBuf {
    let path = temp.path().join(name);
    fs::write(&path, "#!/bin/sh\nprintf 'fake:%s\\n' \"$*\"\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

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
    assert!(!root.path().join("shims/godot").exists());
}

#[test]
fn all_non_official_variant_families_import_independently() {
    let root = TempDir::new().unwrap();
    let sources = TempDir::new().unwrap();
    for (version, variant, filename) in [
        ("4.7", "double", "Godot-double"),
        ("4.7", "godotjs", "GodotJS"),
        ("4.7", "custom:studio", "Godot-studio"),
    ] {
        let source = fake_godot(&sources, filename);
        ug(root.path())
            .args(["install", version, "--variant", variant, "--from"])
            .arg(source)
            .assert()
            .success();
    }
    ug(root.path())
        .args(["list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("double"))
        .stdout(predicate::str::contains("godot-js"))
        .stdout(predicate::str::contains("studio"));
}

#[test]
fn official_download_is_verified_and_committed_atomically() {
    let root = TempDir::new().unwrap();
    let archive = godot_zip();
    let hash = hex::encode(Sha256::digest(&archive));
    let (api, handle) = mock_release_server(archive, hash);
    ug(root.path())
        .args(["install", "4.7", "--api-base", &api])
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    handle.join().unwrap();
    ug(root.path())
        .args(["which", "4.7"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Godot.app/Contents/MacOS/Godot"));
    let names: Vec<_> = fs::read_dir(root.path().join("versions"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 1);
    assert!(!names[0].starts_with(".staging-"));
}

#[test]
fn integrity_failure_leaves_no_partial_install() {
    let root = TempDir::new().unwrap();
    let archive = godot_zip();
    let (api, handle) = mock_release_server(archive, "0".repeat(64));
    ug(root.path())
        .args(["install", "4.7", "--api-base", &api])
        .assert()
        .failure()
        .stderr(predicate::str::contains("integrity check failed"));
    handle.join().unwrap();
    assert_eq!(
        fs::read_dir(root.path().join("versions")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(root.path().join("downloads")).unwrap().count(),
        0
    );
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
fn shell_integration_is_explicit_for_zsh_bash_and_fish() {
    let root = TempDir::new().unwrap();
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

fn godot_zip() -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut cursor);
        let options = SimpleFileOptions::default().unix_permissions(0o755);
        zip.start_file("Godot.app/Contents/MacOS/Godot", options)
            .unwrap();
        zip.write_all(b"#!/bin/sh\nexit 0\n").unwrap();
        zip.finish().unwrap();
    }
    cursor.into_inner()
}

fn mock_release_server(archive: Vec<u8>, digest: String) -> (String, thread::JoinHandle<()>) {
    let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
    let base = format!("http://{}", server.server_addr());
    let asset_url = format!("{base}/Godot_v4.7-stable_macos.universal.zip");
    let body = serde_json::json!([{ "tag_name": "4.7-stable", "draft": false, "prerelease": false, "published_at": "2026-06-18T00:00:00Z", "assets": [{ "name": "Godot_v4.7-stable_macos.universal.zip", "browser_download_url": asset_url, "size": archive.len(), "digest": format!("sha256:{digest}") }] }]).to_string();
    let handle = thread::spawn(move || {
        for _ in 0..2 {
            let request = server.recv().unwrap();
            if request.url().starts_with("/releases?") {
                request
                    .respond(Response::from_string(body.clone()).with_header(
                        Header::from_bytes("Content-Type", "application/json").unwrap(),
                    ))
                    .unwrap();
            } else {
                request
                    .respond(Response::from_data(archive.clone()))
                    .unwrap();
            }
        }
    });
    (base, handle)
}
