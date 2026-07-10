use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{Installation, atomic, paths::Paths};

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
        if installation.binary.is_relative() {
            installation.binary = entry.path().join(&installation.binary);
        }
        result.push(installation);
    }
    result.sort_by(|a, b| compare_installations(b, a));
    Ok(result)
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
