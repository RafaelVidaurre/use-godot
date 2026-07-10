use std::collections::HashSet;

use semver::Version;
use thiserror::Error;

use crate::{Installation, State, Variant, model::Channel};

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("no installed Godot matches '{0}'")]
    NotFound(String),
    #[error("alias cycle while resolving '{0}'")]
    AliasCycle(String),
    #[error("selector '{selector}' is ambiguous; use a canonical identity: {matches}")]
    Ambiguous { selector: String, matches: String },
    #[error("invalid selector '{0}'")]
    Invalid(String),
}

#[derive(Debug)]
struct Selector {
    version: Option<Vec<u64>>,
    channel: Option<ChannelFilter>,
    variant: Option<Variant>,
    canonical: Option<String>,
}

#[derive(Debug)]
enum ChannelFilter {
    Stable,
    Rc,
    Beta,
    Dev,
    Alpha,
    Exact(Channel),
    Any,
}

pub fn resolve_installed<'a>(
    input: &str,
    state: &State,
    installed: &'a [Installation],
) -> Result<&'a Installation, ResolveError> {
    let expanded = expand_alias(input, state)?;
    if let Some(found) = installed
        .iter()
        .find(|i| i.identity.canonical() == expanded)
    {
        return Ok(found);
    }
    let selector = parse_selector(&expanded)?;
    if let Some(canonical) = selector.canonical.as_ref() {
        return installed
            .iter()
            .find(|i| i.identity.canonical() == *canonical)
            .ok_or_else(|| ResolveError::NotFound(input.into()));
    }
    let mut candidates: Vec<_> = installed
        .iter()
        .filter(|install| matches_selector(&selector, install))
        .collect();
    candidates.sort_by(|a, b| {
        b.identity.version.cmp(&a.identity.version).then_with(|| {
            b.identity
                .channel
                .precedence()
                .cmp(&a.identity.channel.precedence())
        })
    });
    let Some(first) = candidates.first().copied() else {
        return Err(ResolveError::NotFound(input.into()));
    };
    let top_version = &first.identity.version;
    let top_channel = &first.identity.channel;
    let top_matches: Vec<_> = candidates
        .iter()
        .filter(|item| {
            &item.identity.version == top_version && &item.identity.channel == top_channel
        })
        .map(|item| item.identity.canonical())
        .collect();
    if top_matches.len() > 1 {
        return Err(ResolveError::Ambiguous {
            selector: input.into(),
            matches: top_matches.join(", "),
        });
    }
    Ok(first)
}

pub fn expand_alias(input: &str, state: &State) -> Result<String, ResolveError> {
    let mut value = input.to_owned();
    let mut seen = HashSet::new();
    while let Some(next) = state.aliases.get(&value) {
        if !seen.insert(value.clone()) {
            return Err(ResolveError::AliasCycle(input.into()));
        }
        value = next.clone();
    }
    Ok(value)
}

fn parse_selector(input: &str) -> Result<Selector, ResolveError> {
    if input.contains('+') && input.contains('@') {
        return Ok(Selector {
            version: None,
            channel: None,
            variant: None,
            canonical: Some(input.into()),
        });
    }
    let (base, variant) = match input.rsplit_once('@') {
        Some((base, variant)) => (
            base,
            Some(
                variant
                    .parse()
                    .map_err(|_| ResolveError::Invalid(input.into()))?,
            ),
        ),
        None => (input, None),
    };
    let (version_text, channel) = parse_base(base)?;
    let version = if version_text.is_empty() || version_text == "latest" {
        None
    } else {
        let parts = version_text
            .split('.')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ResolveError::Invalid(input.into()))?;
        if parts.is_empty() || parts.len() > 3 {
            return Err(ResolveError::Invalid(input.into()));
        }
        Some(parts)
    };
    Ok(Selector {
        version,
        channel: Some(channel),
        variant,
        canonical: None,
    })
}

