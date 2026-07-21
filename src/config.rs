use std::{env, fs};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::{atomic, paths::Paths, project::ProjectSettings};

/// User preferences under `$UG_ROOT/config.json` (not activation state).
///
/// Project `ug.toml` files may override these per directory (see
/// [`resolve_exit_noise_policy`]).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserConfig {
    /// When true, wrap Godot and apply exit-noise rules (default false).
    #[serde(default)]
    pub tolerate_exit_noise: bool,
    /// Allow experimental exit-noise rules (default false).
    #[serde(default)]
    pub experimental_exit_noise_rules: bool,
}

impl UserConfig {
    pub fn load(paths: &Paths) -> Result<Self> {
        match fs::read(paths.config()) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("parse config.json"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("read {}", paths.config().display())),
        }
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        paths.ensure()?;
        atomic::write_json(&paths.config(), self)
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

/// Precedence: CLI > env > project `ug.toml` chain > machine `config.json` > default (false).
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
    fn config_round_trip() {
        let dir = tempdir().unwrap();
        let paths = Paths {
            root: dir.path().to_path_buf(),
        };
        let mut config = UserConfig::default();
        assert!(!config.tolerate_exit_noise);
        config.tolerate_exit_noise = true;
        config.save(&paths).unwrap();
        let loaded = UserConfig::load(&paths).unwrap();
        assert!(loaded.tolerate_exit_noise);
    }

    #[test]
    fn unknown_config_key_errors() {
        assert!(config_key_to_field("not-a-key").is_err());
    }
}
