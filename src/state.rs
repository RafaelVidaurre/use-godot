use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{Installation, atomic, paths::Paths};

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DurableStep {
    ActivationJournal,
    ActivationShim,
    ActivationState,
    ActivationJournalRemoval,
    UninstallJournal,
    UninstallRename,
    UninstallShim,
    UninstallState,
    UninstallTrash,
    UninstallJournalRemoval,
}

#[cfg(test)]
thread_local! {
    static FAIL_AFTER: std::cell::Cell<Option<DurableStep>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn fail_after(step: DurableStep) {
    FAIL_AFTER.with(|configured| configured.set(Some(step)));
}

#[cfg(test)]
fn observe_durable_step(step: DurableStep) -> Result<()> {
    FAIL_AFTER.with(|configured| {
        if configured.get() == Some(step) {
            configured.set(None);
            anyhow::bail!("injected failure after {step:?}");
        }
        Ok(())
    })
}

#[cfg(test)]
macro_rules! after_durable_step {
    ($step:expr) => {
        observe_durable_step($step)?
    };
}

#[cfg(not(test))]
macro_rules! after_durable_step {
    ($step:expr) => {};
}

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
    after_durable_step!(DurableStep::ActivationJournal);
    apply_activation(paths, state, installation, set_default)?;
    atomic::remove_file(&paths.pending())?;
    after_durable_step!(DurableStep::ActivationJournalRemoval);
    Ok(())
}

