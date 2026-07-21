use std::{env, fs};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::{atomic, paths::Paths, project::ProjectSettings};

/// User preferences under `$UG_ROOT/ug.toml` (not activation state).
///
/// Project `ug.toml` files may override these per directory (see
/// [`resolve_exit_noise_policy`]). Legacy `$UG_ROOT/config.json` is still read
/// and migrated on load/save.
/// In-memory machine preferences. CLI `--json` uses snake_case field names;
/// on disk they are kebab-case in `ug.toml`.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct UserConfig {
    /// When true, wrap Godot and apply exit-noise rules (default false).
    pub tolerate_exit_noise: bool,
    /// Allow experimental exit-noise rules (default false).
    pub experimental_exit_noise_rules: bool,
}

/// On-disk `ug.toml` shape (machine or project). Machine uses concrete defaults;
/// project layers use [`crate::project`] with optional keys.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TomlConfigFile {
    #[serde(default, rename = "tolerate-exit-noise")]
    tolerate_exit_noise: bool,
    #[serde(default, rename = "experimental-exit-noise-rules")]
    experimental_exit_noise_rules: bool,
}

/// Legacy machine file written by early exit-noise builds.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
struct LegacyJsonConfig {
    #[serde(default)]
    tolerate_exit_noise: bool,
    #[serde(default)]
    experimental_exit_noise_rules: bool,
}

impl From<TomlConfigFile> for UserConfig {
    fn from(value: TomlConfigFile) -> Self {
        Self {
            tolerate_exit_noise: value.tolerate_exit_noise,
            experimental_exit_noise_rules: value.experimental_exit_noise_rules,
        }
    }
}

impl From<&UserConfig> for TomlConfigFile {
    fn from(value: &UserConfig) -> Self {
        Self {
            tolerate_exit_noise: value.tolerate_exit_noise,
            experimental_exit_noise_rules: value.experimental_exit_noise_rules,
        }
    }
}

impl From<LegacyJsonConfig> for UserConfig {
    fn from(value: LegacyJsonConfig) -> Self {
        Self {
            tolerate_exit_noise: value.tolerate_exit_noise,
            experimental_exit_noise_rules: value.experimental_exit_noise_rules,
        }
    }
}

impl UserConfig {
    pub fn load(paths: &Paths) -> Result<Self> {
        let config_path = paths.config();
        if config_path.is_file() {
            let config = Self::load_toml(&config_path)?;
            // Drop stale JSON left beside the new file.
            remove_legacy_config(paths);
            return Ok(config);
        }

        let legacy = paths.legacy_config();
        if legacy.is_file() {
            let config = Self::load_legacy_json(&legacy)?;
            // Eager migrate so `ug config path` and later loads use ug.toml only.
            config.save(paths)?;
            return Ok(config);
        }

        Ok(Self::default())
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        paths.ensure()?;
        let body = toml::to_string_pretty(&TomlConfigFile::from(self))
            .context("serialize machine ug.toml")?;
        atomic::write_text(&paths.config(), &body)?;
        remove_legacy_config(paths);
        Ok(())
    }

    fn load_toml(path: &std::path::Path) -> Result<Self> {
        let contents =
            fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let file: TomlConfigFile =
            toml::from_str(&contents).with_context(|| format!("parse {}", path.display()))?;
        Ok(file.into())
    }

    fn load_legacy_json(path: &std::path::Path) -> Result<Self> {
        let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        let file: LegacyJsonConfig =
            serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
        Ok(file.into())
    }
}

fn remove_legacy_config(paths: &Paths) {
    let legacy = paths.legacy_config();
    if legacy.is_file() {
        let _ = fs::remove_file(&legacy);
    }
}

/// Effective policy after CLI / env / config resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitNoisePolicy {
    pub tolerate: bool,
    pub allow_experimental_rules: bool,
    pub quiet: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CliPolicyOverrides {
    /// `Some` when `--tolerate-exit-noise` / `--no-tolerate-exit-noise` was passed.
    pub tolerate_exit_noise: Option<bool>,
}

pub fn parse_env_bool(raw: &str) -> Result<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        "" => bail!("empty boolean environment value"),
        other => bail!(
            "invalid boolean environment value '{other}' (use 1/0, true/false, yes/no, on/off)"
        ),
    }
}

