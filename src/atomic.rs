use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use fs2::FileExt;
use serde::Serialize;
use tempfile::{Builder, NamedTempFile};

pub struct StateLock(File);

impl StateLock {
    pub fn acquire(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .with_context(|| format!("open lock {}", path.display()))?;
        file.lock_exclusive()
            .with_context(|| format!("lock {}", path.display()))?;
        Ok(Self(file))
    }
}

impl Drop for StateLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path.parent().context("atomic write path has no parent")?;
    fs::create_dir_all(parent)?;
    let mut temp = NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temp, value)?;
    temp.write_all(b"\n")?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    sync_dir(parent)
}

pub fn write_text(path: &Path, value: &str) -> Result<()> {
    let parent = path.parent().context("atomic write path has no parent")?;
    fs::create_dir_all(parent)?;
    let mut temp = NamedTempFile::new_in(parent)?;
    temp.write_all(value.as_bytes())?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    sync_dir(parent)
}

pub fn replace_symlink(target: &Path, link: &Path) -> Result<()> {
    let parent = link.parent().context("shim path has no parent")?;
    fs::create_dir_all(parent)?;
    let temporary = Builder::new()
        .prefix(".shim-")
        .tempfile_in(parent)?
        .into_temp_path();
    fs::remove_file(&temporary)?;
    create_symlink(target, &temporary)?;
    temporary
        .persist(link)
        .map_err(|error| error.error)
        .with_context(|| format!("atomically replace {}", link.display()))?;
    sync_dir(parent)
}

#[cfg(unix)]
pub fn remove_symlink(link: &Path) -> Result<()> {
    if link.is_symlink() {
        fs::remove_file(link)?;
        if let Some(parent) = link.parent() {
            sync_dir(parent)?;
        }
    } else if link.exists() {
        bail!("refusing to remove non-symlink {}", link.display());
    }
    Ok(())
}

#[cfg(windows)]
pub fn remove_symlink(link: &Path) -> Result<()> {
    if link.is_file() {
        fs::remove_file(link)?;
        if let Some(parent) = link.parent() {
            sync_dir(parent)?;
        }
    } else if link.exists() {
        bail!("refusing to remove non-file shim {}", link.display());
    }
    Ok(())
}

pub fn remove_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => {
            if let Some(parent) = path.parent() {
                sync_dir(parent)?;
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("remove {}", path.display())),
    }
}

#[cfg(unix)]
fn create_symlink(target: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link)
        .with_context(|| format!("create symlink {}", link.display()))
}

#[cfg(windows)]
fn create_symlink(target: &Path, link: &Path) -> Result<()> {
    // Managed installations and shims share a filesystem, so a hard link avoids
    // the elevated privilege Windows requires for symbolic links.
    fs::hard_link(target, link).with_context(|| format!("create hard link {}", link.display()))
}

#[cfg(unix)]
fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)?
        .sync_all()
        .with_context(|| format!("sync {}", path.display()))
}

#[cfg(windows)]
fn sync_dir(_path: &Path) -> Result<()> {
    // Windows does not expose directory handles through File::open. The file or
    // link itself is persisted before publication, and persist uses replace
    // semantics on this platform.
    Ok(())
}

pub fn atomic_dir_commit(staging: PathBuf, destination: &Path) -> Result<()> {
    if destination.exists() {
        bail!("destination already exists: {}", destination.display());
    }
    fs::rename(&staging, destination)
        .with_context(|| format!("commit installation to {}", destination.display()))?;
    if let Some(parent) = destination.parent() {
        sync_dir(parent)?;
    }
    Ok(())
}
