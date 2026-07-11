#![allow(dead_code)]

use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{Receiver, SyncSender, sync_channel},
    },
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
    Command::from_std(ug_process(root))
}

pub fn ug_process(root: &Path) -> std::process::Command {
    let cwd = root.join(".test-environment/cwd");
    let mut command = isolated_ug_process(root, &cwd);
    command.arg("--root").arg(root);
    command
}

pub fn isolated_ug(environment_root: &Path, cwd: &Path) -> Command {
    Command::from_std(isolated_ug_process(environment_root, cwd))
}

fn isolated_ug_process(environment_root: &Path, cwd: &Path) -> std::process::Command {
    let environment = environment_root.join(".test-environment");
    let home = environment.join("home");
    let config = environment.join("config");
    let data = environment.join("data");
    let cache = environment.join("cache");
    for directory in [&home, &config, &data, &cache, cwd] {
        fs::create_dir_all(directory).unwrap();
    }

    let mut command = std::process::Command::new(assert_cmd::cargo::cargo_bin!("ug"));
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
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let path = temp.path().join(name);
        fs::write(&path, "#!/bin/sh\nprintf 'fake:%s\\n' \"$*\"\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
    #[cfg(windows)]
    {
        let source = temp.path().join(format!("{name}.rs"));
        let path = temp.path().join(format!("{name}.exe"));
        fs::write(
            &source,
            r#"fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    println!("fake:{arguments}");
}
"#,
        )
        .unwrap();
        let status = std::process::Command::new("rustc")
            .args(["--crate-name", "ug_test_godot"])
            .arg(&source)
            .arg("-o")
            .arg(&path)
            .status()
            .expect("run rustc for the Windows executable fixture");
        assert!(status.success(), "compile the Windows executable fixture");
        path
    }
}

pub fn fake_godot_with_exit(temp: &TempDir, name: &str, code: u8) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let path = temp.path().join(name);
        fs::write(&path, format!("#!/bin/sh\nexit {code}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
    #[cfg(windows)]
    {
        let source = temp.path().join(format!("{name}.rs"));
        let path = temp.path().join(format!("{name}.exe"));
        fs::write(
            &source,
            format!("fn main() {{ std::process::exit({code}); }}\n"),
        )
        .unwrap();
        let status = std::process::Command::new("rustc")
            .args(["--crate-name", "ug_test_godot_exit"])
            .arg(&source)
            .arg("-o")
            .arg(&path)
            .status()
            .expect("run rustc for the Windows exit-status fixture");
        assert!(status.success(), "compile the Windows exit-status fixture");
        path
    }
}

pub fn godot_zip() -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut cursor);
        let options = SimpleFileOptions::default().unix_permissions(0o755);
        zip.start_file(official_binary_path(), options).unwrap();
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

pub fn absolute_path_zip() -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut cursor);
        zip.start_file(
            "/absolute/Godot.app/Contents/MacOS/Godot",
            SimpleFileOptions::default().unix_permissions(0o755),
        )
        .unwrap();
        zip.write_all(b"#!/bin/sh\nexit 0\n").unwrap();
        zip.finish().unwrap();
    }
    cursor.into_inner()
}

pub fn missing_executable_zip() -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut cursor);
        zip.start_file("README.txt", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"no executable in this archive").unwrap();
        zip.finish().unwrap();
    }
    cursor.into_inner()
}

pub fn duplicate_path_zip() -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut cursor);
        for (name, contents) in [
            ("duplicate/path", b"#!/bin/sh\nexit 0\n".as_slice()),
            ("duplicate//path", b"replacement".as_slice()),
        ] {
            let options = SimpleFileOptions::default().unix_permissions(0o755);
            zip.start_file(name, options).unwrap();
            zip.write_all(contents).unwrap();
        }
        zip.finish().unwrap();
    }
    cursor.into_inner()
}

pub fn high_compression_ratio_zip() -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut cursor);
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o755);
        zip.start_file(official_binary_path(), options).unwrap();
        zip.write_all(&vec![0; 8 * 1024 * 1024]).unwrap();
        zip.finish().unwrap();
    }
    cursor.into_inner()
}

pub fn excessive_depth_zip() -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut cursor);
        let name = format!("{}/Godot", vec!["nested"; 65].join("/"));
        zip.start_file(name, SimpleFileOptions::default().unix_permissions(0o755))
            .unwrap();
        zip.write_all(b"#!/bin/sh\nexit 0\n").unwrap();
        zip.finish().unwrap();
    }
    cursor.into_inner()
}

#[cfg(unix)]
pub fn escaping_symlink_zip() -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut cursor);
        zip.add_symlink(
            "Godot.app/Contents/MacOS/Godot",
            "../../../../outside",
            SimpleFileOptions::default(),
        )
        .unwrap();
        zip.finish().unwrap();
    }
    cursor.into_inner()
}

pub struct MockReleaseServer {
    pub base_url: String,
    handle: Option<thread::JoinHandle<Result<(), String>>>,
}

pub struct PausedReleaseServer {
    pub base_url: String,
    catalog_requested: Receiver<()>,
    resume_catalog: SyncSender<()>,
    handle: Option<thread::JoinHandle<Result<(), String>>>,
}

impl PausedReleaseServer {
    pub fn wait_until_catalog_requested(&self) {
        self.catalog_requested
            .recv_timeout(SERVER_TIMEOUT)
            .expect("installer did not request the release catalog while holding the lock");
    }

    pub fn resume(&self) {
        self.resume_catalog
            .send(())
            .expect("mock release server stopped before resume");
    }