pub fn uninstall(paths: &Paths, state: &mut State, canonical: &str) -> Result<()> {
    atomic::write_json(
        &paths.pending(),
        &PendingOperation::Uninstall {
            canonical: canonical.to_owned(),
        },
    )?;
    after_durable_step!(DurableStep::UninstallJournal);
    apply_uninstall(paths, state, canonical)?;
    atomic::remove_file(&paths.pending())?;
    after_durable_step!(DurableStep::UninstallJournalRemoval);
    Ok(())
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
    after_durable_step!(DurableStep::ActivationShim);
    state.active = Some(canonical.clone());
    if set_default {
        state.default = Some(canonical);
    }
    state.save(paths)?;
    after_durable_step!(DurableStep::ActivationState);
    Ok(())
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
        after_durable_step!(DurableStep::UninstallRename);
    }
    if state.active.as_deref() == Some(canonical) {
        atomic::remove_symlink(&paths.shim())?;
        after_durable_step!(DurableStep::UninstallShim);
        state.active = None;
    }
    if state.default.as_deref() == Some(canonical) {
        state.default = None;
    }
    state.aliases.retain(|_, value| value != canonical);
    state.save(paths)?;
    after_durable_step!(DurableStep::UninstallState);
    if trash.exists() {
        fs::remove_dir_all(&trash)?;
        after_durable_step!(DurableStep::UninstallTrash);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Channel, Identity, Variant, model::InstallSource};
    use semver::Version;
    use tempfile::TempDir;

    #[test]
    fn activation_recovers_idempotently_after_every_durable_step() {
        for step in [
            DurableStep::ActivationJournal,
            DurableStep::ActivationShim,
            DurableStep::ActivationState,
            DurableStep::ActivationJournalRemoval,
        ] {
            let temp = TempDir::new().unwrap();
            let paths = test_paths(&temp);
            let old = installation(&paths, 4, 7, "old");
            let next = installation(&paths, 4, 8, "next");
            let old_canonical = old.identity.canonical();
            let next_canonical = next.identity.canonical();
            let mut state = State::default();
            activate(&paths, &mut state, &old, true).unwrap();
            state.aliases.insert("editor".into(), old_canonical.clone());
            state.save(&paths).unwrap();

            fail_after(step);
            let error = activate(&paths, &mut state, &next, true).unwrap_err();
            assert!(error.to_string().contains("injected failure"));
            let journal_was_removed = step == DurableStep::ActivationJournalRemoval;
            assert_eq!(paths.pending().is_file(), !journal_was_removed);

            let persisted = State::load(&paths).unwrap();
            if matches!(
                step,
                DurableStep::ActivationState | DurableStep::ActivationJournalRemoval
            ) {
                assert_eq!(persisted.active.as_deref(), Some(next_canonical.as_str()));
                assert_eq!(persisted.default.as_deref(), Some(next_canonical.as_str()));
            } else {
                assert_eq!(persisted.active.as_deref(), Some(old_canonical.as_str()));
                assert_eq!(persisted.default.as_deref(), Some(old_canonical.as_str()));
            }
            let expected_shim = if step == DurableStep::ActivationJournal {
                &old.binary
            } else {
                &next.binary
            };
            assert!(same_file::is_same_file(paths.shim(), expected_shim).unwrap());

            assert_eq!(recover_pending(&paths).unwrap(), !journal_was_removed);
            assert!(!recover_pending(&paths).unwrap());
            let recovered = State::load(&paths).unwrap();
            assert_eq!(recovered.active.as_deref(), Some(next_canonical.as_str()));
            assert_eq!(recovered.default.as_deref(), Some(next_canonical.as_str()));
            assert_eq!(
                recovered.aliases.get("editor").map(String::as_str),
                Some(old_canonical.as_str())
            );
            assert!(same_file::is_same_file(paths.shim(), &next.binary).unwrap());
            assert!(!paths.pending().exists());
        }
    }

    #[test]
    fn uninstall_recovers_idempotently_after_every_durable_step() {
        for step in [
            DurableStep::UninstallJournal,
            DurableStep::UninstallRename,
            DurableStep::UninstallShim,
            DurableStep::UninstallState,
            DurableStep::UninstallTrash,
            DurableStep::UninstallJournalRemoval,
        ] {
            let temp = TempDir::new().unwrap();
            let paths = test_paths(&temp);
            let target = installation(&paths, 4, 7, "target");
            let canonical = target.identity.canonical();
            let directory = paths.install_dir(&canonical);
            let trash = paths.versions().join(format!(".trash-{canonical}"));
            let mut state = State::default();
            activate(&paths, &mut state, &target, true).unwrap();
            state.aliases.insert("editor".into(), canonical.clone());
            state.save(&paths).unwrap();

            fail_after(step);
            let error = uninstall(&paths, &mut state, &canonical).unwrap_err();
            assert!(error.to_string().contains("injected failure"));
            let journal_was_removed = step == DurableStep::UninstallJournalRemoval;
            assert_eq!(paths.pending().is_file(), !journal_was_removed);

            let renamed = step != DurableStep::UninstallJournal;
            assert_eq!(directory.exists(), !renamed);
            assert_eq!(
                trash.exists(),
                matches!(
                    step,
                    DurableStep::UninstallRename
                        | DurableStep::UninstallShim
                        | DurableStep::UninstallState
                )
            );
            let state_was_saved = matches!(
                step,
                DurableStep::UninstallState
                    | DurableStep::UninstallTrash
                    | DurableStep::UninstallJournalRemoval
            );
            let persisted = State::load(&paths).unwrap();
            if state_was_saved {
                assert!(persisted.active.is_none());
                assert!(persisted.default.is_none());
                assert!(!persisted.aliases.contains_key("editor"));
            } else {
                assert_eq!(persisted.active.as_deref(), Some(canonical.as_str()));
                assert_eq!(persisted.default.as_deref(), Some(canonical.as_str()));
                assert_eq!(
                    persisted.aliases.get("editor").map(String::as_str),
                    Some(canonical.as_str())
                );
            }
            let shim_removed = matches!(
                step,
                DurableStep::UninstallShim
                    | DurableStep::UninstallState
                    | DurableStep::UninstallTrash
                    | DurableStep::UninstallJournalRemoval
            );
            assert_eq!(
                paths.shim().exists() || paths.shim().is_symlink(),
                !shim_removed
            );

            assert_eq!(recover_pending(&paths).unwrap(), !journal_was_removed);
            assert!(!recover_pending(&paths).unwrap());
            let recovered = State::load(&paths).unwrap();
            assert!(recovered.active.is_none());
            assert!(recovered.default.is_none());
            assert!(!recovered.aliases.contains_key("editor"));
            assert!(!directory.exists());
            assert!(!trash.exists());
            assert!(!paths.shim().exists());
            assert!(!paths.pending().exists());
        }
    }

    #[test]
    fn conflicting_uninstall_paths_fail_without_destroying_either_copy() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let target = installation(&paths, 4, 7, "target");
        let canonical = target.identity.canonical();
        let directory = paths.install_dir(&canonical);
        let trash = paths.versions().join(format!(".trash-{canonical}"));
        let mut state = State::default();
        activate(&paths, &mut state, &target, true).unwrap();

        fail_after(DurableStep::UninstallJournal);
        uninstall(&paths, &mut state, &canonical).unwrap_err();
        fs::create_dir_all(&trash).unwrap();
        fs::write(trash.join("conflict-marker"), "preserve").unwrap();

        let error = recover_pending(&paths).unwrap_err();
        assert!(error.to_string().contains("both"));
        assert!(directory.is_dir());
        assert_eq!(
            fs::read_to_string(trash.join("conflict-marker")).unwrap(),
            "preserve"
        );
        assert!(paths.pending().is_file());
        let preserved = State::load(&paths).unwrap();
        assert_eq!(preserved.active.as_deref(), Some(canonical.as_str()));
        assert_eq!(preserved.default.as_deref(), Some(canonical.as_str()));
        assert!(same_file::is_same_file(paths.shim(), target.binary).unwrap());
    }

    fn test_paths(temp: &TempDir) -> Paths {
        let paths = Paths {
            root: temp.path().join("managed"),
        };
        paths.ensure().unwrap();
        paths
    }

    fn installation(paths: &Paths, major: u64, minor: u64, name: &str) -> Installation {
        let identity = Identity::new(
            Version::new(major, minor, 0),
            Channel::Stable,
            Variant::Double,
            if cfg!(windows) { "windows" } else { "test" },
            "test",
        );
        let directory = paths.install_dir(&identity.canonical());
        fs::create_dir_all(&directory).unwrap();
        let binary = directory.join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_owned()
        });
        fs::write(&binary, name).unwrap();
        let installation = Installation {
            identity,
            binary,
            source: InstallSource::Imported {
                original_path: Path::new(name).into(),
            },
            installed_at_unix: 0,
            sha256: None,
        };
        write_manifest(&directory, &installation).unwrap();
        installation
    }
}
