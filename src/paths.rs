use std::{
    env,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result};

#[derive(Clone, Debug)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn discover(explicit: Option<PathBuf>) -> Result<Self> {
        if let Some(root) = explicit.or_else(|| env::var_os("UG_ROOT").map(PathBuf::from)) {
            return Ok(Self {
                root: absolute_path(root)?,
            });
        }
        let home = env::var_os("HOME").context("HOME is not set; pass --root or set UG_ROOT")?;
        Ok(Self {
            root: absolute_path(PathBuf::from(home).join(".local/share/use-godot"))?,
        })
    }

    pub fn versions(&self) -> PathBuf {
        self.root.join("versions")
    }
    pub fn state(&self) -> PathBuf {
        self.root.join("state.json")
    }
    pub fn lock(&self) -> PathBuf {
        self.root.join("state.lock")
    }
    pub fn pending(&self) -> PathBuf {
        self.root.join("pending-operation.json")
    }
    pub fn shims(&self) -> PathBuf {
        self.root.join("shims")
    }
    pub fn shim(&self) -> PathBuf {
        self.shims()
            .join(if cfg!(windows) { "godot.exe" } else { "godot" })
    }
    pub fn cache(&self) -> PathBuf {
        self.root.join("cache")
    }
    pub fn downloads(&self) -> PathBuf {
        self.root.join("downloads")
    }
    pub fn install_dir(&self, canonical: &str) -> PathBuf {
        self.versions().join(canonical)
    }
    pub fn manifest(&self, canonical: &str) -> PathBuf {
        self.install_dir(canonical).join("manifest.json")
    }

    pub fn ensure(&self) -> Result<()> {
        for path in [
            &self.root,
            &self.versions(),
            &self.shims(),
            &self.cache(),
            &self.downloads(),
        ] {
            std::fs::create_dir_all(path).with_context(|| format!("create {}", path.display()))?;
        }
        Ok(())
    }

    pub fn is_inside_root(&self, path: &Path) -> bool {
        path.starts_with(&self.root)
    }
}

fn absolute_path(path: PathBuf) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .context("read current directory")?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(
                    normalized.components().next_back(),
                    Some(Component::Normal(_))
                ) {
                    normalized.pop();
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_relative_root_becomes_absolute() {
        let paths = Paths::discover(Some(PathBuf::from("build/../managed"))).unwrap();
        assert!(paths.root.is_absolute());
        assert!(paths.root.ends_with("managed"));
        assert!(!paths.root.to_string_lossy().contains(".."));
    }
}
