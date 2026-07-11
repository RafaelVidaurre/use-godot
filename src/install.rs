use std::{
    collections::HashSet,
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

const MIB: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug)]
struct ResourceLimits {
    max_download_bytes: u64,
    max_archive_entries: usize,
    max_archive_entry_bytes: u64,
    max_archive_total_bytes: u64,
    max_compression_ratio: u64,
    max_path_depth: usize,
    max_symlink_target_bytes: u64,
}

const PRODUCTION_LIMITS: ResourceLimits = ResourceLimits {
    max_download_bytes: 2 * 1024 * MIB,
    max_archive_entries: 100_000,
    max_archive_entry_bytes: 2 * 1024 * MIB,
    max_archive_total_bytes: 8 * 1024 * MIB,
    max_compression_ratio: 1_000,
    max_path_depth: 64,
    max_symlink_target_bytes: 4 * 1024,
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
    let max_download_bytes = download_limit(asset.size, PRODUCTION_LIMITS)?;
    options.reporter.phase("Reading integrity metadata");
    let digest = catalog.authoritative_digest(release, asset)?;
    let mut partial = Builder::new()
        .prefix(".download-")
        .suffix(".partial")
        .tempfile_in(paths.downloads())?;
    options.reporter.download_started(&asset.name, asset.size);
    let mut response = catalog
        .client()
        .get(&asset.browser_download_url)
        .send()
        .with_context(|| format!("download {}", asset.browser_download_url))?
        .error_for_status()?;
    let (actual_sha256, verified, downloaded) = copy_and_hash(
        &mut response,
        partial.as_file_mut(),
        &digest,
        options.reporter,
        max_download_bytes,
    )?;
    partial.as_file().sync_all()?;
    options.reporter.phase("Verifying download");
    if !verified || (asset.size > 0 && downloaded != asset.size) {
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
    extract_zip(partial.path(), temp.path())?;
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
    if !source.exists() {
        bail!("import source does not exist: {}", source.display());
    }
    if options.variant.is_official_download() {
        if !source.is_file() {
            bail!(
                "local standard/mono imports must be a single executable file; install official application bundles by version instead"
            );
        }
        if options.checksum.is_none() {
            bail!(
                "local standard/mono imports require --checksum SHA256; custom, double, and GodotJS imports record provenance without claiming official integrity"
            );
        }
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
    max_bytes: u64,
) -> Result<(String, bool, u64)> {
    let mut sha256 = Sha256::new();
    let mut sha512 = Sha512::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let remaining_with_probe = max_bytes.saturating_sub(total).saturating_add(1);
        let read_length = remaining_with_probe.min(buffer.len() as u64) as usize;
        let count = reader.read(&mut buffer[..read_length])?;
        if count == 0 {
            break;
        }
        let next_total = total
            .checked_add(count as u64)
            .context("download byte count overflow")?;
        if next_total > max_bytes {
            bail!("download exceeded safety limit of {max_bytes} bytes");
        }
        writer.write_all(&buffer[..count])?;
        sha256.update(&buffer[..count]);
        sha512.update(&buffer[..count]);
        total = next_total;
        reporter.download_advanced(count as u64);
    }
    let actual256 = hex::encode(sha256.finalize());
    let valid = match expected {
        Digest::Sha256(hash) => actual256.eq_ignore_ascii_case(hash),
        Digest::Sha512(hash) => hex::encode(sha512.finalize()).eq_ignore_ascii_case(hash),
    };
    Ok((actual256, valid, total))
}

fn download_limit(authoritative_size: u64, limits: ResourceLimits) -> Result<u64> {
    if authoritative_size > limits.max_download_bytes {
        bail!(
            "official asset size {authoritative_size} exceeds safety limit of {} bytes",
            limits.max_download_bytes
        );
    }
    Ok(if authoritative_size == 0 {
        limits.max_download_bytes
    } else {
        authoritative_size
    })
}

fn extract_zip(archive: &Path, destination: &Path) -> Result<()> {
    extract_zip_with_limits(archive, destination, PRODUCTION_LIMITS)
}

fn extract_zip_with_limits(
    archive: &Path,
    destination: &Path,
    limits: ResourceLimits,
) -> Result<()> {
    let file = File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file).context("open Godot ZIP archive")?;
    validate_archive_entries(&mut zip, limits)?;
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
            let entry_name = entry.name().to_owned();
            let expected_size = entry.size();
            let mut target_bytes = Vec::new();
            copy_exact_size(&mut entry, &mut target_bytes, expected_size, &entry_name)?;
            let target = String::from_utf8(target_bytes)
                .with_context(|| format!("archive symlink target is not UTF-8: {entry_name}"))?;
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
        let entry_name = entry.name().to_owned();
        let expected_size = entry.size();
        copy_exact_size(&mut entry, &mut file, expected_size, &entry_name)?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&output, fs::Permissions::from_mode(mode))?;
        }
    }
    Ok(())
}

