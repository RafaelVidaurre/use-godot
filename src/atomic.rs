use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use fs2::FileExt;
use serde::Serialize;
use tempfile::NamedTempFile;

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
    let temp = parent.join(format!(
        ".{}.tmp-{}",
        link.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    if temp.exists() || temp.is_symlink() {
        fs::remove_file(&temp)?;
    }
    create_symlink(target, &temp)?;
    fs::rename(&temp, link).with_context(|| format!("atomically replace {}", link.display()))?;
    sync_dir(parent)
}

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

#[cfg(unix)]
fn create_symlink(target: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link)
        .with_context(|| format!("create symlink {}", link.display()))
}

#[cfg(windows)]
fn create_symlink(target: &Path, link: &Path) -> Result<()> {
    std::os::windows::fs::symlink_file(target, link)
        .with_context(|| format!("create symlink {}", link.display()))
}

fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)?
        .sync_all()
        .with_context(|| format!("sync {}", path.display()))
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
