use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

pub const PROJECT_FILE: &str = ".ugrc";
pub const PROJECT_CONFIG_FILE: &str = "ug.toml";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSelector {
    pub path: PathBuf,
    pub selector: String,
}

/// Merged project settings from the ancestor `ug.toml` chain (child overrides parent).
///
/// Only keys present in at least one file are `Some`. Machine `config.json` fills
/// gaps when resolving effective policy.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectSettings {
    pub tolerate_exit_noise: Option<bool>,
    pub experimental_exit_noise_rules: Option<bool>,
    /// Paths of `ug.toml` files applied, root-first then closer parents.
    pub sources: Vec<PathBuf>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct ProjectConfigFile {
    #[serde(default, rename = "tolerate-exit-noise")]
    tolerate_exit_noise: Option<bool>,
    #[serde(default, rename = "experimental-exit-noise-rules")]
    experimental_exit_noise_rules: Option<bool>,
}

pub fn discover(start: &Path) -> Result<Option<ProjectSelector>> {
    let mut directory = start
        .canonicalize()
        .with_context(|| format!("resolve project directory {}", start.display()))?;
    loop {
        let path = directory.join(PROJECT_FILE);
        if path.is_file() {
            return Ok(Some(ProjectSelector {
                selector: read(&path)?,
                path,
            }));
        }
        if !directory.pop() {
            return Ok(None);
        }
    }
}

pub fn read(path: &Path) -> Result<String> {
    let contents = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let selector = contents.trim();
    if selector.is_empty() {
        bail!("{} is empty", path.display());
    }
    if selector.chars().any(char::is_whitespace) {
        bail!(
            "{} must contain one selector without whitespace",
            path.display()
        );
    }
    Ok(selector.to_owned())
}

pub fn pin(directory: &Path, selector: &str) -> Result<PathBuf> {
    let selector = selector.trim();
    if selector.is_empty() || selector.chars().any(char::is_whitespace) {
        bail!("project selector must be one non-empty value without whitespace");
    }
    let path = directory.join(PROJECT_FILE);
    crate::atomic::write_text(&path, &format!("{selector}\n"))?;
    Ok(path)
}

/// Load and merge every `ug.toml` from filesystem root ancestors of `start` down
/// to `start`. Closer files override farther ones for each key independently.
pub fn load_settings(start: &Path) -> Result<ProjectSettings> {
    let mut directory = start
        .canonicalize()
        .with_context(|| format!("resolve project directory {}", start.display()))?;

    let mut chain = Vec::new();
    loop {
        let path = directory.join(PROJECT_CONFIG_FILE);
        if path.is_file() {
            chain.push(path);
        } else if path.exists() {
            bail!("{} exists but is not a regular file", path.display());
        }
        if !directory.pop() {
            break;
        }
    }

    // chain is leaf → root; reverse so parents apply first.
    chain.reverse();

    let mut settings = ProjectSettings::default();
    for path in chain {
        let layer = read_config_file(&path)?;
        if let Some(value) = layer.tolerate_exit_noise {
            settings.tolerate_exit_noise = Some(value);
        }
        if let Some(value) = layer.experimental_exit_noise_rules {
            settings.experimental_exit_noise_rules = Some(value);
        }
        settings.sources.push(path);
    }
    Ok(settings)
}

fn read_config_file(path: &Path) -> Result<ProjectConfigFile> {
    let contents = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    toml::from_str(&contents).with_context(|| format!("parse {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_parent_project_file_and_pins_atomically() {
        let temp = tempfile::tempdir().unwrap();
        let child = temp.path().join("a/b");
        fs::create_dir_all(&child).unwrap();
        let path = pin(temp.path(), "4.7@mono").unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "4.7@mono\n");
        assert_eq!(discover(&child).unwrap().unwrap().selector, "4.7@mono");
    }

    #[test]
    fn rejects_ambiguous_file_content() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(PROJECT_FILE);
        fs::write(&path, "4.7\n4.8\n").unwrap();
        assert!(read(&path).is_err());
    }

    #[test]
    fn project_toml_child_overrides_parent_keys() {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("repo");
        let child = parent.join("game");
        fs::create_dir_all(&child).unwrap();
        fs::write(
            parent.join(PROJECT_CONFIG_FILE),
            "tolerate-exit-noise = true\nexperimental-exit-noise-rules = true\n",
        )
        .unwrap();
        fs::write(
            child.join(PROJECT_CONFIG_FILE),
            "tolerate-exit-noise = false\n",
        )
        .unwrap();

        let settings = load_settings(&child).unwrap();
        assert_eq!(settings.tolerate_exit_noise, Some(false));
        assert_eq!(settings.experimental_exit_noise_rules, Some(true));
        assert_eq!(settings.sources.len(), 2);
        assert_eq!(
            settings.sources[0].file_name().and_then(|n| n.to_str()),
            Some(PROJECT_CONFIG_FILE)
        );
        assert!(settings.sources[0].parent().unwrap().ends_with("repo"));
        assert!(settings.sources[1].parent().unwrap().ends_with("game"));
    }

    #[test]
    fn project_toml_missing_is_empty_settings() {
        let temp = tempfile::tempdir().unwrap();
        let settings = load_settings(temp.path()).unwrap();
        assert_eq!(settings, ProjectSettings::default());
    }

    #[test]
    fn project_toml_rejects_unknown_keys() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join(PROJECT_CONFIG_FILE),
            "not-a-real-key = true\n",
        )
        .unwrap();
        let err = load_settings(temp.path()).unwrap_err().to_string();
        assert!(
            err.contains("parse") || err.contains("unknown"),
            "unexpected error: {err}"
        );
    }
}
