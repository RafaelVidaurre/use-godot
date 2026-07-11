mod support;

use std::fs;

use predicates::prelude::*;
use tempfile::TempDir;

use support::{
    absolute_path_zip, duplicate_path_zip, excessive_depth_zip, fake_godot, godot_zip,
    high_compression_ratio_zip, missing_executable_zip, mock_release_server,
    mock_release_server_with_size, mock_sha512_release_server, official_binary_path, sha256,
    sha512, traversal_zip, ug,
};

#[cfg(unix)]
use support::escaping_symlink_zip;

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
    let server = mock_release_server(archive.clone(), sha256(&archive));
    ug(root.path())
        .args(["install", "4.7", "--api-base", &server.base_url])
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    server.finish();
    ug(root.path())
        .args(["which", "4.7"])
        .assert()
        .success()
        .stdout(predicate::str::contains(official_binary_path()));
    let names: Vec<_> = fs::read_dir(root.path().join("versions"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 1);
    assert!(!names[0].starts_with(".staging-"));
}

#[test]
fn integrity_failure_leaves_no_partial_install() {
    let root = TempDir::new().unwrap();
    let server = mock_release_server(godot_zip(), "0".repeat(64));
    ug(root.path())
        .args(["install", "4.7", "--api-base", &server.base_url])
        .assert()
        .failure()
        .stderr(predicate::str::contains("integrity check failed"));
    server.finish();
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
#[cfg(unix)]
fn root_source_symlink_is_rejected() {
    let root = TempDir::new().unwrap();
    let sources = TempDir::new().unwrap();
    let source = fake_godot(&sources, "Godot-source");
    let link = sources.path().join("Godot-link");
    std::os::unix::fs::symlink(source, &link).unwrap();
    ug(root.path())
        .args(["install", "4.7@double", "--from"])
        .arg(link)
        .assert()
        .failure()
        .stderr(predicate::str::contains("symlink as the root source"));
}

#[test]
#[cfg(unix)]
fn internal_absolute_symlink_is_rewritten_into_managed_copy() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let root = TempDir::new().unwrap();
    let sources = TempDir::new().unwrap();
    let bundle = sources.path().join("Godot Custom.app");
    let binary = bundle.join("Contents/MacOS/Godot");
    let resource = bundle.join("Contents/Resources/data.txt");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::create_dir_all(resource.parent().unwrap()).unwrap();
    fs::write(&binary, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(&resource, "managed").unwrap();
    symlink(
        resource.canonicalize().unwrap(),
        bundle.join("Contents/Resources/current.txt"),
    )
    .unwrap();

    ug(root.path())
        .args(["install", "4.7@custom:studio", "--from"])
        .arg(&bundle)
        .assert()
        .success();
    fs::remove_dir_all(&bundle).unwrap();

    let managed = fs::read_dir(root.path().join("versions"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
        .join("Godot Custom.app/Contents/Resources/current.txt");
    assert!(managed.is_symlink());
    assert!(!fs::read_link(&managed).unwrap().is_absolute());
    assert_eq!(fs::read_to_string(managed).unwrap(), "managed");
}

#[test]
fn unsafe_manifest_binary_is_rejected() {
    let root = TempDir::new().unwrap();
    let sources = TempDir::new().unwrap();
    let source = fake_godot(&sources, "Godot-manifest");
    ug(root.path())
        .args(["install", "4.7@double", "--from"])
        .arg(source)
        .assert()
        .success();
    let directory = fs::read_dir(root.path().join("versions"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let manifest = directory.join("manifest.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    value["binary"] = serde_json::json!("../../outside");
    fs::write(&manifest, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    ug(root.path())
        .arg("list")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsafe binary path"));
}

#[test]
fn corrupt_archive_leaves_no_partial_download() {
    let root = TempDir::new().unwrap();
    let archive = b"not a zip archive".to_vec();
    let server = mock_release_server(archive.clone(), sha256(&archive));
    ug(root.path())
        .args(["install", "4.7", "--api-base", &server.base_url])
        .assert()
        .failure();
    server.finish();
    assert_eq!(
        fs::read_dir(root.path().join("downloads")).unwrap().count(),
        0
    );
    assert_eq!(
        fs::read_dir(root.path().join("versions")).unwrap().count(),
        0
    );
}

#[test]
fn archive_path_traversal_is_rejected() {
    let root = TempDir::new().unwrap();
    let archive = traversal_zip();
    let server = mock_release_server(archive.clone(), sha256(&archive));
    ug(root.path())
        .args(["install", "4.7", "--api-base", &server.base_url])
        .assert()
        .failure()
        .stderr(predicate::str::contains("escapes destination"));
    server.finish();
    assert!(!root.path().join("versions/escaped").exists());
    assert_eq!(
        fs::read_dir(root.path().join("downloads")).unwrap().count(),
        0
    );
}

#[test]
fn absolute_archive_path_is_rejected() {
    let root = TempDir::new().unwrap();
    let archive = absolute_path_zip();
    let server = mock_release_server(archive.clone(), sha256(&archive));
    ug(root.path())
        .args(["install", "4.7", "--api-base", &server.base_url])
        .assert()
        .failure()
        .stderr(predicate::str::contains("escapes destination"));
    server.finish();
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
#[cfg(unix)]
fn archive_symlink_escape_is_rejected() {
    let root = TempDir::new().unwrap();
    let archive = escaping_symlink_zip();
    let server = mock_release_server(archive.clone(), sha256(&archive));
    ug(root.path())
        .args(["install", "4.7", "--api-base", &server.base_url])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsafe archive symlink"));
    server.finish();
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
fn archive_without_godot_executable_is_rejected() {
    let root = TempDir::new().unwrap();
    let archive = missing_executable_zip();
    let server = mock_release_server(archive.clone(), sha256(&archive));
    ug(root.path())
        .args(["install", "4.7", "--api-base", &server.base_url])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "could not locate Godot executable",
        ));
    server.finish();
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
fn advertised_download_size_mismatch_is_rejected() {
    let root = TempDir::new().unwrap();
    let archive = godot_zip();
    let server =
        mock_release_server_with_size(archive.clone(), sha256(&archive), archive.len() as u64 + 1);
    ug(root.path())
        .args(["install", "4.7", "--api-base", &server.base_url])
        .assert()
        .failure()
        .stderr(predicate::str::contains("integrity check failed"));
    server.finish();
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
fn download_stops_at_authoritative_size_and_cleans_up() {
    let root = TempDir::new().unwrap();
    let archive = godot_zip();
    let server =
        mock_release_server_with_size(archive.clone(), sha256(&archive), archive.len() as u64 - 1);
    ug(root.path())
        .args(["install", "4.7", "--api-base", &server.base_url])
        .assert()
        .failure()
        .stderr(predicate::str::contains("download exceeded safety limit"));
    server.finish();
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
fn archive_resource_policy_failures_leave_no_install() {
    for (archive, expected) in [
        (duplicate_path_zip(), "duplicate output path"),
        (
            high_compression_ratio_zip(),
            "compression ratio safety limit",
        ),
        (excessive_depth_zip(), "path depth"),
    ] {
        let root = TempDir::new().unwrap();
        let server = mock_release_server(archive.clone(), sha256(&archive));
        ug(root.path())
            .args(["install", "4.7", "--api-base", &server.base_url])
            .assert()
            .failure()
            .stderr(predicate::str::contains(expected));
        server.finish();
        assert_eq!(
            fs::read_dir(root.path().join("versions")).unwrap().count(),
            0
        );
        assert_eq!(
            fs::read_dir(root.path().join("downloads")).unwrap().count(),
            0
        );
    }
}

#[test]
fn sha512_sums_fallback_verifies_official_download() {
    let root = TempDir::new().unwrap();
    let archive = godot_zip();
    let server = mock_sha512_release_server(archive.clone(), sha512(&archive));
    ug(root.path())
        .args(["install", "4.7", "--api-base", &server.base_url])
        .assert()
        .success();
    server.finish();
}
