use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use semver::Version;
use sha2::{Digest as _, Sha256, Sha512};
use tempfile::Builder;

use crate::{
    Channel, Identity, Installation, Variant, atomic,
    model::InstallSource,
    paths::Paths,
    remote::{Digest, ReleaseCatalog},
    state,
};

pub trait InstallReporter {
    fn phase(&self, _message: &str) {}
    fn download_started(&self, _asset: &str, _total_bytes: u64) {}
    fn download_advanced(&self, _bytes: u64) {}
}

pub struct SilentReporter;
impl InstallReporter for SilentReporter {}

pub struct InstallOptions<'a> {
    pub selector: &'a str,
    pub variant: Variant,
    pub platform: &'a str,
    pub arch: &'a str,
    pub from: Option<&'a Path>,
    pub checksum: Option<&'a str>,
    pub refresh: bool,
    pub api_base: Option<&'a str>,
    pub reporter: &'a dyn InstallReporter,
}

pub fn install(paths: &Paths, options: InstallOptions<'_>) -> Result<Installation> {
    paths.ensure()?;
    crate::model::validate_component(options.platform, "platform")?;
    crate::model::validate_component(options.arch, "architecture")?;
    match options.from {
        Some(source) => import(paths, source, options),
        None => official(paths, options),
    }
}

fn official(paths: &Paths, options: InstallOptions<'_>) -> Result<Installation> {
    options.reporter.phase("Resolving official release");
    let catalog = ReleaseCatalog::fetch(paths, options.refresh, options.api_base)?;
    let release = catalog.resolve(options.selector, true)?;
    let (version, channel) = release
        .parsed()
        .context("selected release tag became invalid")?;
    let identity = Identity::new(
        version,
        channel,
        options.variant.clone(),
        options.platform,
        normalized_arch(options.platform, options.arch),
    );
    let destination = paths.install_dir(&identity.canonical());
    if destination.exists() {
        bail!("{} is already installed", identity.display_short());
    }
    let asset = catalog.asset_for(release, &options.variant, options.platform, options.arch)?;
    options.reporter.phase("Reading integrity metadata");
    let digest = catalog.authoritative_digest(release, asset)?;
    let partial = paths
        .downloads()
        .join(format!("{}.partial-{}", asset.name, std::process::id()));
    options.reporter.download_started(&asset.name, asset.size);
    let mut response = catalog
        .client()
        .get(&asset.browser_download_url)
        .send()
        .with_context(|| format!("download {}", asset.browser_download_url))?
        .error_for_status()?;
    let mut output = File::create(&partial)?;
    let (actual_sha256, verified, downloaded) =
        copy_and_hash(&mut response, &mut output, &digest, options.reporter)?;
    output.sync_all()?;
    options.reporter.phase("Verifying download");
    if !verified || (asset.size > 0 && downloaded != asset.size) {
        let _ = fs::remove_file(&partial);
        bail!(
            "integrity check failed for {} (expected {} bytes, received {downloaded})",
            asset.name,
            asset.size
        );
    }

    let temp = Builder::new()
        .prefix(".staging-")
        .tempdir_in(paths.versions())?;
    options.reporter.phase("Extracting archive");
    extract_zip(&partial, temp.path())?;
    let _ = fs::remove_file(&partial);
    let binary = locate_binary(temp.path(), &options.variant, options.platform)?;
    ensure_executable(&binary)?;
    let installation = Installation {
        identity,
        binary: binary.clone(),
        source: InstallSource::Official {
            url: asset.browser_download_url.clone(),
            asset: asset.name.clone(),
        },
        installed_at_unix: now_unix(),
        sha256: Some(actual_sha256),
    };
    state::write_manifest(temp.path(), &installation)?;
    let relative_binary = binary.strip_prefix(temp.path())?.to_owned();
    let staging = temp.keep();
    options.reporter.phase("Committing installation");
    atomic::atomic_dir_commit(staging, &destination)?;
    let mut committed = installation;
    committed.binary = destination.join(relative_binary);
    Ok(committed)
}

