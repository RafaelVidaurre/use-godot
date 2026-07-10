use std::{fmt, path::PathBuf, str::FromStr};

use anyhow::{Context, Result, bail};
use semver::Version;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Channel {
    Stable,
    Rc(u64),
    Beta(u64),
    Dev(u64),
    Alpha(u64),
}

impl Channel {
    pub fn is_stable(&self) -> bool {
        matches!(self, Self::Stable)
    }

    pub fn precedence(&self) -> (u8, u64) {
        match self {
            Self::Stable => (5, 0),
            Self::Rc(n) => (4, *n),
            Self::Beta(n) => (3, *n),
            Self::Alpha(n) => (2, *n),
            Self::Dev(n) => (1, *n),
        }
    }
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stable => write!(f, "stable"),
            Self::Rc(n) => write!(f, "rc{n}"),
            Self::Beta(n) => write!(f, "beta{n}"),
            Self::Dev(n) => write!(f, "dev{n}"),
            Self::Alpha(n) => write!(f, "alpha{n}"),
        }
    }
}

impl FromStr for Channel {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        if value == "stable" {
            return Ok(Self::Stable);
        }
        for (prefix, ctor) in [
            ("rc", Self::Rc as fn(u64) -> Self),
            ("beta", Self::Beta),
            ("dev", Self::Dev),
            ("alpha", Self::Alpha),
        ] {
            if let Some(number) = value.strip_prefix(prefix) {
                return Ok(ctor(
                    number
                        .parse()
                        .with_context(|| format!("invalid channel {value}"))?,
                ));
            }
        }
        bail!("unsupported channel '{value}' (expected stable, rcN, betaN, devN, or alphaN)")
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Variant {
    Standard,
    Mono,
    Double,
    GodotJs,
    Custom(String),
}

impl Variant {
    pub fn slug(&self) -> String {
        match self {
            Self::Standard => "standard".into(),
            Self::Mono => "mono".into(),
            Self::Double => "double".into(),
            Self::GodotJs => "godotjs".into(),
            Self::Custom(name) => format!("custom-{name}"),
        }
    }

    pub fn is_official_download(&self) -> bool {
        matches!(self, Self::Standard | Self::Mono)
    }
}

impl fmt::Display for Variant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Standard => write!(f, "standard"),
            Self::Mono => write!(f, "mono"),
            Self::Double => write!(f, "double"),
            Self::GodotJs => write!(f, "godotjs"),
            Self::Custom(name) => write!(f, "custom:{name}"),
        }
    }
}

impl FromStr for Variant {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "standard" | "std" => Ok(Self::Standard),
            "mono" | "dotnet" | ".net" => Ok(Self::Mono),
            "double" | "double-precision" => Ok(Self::Double),
            "godotjs" | "js" => Ok(Self::GodotJs),
            v if v.starts_with("custom:") => {
                let name = &value[7..];
                validate_component(name, "custom variant")?;
                Ok(Self::Custom(name.to_owned()))
            }
            _ => bail!("unknown variant '{value}'"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Identity {
    pub version: Version,
    pub channel: Channel,
    pub variant: Variant,
    pub platform: String,
    pub arch: String,
}

impl Identity {
    pub fn new(
        version: Version,
        channel: Channel,
        variant: Variant,
        platform: impl Into<String>,
        arch: impl Into<String>,
    ) -> Self {
        Self {
            version,
            channel,
            variant,
            platform: platform.into(),
            arch: arch.into(),
        }
    }

    pub fn canonical(&self) -> String {
        format!(
            "{}-{}@{}+{}-{}",
            self.version,
            self.channel,
            self.variant.slug(),
            self.platform,
            self.arch
        )
    }

    pub fn display_short(&self) -> String {
        format!("{}-{}@{}", self.version, self.channel, self.variant)
    }

    pub fn release_tag(&self) -> String {
        let base = if self.version.patch == 0 {
            format!("{}.{}", self.version.major, self.version.minor)
        } else {
            self.version.to_string()
        };
        format!("{base}-{}", self.channel)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Installation {
    pub identity: Identity,
    pub binary: PathBuf,
    pub source: InstallSource,
    pub installed_at_unix: u64,
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum InstallSource {
    Official { url: String, asset: String },
    Imported { original_path: PathBuf },
}

pub fn validate_component(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        bail!("invalid {label} '{value}': use 1-64 letters, digits, '.', '-', or '_'")
    }
    Ok(())
}

pub fn parse_release_tag(tag: &str) -> Result<(Version, Channel)> {
    let (version, channel) = tag
        .split_once('-')
        .with_context(|| format!("invalid Godot release tag '{tag}'"))?;
    let mut parts = version.split('.');
    let major: u64 = parts.next().context("missing major")?.parse()?;
    let minor: u64 = parts.next().context("missing minor")?.parse()?;
    let patch: u64 = parts.next().unwrap_or("0").parse()?;
    if parts.next().is_some() {
        bail!("invalid Godot version '{version}'");
    }
    Ok((Version::new(major, minor, patch), channel.parse()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_tags_preserve_channels_and_implicit_patch() {
        assert_eq!(
            parse_release_tag("4.7-stable").unwrap(),
            (Version::new(4, 7, 0), Channel::Stable)
        );
        assert_eq!(
            parse_release_tag("4.7.1-rc2").unwrap(),
            (Version::new(4, 7, 1), Channel::Rc(2))
        );
    }

    #[test]
    fn variants_are_distinct_identity() {
        let a = Identity::new(
            Version::new(4, 7, 0),
            Channel::Stable,
            Variant::Standard,
            "macos",
            "universal",
        );
        let b = Identity {
            variant: Variant::Double,
            ..a.clone()
        };
        assert_ne!(a.canonical(), b.canonical());
    }
}