fn env_bool(name: &str) -> Result<Option<bool>> {
    match env::var(name) {
        Ok(value) => Ok(Some(parse_env_bool(&value)?)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(e) => Err(e).with_context(|| format!("read {name}")),
    }
}

/// Precedence: CLI > env > project `ug.toml` chain > machine `$UG_ROOT/ug.toml` > default (false).
///
/// In the project chain, a closer `ug.toml` overrides the same key from a parent
/// directory. Omitted keys do not clear parent or machine values.
pub fn resolve_exit_noise_policy(
    cli: CliPolicyOverrides,
    machine: &UserConfig,
    project: &ProjectSettings,
    quiet: bool,
) -> Result<ExitNoisePolicy> {
    let tolerate = if let Some(v) = cli.tolerate_exit_noise {
        v
    } else if let Some(v) = env_bool("UG_TOLERATE_EXIT_NOISE")? {
        v
    } else if let Some(v) = project.tolerate_exit_noise {
        v
    } else {
        machine.tolerate_exit_noise
    };

    let allow_experimental = if let Some(v) = env_bool("UG_EXIT_NOISE_EXPERIMENTAL")? {
        v
    } else if let Some(v) = project.experimental_exit_noise_rules {
        v
    } else {
        machine.experimental_exit_noise_rules
    };

    Ok(ExitNoisePolicy {
        tolerate,
        allow_experimental_rules: allow_experimental,
        quiet,
    })
}

pub fn config_key_to_field(key: &str) -> Result<&'static str> {
    match key {
        "tolerate-exit-noise" => Ok("tolerate_exit_noise"),
        "experimental-exit-noise-rules" => Ok("experimental_exit_noise_rules"),
        _ => bail!(
            "unknown config key '{key}' (known: tolerate-exit-noise, experimental-exit-noise-rules)"
        ),
    }
}

pub fn set_config_bool(config: &mut UserConfig, key: &str, value: bool) -> Result<()> {
    match config_key_to_field(key)? {
        "tolerate_exit_noise" => config.tolerate_exit_noise = value,
        "experimental_exit_noise_rules" => config.experimental_exit_noise_rules = value,
        _ => unreachable!(),
    }
    Ok(())
}

pub fn parse_config_bool_value(raw: &str) -> Result<bool> {
    parse_env_bool(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn env_bool_parser_accepts_common_tokens() {
        for true_token in ["1", "true", "TRUE", "yes", "on"] {
            assert!(parse_env_bool(true_token).unwrap());
        }
        for false_token in ["0", "false", "no", "off"] {
            assert!(!parse_env_bool(false_token).unwrap());
        }
        assert!(parse_env_bool("").is_err());
        assert!(parse_env_bool("maybe").is_err());
    }

    #[test]
    fn resolve_prefers_cli_over_config() {
        let machine = UserConfig {
            tolerate_exit_noise: true,
            experimental_exit_noise_rules: false,
        };
        let policy = resolve_exit_noise_policy(
            CliPolicyOverrides {
                tolerate_exit_noise: Some(false),
            },
            &machine,
            &ProjectSettings::default(),
            false,
        )
        .unwrap();
        assert!(!policy.tolerate);
    }

    #[test]
    fn resolve_prefers_project_over_machine() {
        let machine = UserConfig {
            tolerate_exit_noise: false,
            experimental_exit_noise_rules: false,
        };
        let project = ProjectSettings {
            tolerate_exit_noise: Some(true),
            experimental_exit_noise_rules: None,
            sources: Vec::new(),
        };
        let policy =
            resolve_exit_noise_policy(CliPolicyOverrides::default(), &machine, &project, false)
                .unwrap();
        assert!(policy.tolerate);
        assert!(!policy.allow_experimental_rules);
    }

    #[test]
    fn resolve_project_none_falls_back_to_machine() {
        let machine = UserConfig {
            tolerate_exit_noise: true,
            experimental_exit_noise_rules: true,
        };
        let policy = resolve_exit_noise_policy(
            CliPolicyOverrides::default(),
            &machine,
            &ProjectSettings::default(),
            false,
        )
        .unwrap();
        assert!(policy.tolerate);
        assert!(policy.allow_experimental_rules);
    }

    #[test]
    fn config_round_trip_writes_ug_toml() {
        let dir = tempdir().unwrap();
        let paths = Paths {
            root: dir.path().to_path_buf(),
        };
        let mut config = UserConfig::default();
        assert!(!config.tolerate_exit_noise);
        config.tolerate_exit_noise = true;
        config.save(&paths).unwrap();
        assert!(paths.config().is_file());
        assert!(!paths.legacy_config().exists());
        let body = fs::read_to_string(paths.config()).unwrap();
        assert!(body.contains("tolerate-exit-noise = true"));
        let loaded = UserConfig::load(&paths).unwrap();
        assert!(loaded.tolerate_exit_noise);
    }

    #[test]
    fn migrates_legacy_config_json_to_ug_toml() {
        let dir = tempdir().unwrap();
        let paths = Paths {
            root: dir.path().to_path_buf(),
        };
        fs::write(
            paths.legacy_config(),
            r#"{"tolerate_exit_noise":true,"experimental_exit_noise_rules":true}"#,
        )
        .unwrap();

        let loaded = UserConfig::load(&paths).unwrap();
        assert!(loaded.tolerate_exit_noise);
        assert!(loaded.experimental_exit_noise_rules);
        assert!(paths.config().is_file());
        assert!(!paths.legacy_config().exists());

        let body = fs::read_to_string(paths.config()).unwrap();
        assert!(body.contains("tolerate-exit-noise = true"));
        assert!(body.contains("experimental-exit-noise-rules = true"));
    }

    #[test]
    fn unknown_config_key_errors() {
        assert!(config_key_to_field("not-a-key").is_err());
    }
}