fn import(paths: &Paths, source: &Path, options: InstallOptions<'_>) -> Result<Installation> {
    options.reporter.phase("Validating local build");
    if options.variant.is_official_download() && options.checksum.is_none() {
        bail!(
            "local imports for standard/mono require --checksum SHA256; custom, double, and GodotJS imports record their provenance without claiming official integrity"
        );
    }
    if !source.exists() {
        bail!("import source does not exist: {}", source.display());
    }
    let (version, channel) = parse_import_version(options.selector)?;
    let identity = Identity::new(
        version,
        channel,
        options.variant.clone(),
        options.platform,
        normalized_arch(options.platform, options.arch),
    );
    let destination = paths.install_dir(&identity.canonical());
    if destination.exists() {
        bail!("{} is already installed", identity.display_short());
    }
    let temp = Builder::new()
        .prefix(".staging-")
        .tempdir_in(paths.versions())?;
    options.reporter.phase("Importing local build");
    let payload = temp.path().join(
        source
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("Godot")),
    );
    copy_recursively(source, &payload)?;
    let binary = if payload.is_file() {
        payload.clone()
    } else {
        locate_binary(temp.path(), &options.variant, options.platform)?
    };
    ensure_executable(&binary)?;
    let sha256 = if source.is_file() {
        Some(hash_file(source)?)
    } else {
        None
    };
    if let Some(expected) = options.checksum {
        let actual = sha256
            .as_deref()
            .context("--checksum currently requires a binary file source")?;
        if !actual.eq_ignore_ascii_case(expected.trim_start_matches("sha256:")) {
            bail!("import checksum mismatch");
        }
    }
    let installation = Installation {
        identity,
        binary: binary.clone(),
        source: InstallSource::Imported {
            original_path: source.to_owned(),
        },
        installed_at_unix: now_unix(),
        sha256,
    };
    state::write_manifest(temp.path(), &installation)?;
    let relative_binary = binary.strip_prefix(temp.path())?.to_owned();
    let staging_path = temp.keep();
    options.reporter.phase("Committing installation");
    atomic::atomic_dir_commit(staging_path, &destination)?;
    let mut committed = installation;
    committed.binary = destination.join(relative_binary);
    Ok(committed)
}

fn parse_import_version(value: &str) -> Result<(Version, Channel)> {
    if value.contains('-') {
        return crate::model::parse_release_tag(value);
    }
    let mut parts = value.split('.');
    let major = parts.next().context("version is required")?.parse()?;
    let minor = parts
        .next()
        .context("imports require at least major.minor")?
        .parse()?;
    let patch = parts.next().unwrap_or("0").parse()?;
    if parts.next().is_some() {
        bail!("invalid import version '{value}'");
    }
    Ok((Version::new(major, minor, patch), Channel::Stable))
}

fn copy_and_hash(
    reader: &mut impl Read,
    writer: &mut impl Write,
    expected: &Digest,
    reporter: &dyn InstallReporter,
) -> Result<(String, bool, u64)> {
    let mut sha256 = Sha256::new();
    let mut sha512 = Sha512::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        writer.write_all(&buffer[..count])?;
        sha256.update(&buffer[..count]);
        sha512.update(&buffer[..count]);
        total += count as u64;
        reporter.download_advanced(count as u64);
    }
    let actual256 = hex::encode(sha256.finalize());
    let valid = match expected {
        Digest::Sha256(hash) => actual256.eq_ignore_ascii_case(hash),
        Digest::Sha512(hash) => hex::encode(sha512.finalize()).eq_ignore_ascii_case(hash),
    };
    Ok((actual256, valid, total))
}

fn extract_zip(archive: &Path, destination: &Path) -> Result<()> {
    let file = File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file).context("open Godot ZIP archive")?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let relative = entry
            .enclosed_name()
            .with_context(|| format!("archive entry escapes destination: {}", entry.name()))?;
        let output = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output)?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        #[cfg(unix)]
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            use std::os::unix::fs::symlink;
            let mut target = String::new();
            entry.read_to_string(&mut target)?;
            let target = Path::new(&target);
            if target.is_absolute()
                || target
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
            {
                bail!(
                    "unsafe archive symlink {} -> {}",
                    output.display(),
                    target.display()
                );
            }
            symlink(target, &output)?;
            continue;
        }
        let mut file = File::create(&output)?;
        std::io::copy(&mut entry, &mut file)?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&output, fs::Permissions::from_mode(mode))?;
        }
    }
    Ok(())
}

