#![allow(dead_code)]

use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::Duration,
};

use assert_cmd::Command;
use sha2::{Digest, Sha256, Sha512};
use tempfile::TempDir;
use tiny_http::{Header, Response, Server};
use zip::write::SimpleFileOptions;

const SERVER_TIMEOUT: Duration = Duration::from_secs(5);

pub fn ug(root: &Path) -> Command {
    let cwd = root.join(".test-environment/cwd");
    let mut command = isolated_ug(root, &cwd);
    command.arg("--root").arg(root);
    command
}

pub fn isolated_ug(environment_root: &Path, cwd: &Path) -> Command {
    let environment = environment_root.join(".test-environment");
    let home = environment.join("home");
    let config = environment.join("config");
    let data = environment.join("data");
    let cache = environment.join("cache");
    for directory in [&home, &config, &data, &cache, cwd] {
        fs::create_dir_all(directory).unwrap();
    }

    let mut command = Command::cargo_bin("ug").unwrap();
    command
        .env_remove("UG_ROOT")
        .env_remove("UG_RELEASE_API")
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", config)
        .env("XDG_DATA_HOME", data)
        .env("XDG_CACHE_HOME", cache)
        .current_dir(cwd);
    command
}

pub fn fake_godot(temp: &TempDir, name: &str) -> PathBuf {
    let path = temp.path().join(name);
    fs::write(&path, "#!/bin/sh\nprintf 'fake:%s\\n' \"$*\"\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

pub fn godot_zip() -> Vec<u8> {
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

pub fn traversal_zip() -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut cursor);
        zip.start_file("../escaped", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"escape").unwrap();
        zip.finish().unwrap();
    }
    cursor.into_inner()
}

pub struct MockReleaseServer {
    pub base_url: String,
    handle: Option<thread::JoinHandle<Result<(), String>>>,
}

impl MockReleaseServer {
    pub fn finish(mut self) {
        let result = self
            .handle
            .take()
            .expect("mock server thread is present")
            .join()
            .expect("mock server thread did not panic");
        if let Err(error) = result {
            panic!("mock release server failed: {error}");
        }
    }
}

impl Drop for MockReleaseServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

pub fn mock_release_server(archive: Vec<u8>, digest: String) -> MockReleaseServer {
    let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
    let base_url = format!("http://{}", server.server_addr());
    let asset_url = format!("{base_url}/Godot_v4.7-stable_macos.universal.zip");
    let body = serde_json::json!([{ "tag_name": "4.7-stable", "draft": false, "prerelease": false, "published_at": "2026-06-18T00:00:00Z", "assets": [{ "name": "Godot_v4.7-stable_macos.universal.zip", "browser_download_url": asset_url, "size": archive.len(), "digest": format!("sha256:{digest}") }] }]).to_string();
    let handle = thread::spawn(move || {
        for request_number in 1..=2 {
            let request = server
                .recv_timeout(SERVER_TIMEOUT)
                .map_err(|error| format!("receive request {request_number}/2: {error}"))?
                .ok_or_else(|| format!("timed out waiting for request {request_number}/2"))?;
            if request.url().starts_with("/releases?") {
                request
                    .respond(Response::from_string(body.clone()).with_header(
                        Header::from_bytes("Content-Type", "application/json").unwrap(),
                    ))
                    .map_err(|error| format!("respond to release request: {error}"))?;
            } else {
                request
                    .respond(Response::from_data(archive.clone()))
                    .map_err(|error| format!("respond to archive request: {error}"))?;
            }
        }
        Ok(())
    });
    MockReleaseServer {
        base_url,
        handle: Some(handle),
    }
}

pub fn mock_sha512_release_server(archive: Vec<u8>, digest: String) -> MockReleaseServer {
    let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
    let base_url = format!("http://{}", server.server_addr());
    let asset_name = "Godot_v4.7-stable_macos.universal.zip";
    let asset_url = format!("{base_url}/{asset_name}");
    let sums_url = format!("{base_url}/SHA512-SUMS.txt");
    let body = serde_json::json!([{
        "tag_name": "4.7-stable",
        "draft": false,
        "prerelease": false,
        "published_at": "2026-06-18T00:00:00Z",
        "assets": [
            { "name": asset_name, "browser_download_url": asset_url, "size": archive.len(), "digest": null },
            { "name": "SHA512-SUMS.txt", "browser_download_url": sums_url, "size": 1, "digest": null }
        ]
    }])
    .to_string();
    let sums = format!("{digest}  {asset_name}\n");
    let handle = thread::spawn(move || {
        for request_number in 1..=3 {
            let request = server
                .recv_timeout(SERVER_TIMEOUT)
                .map_err(|error| format!("receive request {request_number}/3: {error}"))?
                .ok_or_else(|| format!("timed out waiting for request {request_number}/3"))?;
            if request.url().starts_with("/releases?") {
                request
                    .respond(Response::from_string(body.clone()).with_header(
                        Header::from_bytes("Content-Type", "application/json").unwrap(),
                    ))
                    .map_err(|error| format!("respond to release request: {error}"))?;
            } else if request.url() == "/SHA512-SUMS.txt" {
                request
                    .respond(Response::from_string(sums.clone()))
                    .map_err(|error| format!("respond to checksum request: {error}"))?;
            } else {
                request
                    .respond(Response::from_data(archive.clone()))
                    .map_err(|error| format!("respond to archive request: {error}"))?;
            }
        }
        Ok(())
    });
    MockReleaseServer {
        base_url,
        handle: Some(handle),
    }
}

pub fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn sha512(bytes: &[u8]) -> String {
    hex::encode(Sha512::digest(bytes))
}
