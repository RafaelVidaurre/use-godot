use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

pub const PROJECT_FILE: &str = ".ugrc";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSelector {
    pub path: PathBuf,
    pub selector: String,
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
}
