use std::{
    fs,
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::{Channel, Variant, model::parse_release_tag, paths::Paths};

const DEFAULT_API: &str = "https://api.github.com/repos/godotengine/godot-builds";
const CACHE_MAX_AGE: Duration = Duration::from_secs(60 * 60);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
    pub digest: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub draft: bool,
    pub prerelease: bool,
    pub published_at: Option<String>,
    pub assets: Vec<Asset>,
}

impl Release {
    pub fn parsed(&self) -> Option<(Version, Channel)> {
        parse_release_tag(&self.tag_name).ok()
    }
}

#[derive(Clone, Debug)]
pub struct ReleaseCatalog {
    pub releases: Vec<Release>,
    client: Client,
    api_base: String,
}

impl ReleaseCatalog {
    pub fn fetch(paths: &Paths, refresh: bool, api_base: Option<&str>) -> Result<Self> {
        paths.ensure()?;
        let api_base = api_base
            .unwrap_or(DEFAULT_API)
            .trim_end_matches('/')
            .to_owned();
        let cache = paths.cache().join("releases.json");
        let client = Client::builder()
            .user_agent(format!("use-godot/{}", env!("CARGO_PKG_VERSION")))
            .build()?;
        if !refresh && api_base == DEFAULT_API && cache_is_fresh(&cache)? {
            let releases =
                serde_json::from_slice(&fs::read(&cache)?).context("parse cached releases")?;
            return Ok(Self {
                releases,
                client,
                api_base,
            });
        }

        let mut releases = Vec::new();
        for page in 1..=10 {
            let url = format!("{api_base}/releases?per_page=100&page={page}");
            let response = client
                .get(&url)
                .send()
                .with_context(|| format!("request {url}"))?
                .error_for_status()
                .with_context(|| format!("GitHub releases request failed: {url}"))?;
            let mut batch: Vec<Release> =
                response.json().context("parse GitHub releases response")?;
            let done = batch.len() < 100;
            releases.append(&mut batch);
            if done {
                break;
            }
        }
        releases.retain(|release| !release.draft && release.parsed().is_some());
        if releases.is_empty() {
            bail!("official release catalog was empty");
        }
        if api_base == DEFAULT_API {
            crate::atomic::write_json(&cache, &releases)?;
        }
        Ok(Self {
            releases,
            client,
            api_base,
        })
    }

    pub fn resolve(&self, selector: &str, include_prerelease: bool) -> Result<&Release> {
        let selector = selector.split('@').next().unwrap_or(selector);
        let requested_channel = if selector == "latest" || selector == "stable" {
            Some("stable")
        } else if selector == "rc" {
            Some("rc")
        } else if selector == "beta" {
            Some("beta")
        } else if selector == "dev" {
            Some("dev")
        } else if selector == "alpha" {
            Some("alpha")
        } else {
            None
        };
        let numeric = selector.split('-').next().unwrap_or(selector);
        let numeric_parts = if numeric == "latest"
            || matches!(numeric, "stable" | "rc" | "beta" | "dev" | "alpha")
        {
            Vec::new()
        } else {
            numeric
                .split('.')
                .map(str::parse::<u64>)
                .collect::<Result<Vec<_>, _>>()
                .with_context(|| format!("invalid version selector '{selector}'"))?
        };
        if numeric_parts.len() > 3 {
            bail!("invalid version selector '{selector}'");
        }
        let explicit_channel = selector.split_once('-').map(|(_, channel)| channel);

        let mut candidates: Vec<_> = self
            .releases
            .iter()
            .filter(|release| {
                let Some((version, channel)) = release.parsed() else {
                    return false;
                };
                if !include_prerelease && !channel.is_stable() {
                    return false;
                }
                let actual = [version.major, version.minor, version.patch];
                if numeric_parts.iter().zip(actual).any(|(a, b)| *a != b) {
                    return false;
                }
                let wanted = explicit_channel.or(requested_channel);
                match wanted {
                    Some("stable") => channel.is_stable(),
                    Some("rc") => matches!(channel, Channel::Rc(_)),
                    Some("beta") => matches!(channel, Channel::Beta(_)),
                    Some("dev") => matches!(channel, Channel::Dev(_)),
                    Some("alpha") => matches!(channel, Channel::Alpha(_)),
                    Some(exact) => channel.to_string() == exact,
                    None => channel.is_stable(),
                }
            })
            .collect();
        candidates.sort_by(|a, b| {
            let (av, ac) = a.parsed().unwrap();
            let (bv, bc) = b.parsed().unwrap();
            bv.cmp(&av)
                .then_with(|| bc.precedence().cmp(&ac.precedence()))
        });
        candidates
            .first()
            .copied()
            .with_context(|| format!("no official Godot release matches '{selector}'"))
    }

