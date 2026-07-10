use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct MigrationPlan {
    pub zshrc: PathBuf,
    pub legacy_script: PathBuf,
    pub legacy_link: PathBuf,
    pub ug_alias_lines: Vec<usize>,
    pub legacy_script_exists: bool,
    pub legacy_link_target: Option<PathBuf>,
    pub action: String,
}

pub fn plan(zshrc: &Path, legacy_script: &Path, legacy_link: &Path) -> Result<MigrationPlan> {
    let contents =
        fs::read_to_string(zshrc).with_context(|| format!("read {}", zshrc.display()))?;
    let ug_alias_lines = contents
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let trimmed = line.trim();
            (trimmed.starts_with("alias ug=") || trimmed.starts_with("alias ug ="))
                .then_some(index + 1)
        })
        .collect();
    let legacy_link_target = if legacy_link.is_symlink() {
        Some(fs::read_link(legacy_link)?)
    } else {
        None
    };
    Ok(MigrationPlan {
        zshrc: zshrc.to_owned(), legacy_script: legacy_script.to_owned(), legacy_link: legacy_link.to_owned(), ug_alias_lines,
        legacy_script_exists: legacy_script.is_file(), legacy_link_target,
        action: "replace only the ug alias with a marked shell-init block; preserve the legacy script, convenience aliases, /Applications, and legacy symlink".into(),
    })
}

pub fn apply(zshrc: &Path, ug_binary: &Path, yes: bool) -> Result<PathBuf> {
    if !yes {
        bail!("migration is dry-run by default; pass --yes after reviewing `ug migrate plan`");
    }
    if !ug_binary.is_absolute() || !ug_binary.is_file() {
        bail!("--ug-binary must be an existing absolute path");
    }
    let contents =
        fs::read_to_string(zshrc).with_context(|| format!("read {}", zshrc.display()))?;
    if contents.contains("# >>> use-godot >>>") {
        bail!(
            "{} already contains a use-godot integration block",
            zshrc.display()
        );
    }
    let escaped = shell_single_quote(&ug_binary.to_string_lossy());
    let block = format!(
        "# >>> use-godot >>>\n# Managed by `ug migrate apply`; the legacy script and symlink are intentionally preserved.\neval \"$({escaped} shell init zsh)\"\n# <<< use-godot <<<"
    );
    let mut replaced = false;
    let mut output = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if !replaced && (trimmed.starts_with("alias ug=") || trimmed.starts_with("alias ug =")) {
            output.push(block.clone());
            replaced = true;
        } else {
            output.push(line.to_owned());
        }
    }
    if !replaced {
        output.push(String::new());
        output.push(block);
    }
    let backup = next_backup_path(zshrc);
    fs::copy(zshrc, &backup)
        .with_context(|| format!("create migration backup {}", backup.display()))?;
    let joined = format!("{}\n", output.join("\n"));
    atomic_write_bytes(zshrc, joined.as_bytes())?;
    Ok(backup)
}

fn atomic_write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let parent = path.parent().context("zshrc has no parent")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    let permissions = fs::metadata(path)?.permissions();
    fs::set_permissions(temp.path(), permissions)?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

fn next_backup_path(path: &Path) -> PathBuf {
    for index in 0..1000 {
        let suffix = if index == 0 {
            ".ug-backup".into()
        } else {
            format!(".ug-backup.{index}")
        };
        let candidate = PathBuf::from(format!("{}{}", path.display(), suffix));
        if !candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from(format!(
        "{}.ug-backup.{}",
        path.display(),
        std::process::id()
    ))
}

pub fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migration_changes_only_ug_alias_and_keeps_backup() {
        let temp = tempfile::tempdir().unwrap();
        let zshrc = temp.path().join(".zshrc");
        let ug = temp.path().join("ug");
        fs::write(
            &zshrc,
            "alias ug=legacy\nalias ug4=legacy4\nexport KEEP=yes\n",
        )
        .unwrap();
        fs::write(&ug, "binary").unwrap();
        let backup = apply(&zshrc, &ug, true).unwrap();
        let now = fs::read_to_string(&zshrc).unwrap();
        assert!(now.contains("# >>> use-godot >>>"));
        assert!(now.contains("alias ug4=legacy4"));
        assert!(now.contains("export KEEP=yes"));
        assert_eq!(
            fs::read_to_string(backup).unwrap(),
            "alias ug=legacy\nalias ug4=legacy4\nexport KEEP=yes\n"
        );
    }

    #[test]
    fn apply_requires_explicit_confirmation() {
        let temp = tempfile::tempdir().unwrap();
        let zshrc = temp.path().join(".zshrc");
        let ug = temp.path().join("ug");
        fs::write(&zshrc, "alias ug=legacy\n").unwrap();
        fs::write(&ug, "binary").unwrap();
        assert!(apply(&zshrc, &ug, false).is_err());
        assert_eq!(fs::read_to_string(zshrc).unwrap(), "alias ug=legacy\n");
    }
}
