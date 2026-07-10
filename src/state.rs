use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{Installation, atomic, paths::Paths};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case")]
enum PendingOperation {
    Activate {
        canonical: String,
        set_default: bool,
    },
    Uninstall {
        canonical: String,
    },
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
    pub default: Option<String>,
    pub active: Option<String>,
}

impl State {
    pub fn load(paths: &Paths) -> Result<Self> {
        match fs::read(paths.state()) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("parse state.json"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("read {}", paths.state().display())),
        }
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        atomic::write_json(&paths.state(), self)
    }
}

pub fn activate(
    paths: &Paths,
    state: &mut State,
    installation: &Installation,
    set_default: bool,
) -> Result<()> {
    let canonical = installation.identity.canonical();
    let pending = PendingOperation::Activate {
        canonical: canonical.clone(),
        set_default,
    };
    atomic::write_json(&paths.pending(), &pending)?;
    apply_activation(paths, state, installation, set_default)?;
    atomic::remove_file(&paths.pending())
}

pub fn uninstall(paths: &Paths, state: &mut State, canonical: &str) -> Result<()> {
    atomic::write_json(
        &paths.pending(),
        &PendingOperation::Uninstall {
            canonical: canonical.to_owned(),
        },
    )?;
    apply_uninstall(paths, state, canonical)?;
    atomic::remove_file(&paths.pending())
}

pub fn recover_pending(paths: &Paths) -> Result<bool> {
    let bytes = match fs::read(paths.pending()) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let pending: PendingOperation = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse {}", paths.pending().display()))?;
    let mut state = State::load(paths)?;
    match pending {
        PendingOperation::Activate {
            canonical,
            set_default,
        } => {
            let installations = load_installations(paths)?;
            let installation = installations
                .iter()
                .find(|item| item.identity.canonical() == canonical)
                .with_context(|| format!("recover activation for missing {canonical}"))?;
            apply_activation(paths, &mut state, installation, set_default)?;
        }
        PendingOperation::Uninstall { canonical } => {
            apply_uninstall(paths, &mut state, &canonical)?;
        }
    }
    atomic::remove_file(&paths.pending())?;
    Ok(true)
}

fn apply_activation(
    paths: &Paths,
    state: &mut State,
    installation: &Installation,
    set_default: bool,
) -> Result<()> {
    let canonical = installation.identity.canonical();
    atomic::replace_symlink(&installation.binary, &paths.shim())?;
    state.active = Some(canonical.clone());
    if set_default {
        state.default = Some(canonical);
    }
    state.save(paths)
}

fn apply_uninstall(paths: &Paths, state: &mut State, canonical: &str) -> Result<()> {
    let directory = paths.install_dir(canonical);
    let trash = paths.versions().join(format!(".trash-{canonical}"));
    if directory.exists() {
        if trash.exists() {
            anyhow::bail!(
                "cannot recover uninstall: both {} and {} exist",
                directory.display(),
                trash.display()
            );
        }
        fs::rename(&directory, &trash)
            .with_context(|| format!("stage uninstall of {canonical}"))?;
    }
    if state.active.as_deref() == Some(canonical) {
        atomic::remove_symlink(&paths.shim())?;
        state.active = None;
    }
    if state.default.as_deref() == Some(canonical) {
        state.default = None;
    }
    state.aliases.retain(|_, value| value != canonical);
    state.save(paths)?;
    if trash.exists() {
        fs::remove_dir_all(&trash)?;
    }
    Ok(())
}

pub fn load_installations(paths: &Paths) -> Result<Vec<Installation>> {
    let mut result = Vec::new();
    let entries = match fs::read_dir(paths.versions()) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(result),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() || entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let manifest = entry.path().join("manifest.json");
        let bytes = fs::read(&manifest).with_context(|| format!("read {}", manifest.display()))?;
        let mut installation: Installation = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse {}", manifest.display()))?;
        let directory_name = entry.file_name().to_string_lossy().into_owned();
        if installation.identity.canonical() != directory_name {
            anyhow::bail!(
                "manifest identity {} does not match installation directory {directory_name}",
                installation.identity.canonical()
            );
        }
        validate_relative_binary(&installation.binary, &manifest)?;
        installation.binary = entry.path().join(&installation.binary);
        if installation.binary.exists() {
            if !installation.binary.is_file() {
                anyhow::bail!(
                    "managed binary is not a file: {}",
                    installation.binary.display()
                );
            }
            let install_root = entry.path().canonicalize()?;
            let resolved = installation.binary.canonicalize().with_context(|| {
                format!("resolve managed binary {}", installation.binary.display())
            })?;
            if !resolved.starts_with(&install_root) {
                anyhow::bail!(
                    "managed binary escapes installation {}: {}",
                    installation.identity.display_short(),
                    installation.binary.display()
                );
            }
            installation.binary = resolved;
        }
        result.push(installation);
    }
    result.sort_by(|a, b| compare_installations(b, a));
    Ok(result)
}

fn validate_relative_binary(binary: &Path, manifest: &Path) -> Result<()> {
    if !binary
        .components()
        .any(|component| matches!(component, Component::Normal(_)))
        || binary.is_absolute()
        || binary.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        anyhow::bail!(
            "manifest {} contains unsafe binary path {}",
            manifest.display(),
            binary.display()
        );
    }
    Ok(())
}

pub fn write_manifest(staging: &Path, installation: &Installation) -> Result<()> {
    let mut stored = installation.clone();
    if let Ok(relative) = stored.binary.strip_prefix(staging) {
        stored.binary = relative.to_owned();
    }
    atomic::write_json(&staging.join("manifest.json"), &stored)
}

fn compare_installations(a: &Installation, b: &Installation) -> std::cmp::Ordering {
    a.identity
        .version
        .cmp(&b.identity.version)
        .then_with(|| {
            a.identity
                .channel
                .precedence()
                .cmp(&b.identity.channel.precedence())
        })
        .then_with(|| a.identity.variant.slug().cmp(&b.identity.variant.slug()))
}