fn locate_binary(root: &Path, _variant: &Variant, platform: &str) -> Result<PathBuf> {
    let mut candidates = Vec::new();
    collect_files(root, &mut candidates)?;
    let found = if platform == "macos" {
        candidates.iter().find(|path| {
            path.components().any(|part| part.as_os_str() == "MacOS")
                && path.file_name().is_some_and(|name| {
                    name.to_string_lossy()
                        .to_ascii_lowercase()
                        .starts_with("godot")
                })
        })
    } else {
        candidates.iter().find(|path| {
            path.file_name().is_some_and(|name| {
                name.to_string_lossy()
                    .to_ascii_lowercase()
                    .starts_with("godot")
            })
        })
    };
    found.cloned().with_context(|| {
        format!(
            "could not locate Godot executable beneath {}",
            root.display()
        )
    })
}

fn collect_files(path: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect_files(&entry.path(), out)?;
        } else if kind.is_file() {
            out.push(entry.path());
        }
    }
    Ok(())
}

fn copy_recursively(source: &Path, destination: &Path) -> Result<()> {
    let source_root = source.canonicalize()?;
    copy_recursively_inner(&source_root, source, destination)
}

fn copy_recursively_inner(source_root: &Path, source: &Path, destination: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(source)?;
        let resolved = if target.is_absolute() {
            target.clone()
        } else {
            source
                .parent()
                .context("symlink has no parent")?
                .join(&target)
        }
        .canonicalize()?;
        if !resolved.starts_with(source_root) {
            bail!(
                "refusing to import external symlink {} -> {}",
                source.display(),
                target.display()
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            symlink(target, destination)?;
            return Ok(());
        }
        #[cfg(not(unix))]
        bail!("symlink imports are not supported on this platform");
    }
    if metadata.is_file() {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, destination)?;
        fs::set_permissions(destination, metadata.permissions())?;
        return Ok(());
    }
    fs::create_dir_all(destination)?;
    fs::set_permissions(destination, metadata.permissions())?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        copy_recursively_inner(
            source_root,
            &entry.path(),
            &destination.join(entry.file_name()),
        )?;
    }
    Ok(())
}

#[cfg(unix)]
fn ensure_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut p = fs::metadata(path)?.permissions();
    p.set_mode(p.mode() | 0o755);
    fs::set_permissions(path, p)?;
    Ok(())
}
#[cfg(windows)]
fn ensure_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    std::io::copy(&mut file, &mut hash)?;
    Ok(hex::encode(hash.finalize()))
}
fn normalized_arch(platform: &str, arch: &str) -> String {
    if platform == "macos" {
        "universal".into()
    } else {
        arch.into()
    }
}
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, io::Cursor};

    struct RecordingReporter(Cell<u64>);
    impl InstallReporter for RecordingReporter {
        fn download_advanced(&self, bytes: u64) {
            self.0.set(self.0.get() + bytes);
        }
    }

    #[test]
    fn imported_version_handles_godot_tags() {
        assert_eq!(parse_import_version("4.7-rc2").unwrap().1, Channel::Rc(2));
    }

    #[test]
    fn download_reports_every_written_byte() {
        let bytes = b"verified download";
        let digest = Digest::Sha256(hex::encode(Sha256::digest(bytes)));
        let reporter = RecordingReporter(Cell::new(0));
        let mut output = Vec::new();
        let (_, verified, total) =
            copy_and_hash(&mut Cursor::new(bytes), &mut output, &digest, &reporter).unwrap();
        assert!(verified);
        assert_eq!(total, bytes.len() as u64);
        assert_eq!(reporter.0.get(), total);
        assert_eq!(output, bytes);
    }
}