    pub fn asset_for<'a>(
        &self,
        release: &'a Release,
        variant: &Variant,
        platform: &str,
        arch: &str,
    ) -> Result<&'a Asset> {
        if !variant.is_official_download() {
            bail!(
                "{variant} is not published in the official Godot editor feed; use --from to import it"
            );
        }
        let tag = &release.tag_name;
        let macos_label = if release
            .parsed()
            .is_some_and(|(version, _)| version.major <= 3)
        {
            "osx"
        } else {
            "macos"
        };
        let expected = match (platform, variant) {
            ("macos", Variant::Standard) => {
                format!("Godot_v{tag}_{macos_label}.universal.zip")
            }
            ("macos", Variant::Mono) => {
                format!("Godot_v{tag}_mono_{macos_label}.universal.zip")
            }
            ("linux", Variant::Standard) => format!("Godot_v{tag}_linux.{arch}.zip"),
            ("linux", Variant::Mono) => format!("Godot_v{tag}_mono_linux_{arch}.zip"),
            ("windows", Variant::Standard) if arch == "x86_64" => {
                format!("Godot_v{tag}_win64.exe.zip")
            }
            ("windows", Variant::Standard) if arch == "x86_32" => {
                format!("Godot_v{tag}_win32.exe.zip")
            }
            ("windows", Variant::Standard) if arch == "arm64" => {
                format!("Godot_v{tag}_windows_arm64.exe.zip")
            }
            ("windows", Variant::Mono) if arch == "x86_64" => {
                format!("Godot_v{tag}_mono_win64.zip")
            }
            ("windows", Variant::Mono) if arch == "x86_32" => {
                format!("Godot_v{tag}_mono_win32.zip")
            }
            ("windows", Variant::Mono) if arch == "arm64" => {
                format!("Godot_v{tag}_mono_windows_arm64.zip")
            }
            _ => bail!("unsupported official target {platform}/{arch} for {variant}"),
        };
        release
            .assets
            .iter()
            .find(|asset| asset.name == expected)
            .with_context(|| {
                format!(
                    "official release {} has no expected asset {expected}",
                    release.tag_name
                )
            })
    }

    pub fn authoritative_digest(&self, release: &Release, asset: &Asset) -> Result<Digest> {
        if let Some(value) = asset.digest.as_deref() {
            if let Some(hex) = value.strip_prefix("sha256:") {
                validate_hex(hex, 64)?;
                return Ok(Digest::Sha256(hex.to_ascii_lowercase()));
            }
        }
        let sums = release.assets.iter().find(|item| item.name == "SHA512-SUMS.txt")
            .with_context(|| format!("{} provides neither a SHA-256 asset digest nor SHA512-SUMS.txt; refusing download", release.tag_name))?;
        let text = self
            .client
            .get(&sums.browser_download_url)
            .send()?
            .error_for_status()?
            .text()?;
        for line in text.lines() {
            let mut fields = line.split_whitespace();
            if let (Some(hash), Some(name)) = (fields.next(), fields.next()) {
                if name.trim_start_matches('*') == asset.name {
                    validate_hex(hash, 128)?;
                    return Ok(Digest::Sha512(hash.to_ascii_lowercase()));
                }
            }
        }
        bail!(
            "SHA512-SUMS.txt has no entry for {}; refusing download",
            asset.name
        )
    }

    pub fn client(&self) -> &Client {
        &self.client
    }
    pub fn api_base(&self) -> &str {
        &self.api_base
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Digest {
    Sha256(String),
    Sha512(String),
}

fn cache_is_fresh(path: &std::path::Path) -> Result<bool> {
    let modified = match fs::metadata(path).and_then(|m| m.modified()) {
        Ok(value) => value,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    Ok(SystemTime::now()
        .duration_since(modified)
        .unwrap_or(CACHE_MAX_AGE)
        < CACHE_MAX_AGE)
}

fn validate_hex(value: &str, length: usize) -> Result<()> {
    if value.len() != length || !value.bytes().all(|c| c.is_ascii_hexdigit()) {
        bail!("invalid authoritative digest");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, names: &[&str]) -> Release {
        Release {
            tag_name: tag.into(),
            draft: false,
            prerelease: false,
            published_at: None,
            assets: names
                .iter()
                .map(|name| Asset {
                    name: (*name).into(),
                    browser_download_url: "https://example.invalid".into(),
                    size: 1,
                    digest: None,
                })
                .collect(),
        }
    }

    fn catalog() -> ReleaseCatalog {
        ReleaseCatalog {
            releases: Vec::new(),
            client: Client::new(),
            api_base: "https://example.invalid".into(),
        }
    }

    #[test]
    fn mac_asset_names_cover_godot_three_and_four() {
        let three = release("3.6.2-stable", &["Godot_v3.6.2-stable_osx.universal.zip"]);
        let four = release(
            "4.7-stable",
            &["Godot_v4.7-stable_mono_macos.universal.zip"],
        );
        assert!(
            catalog()
                .asset_for(&three, &Variant::Standard, "macos", "arm64")
                .is_ok()
        );
        assert!(
            catalog()
                .asset_for(&four, &Variant::Mono, "macos", "arm64")
                .is_ok()
        );
    }
}