    pub fn finish(mut self) {
        let result = self
            .handle
            .take()
            .expect("paused mock server thread is present")
            .join()
            .expect("paused mock server thread did not panic");
        if let Err(error) = result {
            panic!("paused mock release server failed: {error}");
        }
    }
}

impl Drop for PausedReleaseServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = self.resume_catalog.try_send(());
            let _ = handle.join();
        }
    }
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
    let advertised_size = archive.len() as u64;
    mock_release_server_with_size(archive, digest, advertised_size)
}

pub fn paused_release_server(archive: Vec<u8>, digest: String) -> PausedReleaseServer {
    let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
    let base_url = format!("http://{}", server.server_addr());
    let asset_name = official_asset_name();
    let asset_url = format!("{base_url}/{asset_name}");
    let body = serde_json::json!([{
        "tag_name": "4.7-stable",
        "draft": false,
        "prerelease": false,
        "published_at": "2026-06-18T00:00:00Z",
        "assets": [{
            "name": asset_name,
            "browser_download_url": asset_url,
            "size": archive.len(),
            "digest": format!("sha256:{digest}")
        }]
    }])
    .to_string();
    let (catalog_requested_tx, catalog_requested) = sync_channel(1);
    let (resume_catalog, resume_catalog_rx) = sync_channel(1);
    let handle = thread::spawn(move || {
        let request = server
            .recv_timeout(SERVER_TIMEOUT)
            .map_err(|error| format!("receive paused catalog request: {error}"))?
            .ok_or_else(|| "timed out waiting for paused catalog request".to_owned())?;
        if !request.url().starts_with("/releases?") {
            return Err(format!(
                "expected release catalog request, got {}",
                request.url()
            ));
        }
        catalog_requested_tx
            .send(())
            .map_err(|error| format!("signal catalog request: {error}"))?;
        resume_catalog_rx
            .recv_timeout(SERVER_TIMEOUT)
            .map_err(|error| format!("wait to resume catalog response: {error}"))?;
        request
            .respond(
                Response::from_string(body)
                    .with_header(Header::from_bytes("Content-Type", "application/json").unwrap()),
            )
            .map_err(|error| format!("respond to paused catalog request: {error}"))?;

        let request = server
            .recv_timeout(SERVER_TIMEOUT)
            .map_err(|error| format!("receive paused archive request: {error}"))?
            .ok_or_else(|| "timed out waiting for paused archive request".to_owned())?;
        request
            .respond(Response::from_data(archive))
            .map_err(|error| format!("respond to paused archive request: {error}"))?;
        Ok(())
    });
    PausedReleaseServer {
        base_url,
        catalog_requested,
        resume_catalog,
        handle: Some(handle),
    }
}

pub fn mock_release_server_with_size(
    archive: Vec<u8>,
    digest: String,
    advertised_size: u64,
) -> MockReleaseServer {
    let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
    let base_url = format!("http://{}", server.server_addr());
    let asset_name = official_asset_name();
    let asset_url = format!("{base_url}/{asset_name}");
    let body = serde_json::json!([{ "tag_name": "4.7-stable", "draft": false, "prerelease": false, "published_at": "2026-06-18T00:00:00Z", "assets": [{ "name": asset_name, "browser_download_url": asset_url, "size": advertised_size, "digest": format!("sha256:{digest}") }] }]).to_string();
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
    let asset_name = official_asset_name();
    let asset_url = format!("{base_url}/{asset_name}");
    let sums_url = format!("{base_url}/SHA512-SUMS.txt");
    let body = serde_json::json!([{
        "tag_name": "4.7-stable",
        "draft": false,
        "prerelease": false,
        "published_at": "2026-06-18T00:00:00Z",
        "assets": [
            { "name": asset_name.clone(), "browser_download_url": asset_url, "size": archive.len(), "digest": null },
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

pub fn shim_path(root: &Path) -> PathBuf {
    root.join("shims")
        .join(if cfg!(windows) { "godot.exe" } else { "godot" })
}

pub fn assert_shim_targets(root: &Path, expected: &Path) {
    assert!(same_file::is_same_file(shim_path(root), expected).unwrap());
}

pub fn official_binary_path() -> String {
    if cfg!(target_os = "macos") {
        "Godot.app/Contents/MacOS/Godot".into()
    } else if cfg!(target_os = "windows") {
        if cfg!(target_arch = "aarch64") {
            "Godot_v4.7-stable_windows_arm64.exe".into()
        } else if cfg!(target_arch = "x86") {
            "Godot_v4.7-stable_win32.exe".into()
        } else {
            "Godot_v4.7-stable_win64.exe".into()
        }
    } else {
        format!(
            "Godot_v4.7-stable_linux.{}",
            if cfg!(target_arch = "aarch64") {
                "arm64"
            } else {
                "x86_64"
            }
        )
    }
}

fn official_asset_name() -> String {
    if cfg!(target_os = "macos") {
        "Godot_v4.7-stable_macos.universal.zip".into()
    } else if cfg!(target_os = "windows") {
        if cfg!(target_arch = "aarch64") {
            "Godot_v4.7-stable_windows_arm64.exe.zip".into()
        } else if cfg!(target_arch = "x86") {
            "Godot_v4.7-stable_win32.exe.zip".into()
        } else {
            "Godot_v4.7-stable_win64.exe.zip".into()
        }
    } else {
        format!(
            "Godot_v4.7-stable_linux.{}.zip",
            if cfg!(target_arch = "aarch64") {
                "arm64"
            } else {
                "x86_64"
            }
        )
    }
}