fn validate_archive_entries<R: Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    limits: ResourceLimits,
) -> Result<()> {
    if zip.len() > limits.max_archive_entries {
        bail!(
            "archive contains {} entries, exceeding safety limit of {}",
            zip.len(),
            limits.max_archive_entries
        );
    }

    let mut total_bytes = 0u64;
    let mut outputs = HashSet::with_capacity(zip.len());
    for index in 0..zip.len() {
        let entry = zip.by_index(index)?;
        let relative = entry
            .enclosed_name()
            .with_context(|| format!("archive entry escapes destination: {}", entry.name()))?;
        let depth = relative.components().count();
        if depth > limits.max_path_depth {
            bail!(
                "archive path depth {depth} exceeds safety limit of {}: {}",
                limits.max_path_depth,
                entry.name()
            );
        }
        if !outputs.insert(relative) {
            bail!("archive contains duplicate output path: {}", entry.name());
        }

        let size = entry.size();
        if size > limits.max_archive_entry_bytes {
            bail!(
                "archive entry {} expands to {size} bytes, exceeding per-entry safety limit of {}",
                entry.name(),
                limits.max_archive_entry_bytes
            );
        }
        #[cfg(unix)]
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
            && size > limits.max_symlink_target_bytes
        {
            bail!(
                "archive symlink target {} is {size} bytes, exceeding safety limit of {}",
                entry.name(),
                limits.max_symlink_target_bytes
            );
        }
        total_bytes = total_bytes
            .checked_add(size)
            .context("archive uncompressed size overflow")?;
        if total_bytes > limits.max_archive_total_bytes {
            bail!(
                "archive expands to more than {} bytes",
                limits.max_archive_total_bytes
            );
        }

        let compressed = entry.compressed_size();
        if size > 0 && size > compressed.saturating_mul(limits.max_compression_ratio) {
            bail!(
                "archive entry {} exceeds compression ratio safety limit of {}:1",
                entry.name(),
                limits.max_compression_ratio
            );
        }
    }
    Ok(())
}

fn copy_exact_size(
    reader: &mut impl Read,
    writer: &mut impl Write,
    expected: u64,
    entry_name: &str,
) -> Result<()> {
    let actual = std::io::copy(&mut reader.take(expected.saturating_add(1)), writer)?;
    if actual != expected {
        bail!(
            "archive entry {entry_name} size differs from metadata (expected {expected} bytes, extracted {actual})"
        );
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
    if fs::symlink_metadata(source)?.file_type().is_symlink() {
        bail!(
            "refusing to import a symlink as the root source: {}",
            source.display()
        );
    }
    let source_root = source.canonicalize()?;
    copy_recursively_inner(&source_root, destination, source, destination)
}

fn copy_recursively_inner(
    source_root: &Path,
    destination_root: &Path,
    source: &Path,
    destination: &Path,
) -> Result<()> {
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
            let managed_target = if target.is_absolute() {
                let relative = resolved.strip_prefix(source_root)?;
                relative_path(
                    destination.parent().context("symlink has no parent")?,
                    &destination_root.join(relative),
                )?
            } else {
                target
            };
            symlink(managed_target, destination)?;
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
            destination_root,
            &entry.path(),
            &destination.join(entry.file_name()),
        )?;
    }
    Ok(())
}