fn parse_base(base: &str) -> Result<(&str, ChannelFilter), ResolveError> {
    for (suffix, filter) in [
        ("-stable", ChannelFilter::Stable),
        ("-rc", ChannelFilter::Rc),
        ("-beta", ChannelFilter::Beta),
        ("-dev", ChannelFilter::Dev),
        ("-alpha", ChannelFilter::Alpha),
    ] {
        if let Some(v) = base.strip_suffix(suffix) {
            return Ok((v, filter));
        }
    }
    if matches!(base, "stable" | "rc" | "beta" | "dev" | "alpha") {
        let f = match base {
            "stable" => ChannelFilter::Stable,
            "rc" => ChannelFilter::Rc,
            "beta" => ChannelFilter::Beta,
            "dev" => ChannelFilter::Dev,
            _ => ChannelFilter::Alpha,
        };
        return Ok(("", f));
    }
    if let Some((v, c)) = base.rsplit_once('-') {
        if let Ok(channel) = c.parse() {
            return Ok((v, ChannelFilter::Exact(channel)));
        }
    }
    Ok((base, ChannelFilter::Stable))
}

fn matches_selector(selector: &Selector, installation: &Installation) -> bool {
    let identity = &installation.identity;
    if let Some(parts) = &selector.version {
        let actual = [
            identity.version.major,
            identity.version.minor,
            identity.version.patch,
        ];
        if parts.iter().zip(actual).any(|(a, b)| *a != b) {
            return false;
        }
    }
    if let Some(variant) = &selector.variant {
        if &identity.variant != variant {
            return false;
        }
    }
    match selector.channel.as_ref().unwrap_or(&ChannelFilter::Any) {
        ChannelFilter::Stable => identity.channel.is_stable(),
        ChannelFilter::Rc => matches!(identity.channel, Channel::Rc(_)),
        ChannelFilter::Beta => matches!(identity.channel, Channel::Beta(_)),
        ChannelFilter::Dev => matches!(identity.channel, Channel::Dev(_)),
        ChannelFilter::Alpha => matches!(identity.channel, Channel::Alpha(_)),
        ChannelFilter::Exact(channel) => &identity.channel == channel,
        ChannelFilter::Any => true,
    }
}

pub fn version_from_selector(input: &str) -> Option<Version> {
    let input = input.split('@').next()?.split('-').next()?;
    let parts: Vec<_> = input.split('.').collect();
    if !(2..=3).contains(&parts.len()) {
        return None;
    }
    Some(Version::new(
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts.get(2).unwrap_or(&"0").parse().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Identity, model::InstallSource};
    use std::path::PathBuf;

    fn install(v: Version, variant: Variant) -> Installation {
        Installation {
            identity: Identity::new(v, Channel::Stable, variant, "macos", "universal"),
            binary: PathBuf::from("godot"),
            source: InstallSource::Imported {
                original_path: "x".into(),
            },
            installed_at_unix: 0,
            sha256: None,
        }
    }

    #[test]
    fn prefix_picks_latest_semantically() {
        let items = vec![
            install(Version::new(4, 9, 0), Variant::Standard),
            install(Version::new(4, 10, 0), Variant::Standard),
        ];
        assert_eq!(
            resolve_installed("4", &State::default(), &items)
                .unwrap()
                .identity
                .version,
            Version::new(4, 10, 0)
        );
    }

    #[test]
    fn variant_ambiguity_is_not_silently_resolved() {
        let items = vec![
            install(Version::new(4, 7, 0), Variant::Standard),
            install(Version::new(4, 7, 0), Variant::Double),
        ];
        assert!(matches!(
            resolve_installed("4.7", &State::default(), &items),
            Err(ResolveError::Ambiguous { .. })
        ));
        assert_eq!(
            resolve_installed("4.7@double", &State::default(), &items)
                .unwrap()
                .identity
                .variant,
            Variant::Double
        );
    }

    #[test]
    fn aliases_detect_cycles() {
        let mut state = State::default();
        state.aliases.insert("a".into(), "b".into());
        state.aliases.insert("b".into(), "a".into());
        assert!(matches!(
            expand_alias("a", &state),
            Err(ResolveError::AliasCycle(_))
        ));
    }

    #[test]
    fn target_ambiguity_requires_canonical_identity() {
        let first = install(Version::new(4, 7, 0), Variant::Standard);
        let mut second = first.clone();
        second.identity.platform = "linux".into();
        second.identity.arch = "arm64".into();
        let items = vec![first, second];
        assert!(matches!(
            resolve_installed("4.7@standard", &State::default(), &items),
            Err(ResolveError::Ambiguous { .. })
        ));
        let canonical = items[0].identity.canonical();
        assert_eq!(
            resolve_installed(&canonical, &State::default(), &items)
                .unwrap()
                .identity
                .canonical(),
            canonical
        );
    }
}