fn relative_path(from: &Path, to: &Path) -> Result<PathBuf> {
    let from: Vec<_> = from.components().collect();
    let to: Vec<_> = to.components().collect();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    if common == 0 {
        bail!("cannot create managed relative symlink across filesystem roots");
    }
    let mut relative = PathBuf::new();
    for _ in common..from.len() {
        relative.push("..");
    }
    for component in &to[common..] {
        relative.push(component.as_os_str());
    }
    Ok(relative)
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

    use zip::{CompressionMethod, write::SimpleFileOptions};

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
        let (_, verified, total) = copy_and_hash(
            &mut Cursor::new(bytes),
            &mut output,
            &digest,
            &reporter,
            bytes.len() as u64,
        )
        .unwrap();
        assert!(verified);
        assert_eq!(total, bytes.len() as u64);
        assert_eq!(reporter.0.get(), total);
        assert_eq!(output, bytes);
    }

    #[test]
    fn download_limits_use_declared_size_and_an_absolute_ceiling() {
        let limits = ResourceLimits {
            max_download_bytes: 10,
            ..test_limits()
        };
        assert_eq!(download_limit(0, limits).unwrap(), 10);
        assert_eq!(download_limit(7, limits).unwrap(), 7);
        assert!(download_limit(11, limits).is_err());

        let bytes = b"sixteen-byte-data";
        let digest = Digest::Sha256(hex::encode(Sha256::digest(bytes)));
        let reporter = RecordingReporter(Cell::new(0));
        let mut output = Vec::new();
        let error = copy_and_hash(
            &mut Cursor::new(bytes),
            &mut output,
            &digest,
            &reporter,
            (bytes.len() - 1) as u64,
        )
        .unwrap_err();
        assert!(error.to_string().contains("download exceeded safety limit"));
        assert!(output.is_empty());
        assert_eq!(reporter.0.get(), 0);
    }

    #[test]
    fn archive_entry_count_is_bounded_before_extraction() {
        let archive = make_archive(
            &[("one", b"1".as_slice()), ("two", b"2".as_slice())],
            CompressionMethod::Stored,
        );
        assert_archive_rejected(
            &archive,
            ResourceLimits {
                max_archive_entries: 1,
                ..test_limits()
            },
            "entries",
        );
    }

    #[test]
    fn archive_per_entry_and_total_sizes_are_bounded_before_extraction() {
        let archive = make_archive(&[("one", b"1234".as_slice())], CompressionMethod::Stored);
        assert_archive_rejected(
            &archive,
            ResourceLimits {
                max_archive_entry_bytes: 3,
                ..test_limits()
            },
            "per-entry safety limit",
        );

        let archive = make_archive(
            &[("one", b"123".as_slice()), ("two", b"456".as_slice())],
            CompressionMethod::Stored,
        );
        assert_archive_rejected(
            &archive,
            ResourceLimits {
                max_archive_total_bytes: 5,
                ..test_limits()
            },
            "expands to more than",
        );
    }

    #[test]
    fn archive_compression_ratio_is_bounded_before_extraction() {
        let payload = vec![0; 4096];
        let archive = make_archive(
            &[("compressed", payload.as_slice())],
            CompressionMethod::Deflated,
        );
        assert_archive_rejected(
            &archive,
            ResourceLimits {
                max_archive_entry_bytes: 10_000,
                max_archive_total_bytes: 10_000,
                max_compression_ratio: 2,
                ..test_limits()
            },
            "compression ratio safety limit",
        );
    }

    #[test]
    fn archive_path_depth_and_duplicate_outputs_are_bounded_before_extraction() {
        let archive = make_archive(
            &[("one/two/three/file", b"data".as_slice())],
            CompressionMethod::Stored,
        );
        assert_archive_rejected(
            &archive,
            ResourceLimits {
                max_path_depth: 3,
                ..test_limits()
            },
            "path depth",
        );

        let archive = make_archive(
            &[
                ("duplicate/path", b"first".as_slice()),
                ("duplicate//path", b"second".as_slice()),
            ],
            CompressionMethod::Stored,
        );
        assert_archive_rejected(&archive, test_limits(), "duplicate output path");
    }

    #[test]
    fn extracted_bytes_must_match_bounded_archive_metadata() {
        for (expected, message) in [(3, "extracted 4"), (5, "extracted 4")] {
            let mut output = Vec::new();
            let error = copy_exact_size(&mut Cursor::new(b"data"), &mut output, expected, "entry")
                .unwrap_err();
            assert!(error.to_string().contains(message));
        }
    }

    #[test]
    #[cfg(unix)]
    fn archive_symlink_target_size_is_bounded_before_extraction() {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut cursor);
            zip.add_symlink("link", "long-target", SimpleFileOptions::default())
                .unwrap();
            zip.finish().unwrap();
        }
        assert_archive_rejected(
            &cursor.into_inner(),
            ResourceLimits {
                max_symlink_target_bytes: 3,
                ..test_limits()
            },
            "symlink target",
        );
    }

    fn test_limits() -> ResourceLimits {
        ResourceLimits {
            max_download_bytes: 100,
            max_archive_entries: 10,
            max_archive_entry_bytes: 100,
            max_archive_total_bytes: 100,
            max_compression_ratio: 1_000,
            max_path_depth: 10,
            max_symlink_target_bytes: 100,
        }
    }

    fn make_archive(entries: &[(&str, &[u8])], method: CompressionMethod) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut cursor);
            for (name, contents) in entries {
                zip.start_file(
                    *name,
                    SimpleFileOptions::default().compression_method(method),
                )
                .unwrap();
                zip.write_all(contents).unwrap();
            }
            zip.finish().unwrap();
        }
        cursor.into_inner()
    }

    fn assert_archive_rejected(archive: &[u8], limits: ResourceLimits, expected: &str) {
        let source = tempfile::NamedTempFile::new().unwrap();
        fs::write(source.path(), archive).unwrap();
        let destination = tempfile::tempdir().unwrap();
        let error = match extract_zip_with_limits(source.path(), destination.path(), limits) {
            Ok(()) => panic!("expected archive rejection for {expected}"),
            Err(error) => error,
        };
        assert!(
            format!("{error:#}").contains(expected),
            "expected {expected:?} in {error:#}"
        );
        assert_eq!(fs::read_dir(destination.path()).unwrap().count(), 0);
    }
}
