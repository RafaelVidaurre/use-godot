use std::{collections::BTreeMap, fs, path::PathBuf};

use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use semver::Version;
use tempfile::tempdir;
use use_godot::{
    Channel, Identity, Installation, Paths, ResolveError, State, Variant,
    model::{InstallSource, parse_release_tag, validate_component},
    resolve::{expand_alias, resolve_installed},
    state,
};

fn channel_strategy() -> impl Strategy<Value = Channel> {
    prop_oneof![
        Just(Channel::Stable),
        (0_u64..1000).prop_map(Channel::Rc),
        (0_u64..1000).prop_map(Channel::Beta),
        (0_u64..1000).prop_map(Channel::Alpha),
        (0_u64..1000).prop_map(Channel::Dev),
    ]
}

fn component_strategy() -> impl Strategy<Value = String> {
    "[A-Za-z0-9_.-]{1,16}"
}

fn variant_strategy() -> impl Strategy<Value = Variant> {
    prop_oneof![
        Just(Variant::Standard),
        Just(Variant::Mono),
        Just(Variant::Double),
        Just(Variant::GodotJs),
        component_strategy().prop_map(Variant::Custom),
    ]
}

fn version_strategy() -> impl Strategy<Value = Version> {
    (0_u64..8, 0_u64..20, 0_u64..20)
        .prop_map(|(major, minor, patch)| Version::new(major, minor, patch))
}

fn supported_platform_strategy() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("macos"), Just("linux"), Just("windows")]
}

fn supported_arch_strategy() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("arm64"),
        Just("x86_64"),
        Just("x86_32"),
        Just("universal"),
    ]
}

fn installation(version: Version, channel: Channel, variant: Variant) -> Installation {
    Installation {
        identity: Identity::new(version, channel, variant, "macos", "arm64"),
        binary: PathBuf::from("godot"),
        source: InstallSource::Imported {
            original_path: PathBuf::from("fixture"),
        },
        installed_at_unix: 0,
        sha256: None,
    }
}

fn property_config(cases: u32, max_shrink_iters: u32) -> ProptestConfig {
    ProptestConfig {
        cases,
        max_shrink_iters,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/properties.txt",
        ))),
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(property_config(64, 4096))]

    #[test]
    fn channels_and_variants_round_trip_through_display(
        channel in channel_strategy(),
        variant in variant_strategy(),
    ) {
        prop_assert_eq!(channel.to_string().parse::<Channel>().unwrap(), channel);
        prop_assert_eq!(variant.to_string().parse::<Variant>().unwrap(), variant);
    }

    #[test]
    fn release_tags_round_trip_version_and_channel(
        version in version_strategy(),
        channel in channel_strategy(),
    ) {
        let identity = Identity::new(
            version.clone(),
            channel.clone(),
            Variant::Standard,
            "macos",
            "arm64",
        );
        prop_assert_eq!(parse_release_tag(&identity.release_tag()).unwrap(), (version, channel));
    }

    #[test]
    fn canonical_identity_is_unique_across_supported_dimensions(
        version_a in version_strategy(),
        channel_a in channel_strategy(),
        variant_a in variant_strategy(),
        platform_a in supported_platform_strategy(),
        arch_a in supported_arch_strategy(),
        version_b in version_strategy(),
        channel_b in channel_strategy(),
        variant_b in variant_strategy(),
        platform_b in supported_platform_strategy(),
        arch_b in supported_arch_strategy(),
    ) {
        let a = Identity::new(
            version_a,
            channel_a,
            variant_a,
            platform_a,
            arch_a,
        );
        let b = Identity::new(
            version_b,
            channel_b,
            variant_b,
            platform_b,
            arch_b,
        );
        if a != b {
            prop_assert_ne!(a.canonical(), b.canonical());
        }
    }

    #[test]
    fn target_component_delimiters_cannot_alias_distinct_identities(
        left in "[A-Za-z0-9_]{1,8}",
        middle in "[A-Za-z0-9_]{1,8}",
        right in "[A-Za-z0-9_]{1,8}",
    ) {
        let a = Identity::new(
            Version::new(4, 7, 0),
            Channel::Stable,
            Variant::Standard,
            &left,
            format!("{middle}-{right}"),
        );
        let b = Identity::new(
            Version::new(4, 7, 0),
            Channel::Stable,
            Variant::Standard,
            format!("{left}-{middle}"),
            &right,
        );
        prop_assert_ne!(&a, &b);
        prop_assert_ne!(a.canonical(), b.canonical());
    }

    #[test]
    fn component_validation_accepts_documented_alphabet(value in component_strategy()) {
        prop_assert!(validate_component(&value, "component").is_ok());
    }

    #[test]
    fn component_validation_rejects_invalid_values(
        prefix in "[A-Za-z0-9_.-]{0,8}",
        invalid in prop_oneof![Just("/"), Just("+"), Just("@"), Just(" "), Just("\n")],
        suffix in "[A-Za-z0-9_.-]{0,8}",
    ) {
        let value = format!("{prefix}{invalid}{suffix}");
        prop_assert!(validate_component(&value, "component").is_err());
        prop_assert!(validate_component("", "component").is_err());
        prop_assert!(validate_component(&"a".repeat(65), "component").is_err());
    }

    #[test]
    fn version_prefix_resolution_selects_the_semantic_maximum(
        versions in prop::collection::btree_set((0_u64..20, 0_u64..20), 1..16),
    ) {
        let installed: Vec<_> = versions
            .iter()
            .map(|(minor, patch)| {
                installation(
                    Version::new(4, *minor, *patch),
                    Channel::Stable,
                    Variant::Standard,
                )
            })
            .collect();
        let expected = versions
            .iter()
            .map(|(minor, patch)| Version::new(4, *minor, *patch))
            .max()
            .unwrap();
        let selected = resolve_installed("4", &State::default(), &installed).unwrap();
        prop_assert_eq!(&selected.identity.version, &expected);
        prop_assert_eq!(
            resolve_installed(&selected.identity.canonical(), &State::default(), &installed)
                .unwrap()
                .identity
                .canonical(),
            selected.identity.canonical(),
        );
    }

    #[test]
    fn variant_ambiguity_requires_an_explicit_variant(
        version in version_strategy(),
        first in variant_strategy(),
        second in variant_strategy(),
    ) {
        prop_assume!(first != second);
        let installed = vec![
            installation(version.clone(), Channel::Stable, first.clone()),
            installation(version.clone(), Channel::Stable, second.clone()),
        ];
        let selector = version.to_string();
        let is_ambiguous = matches!(
            resolve_installed(&selector, &State::default(), &installed),
            Err(ResolveError::Ambiguous { .. })
        );
        prop_assert!(is_ambiguous);
        let explicit = format!("{selector}@{first}");
        prop_assert_eq!(
            &resolve_installed(&explicit, &State::default(), &installed)
                .unwrap()
                .identity
                .variant,
            &first,
        );
    }

    #[test]
    fn alias_chains_resolve_and_cycles_fail_closed(length in 1_usize..12) {
        let installed = vec![installation(
            Version::new(4, 7, 0),
            Channel::Stable,
            Variant::Standard,
        )];
        let canonical = installed[0].identity.canonical();
        let mut chain = State::default();
        for index in 0..length {
            let next = if index + 1 == length {
                canonical.clone()
            } else {
                format!("alias-{}", index + 1)
            };
            chain.aliases.insert(format!("alias-{index}"), next);
        }
        prop_assert_eq!(expand_alias("alias-0", &chain).unwrap(), canonical);
        prop_assert_eq!(
            resolve_installed("alias-0", &chain, &installed)
                .unwrap()
                .identity
                .canonical(),
            installed[0].identity.canonical(),
        );

        let mut cycle = State::default();
        for index in 0..length {
            cycle.aliases.insert(
                format!("cycle-{index}"),
                format!("cycle-{}", (index + 1) % length),
            );
        }
        prop_assert!(matches!(
            expand_alias("cycle-0", &cycle),
            Err(ResolveError::AliasCycle(_))
        ));
    }
}

#[derive(Clone, Debug)]
enum StateOperation {
    Activate { index: usize, set_default: bool },
    Alias { name: usize, index: usize },
    Uninstall { index: usize },
}

fn state_operation_strategy() -> impl Strategy<Value = StateOperation> {
    prop_oneof![
        (0_usize..3, any::<bool>())
            .prop_map(|(index, set_default)| { StateOperation::Activate { index, set_default } }),
        (0_usize..5, 0_usize..3).prop_map(|(name, index)| StateOperation::Alias { name, index }),
        (0_usize..3).prop_map(|index| StateOperation::Uninstall { index }),
    ]
}

#[derive(Default)]
struct StateModel {
    aliases: BTreeMap<String, String>,
    default: Option<String>,
    active: Option<String>,
    installed: [bool; 3],
}

fn assert_state_matches(actual: &State, expected: &StateModel) {
    assert_eq!(actual.aliases, expected.aliases);
    assert_eq!(actual.default, expected.default);
    assert_eq!(actual.active, expected.active);
}

proptest! {
    #![proptest_config(property_config(24, 512))]

    #[test]
    fn state_transitions_match_a_small_reference_model(
        operations in prop::collection::vec(state_operation_strategy(), 1..20),
    ) {
        let temporary = tempdir().unwrap();
        let paths = Paths { root: temporary.path().join("managed") };
        paths.ensure().unwrap();
        let installations: Vec<_> = [Variant::Standard, Variant::Double, Variant::GodotJs]
            .into_iter()
            .enumerate()
            .map(|(index, variant)| {
                let identity = Identity::new(
                    Version::new(4, 7, index as u64),
                    Channel::Stable,
                    variant,
                    "macos",
                    "arm64",
                );
                let directory = paths.install_dir(&identity.canonical());
                fs::create_dir_all(&directory).unwrap();
                let binary = directory.join("godot");
                fs::write(&binary, b"fixture").unwrap();
                Installation {
                    identity,
                    binary,
                    source: InstallSource::Imported {
                        original_path: PathBuf::from("fixture"),
                    },
                    installed_at_unix: 0,
                    sha256: None,
                }
            })
            .collect();
        let mut actual = State::default();
        let mut expected = StateModel {
            installed: [true; 3],
            ..StateModel::default()
        };

        for operation in operations {
            match operation {
                StateOperation::Activate { index, set_default } if expected.installed[index] => {
                    state::activate(&paths, &mut actual, &installations[index], set_default).unwrap();
                    let canonical = installations[index].identity.canonical();
                    expected.active = Some(canonical.clone());
                    if set_default {
                        expected.default = Some(canonical);
                    }
                }
                StateOperation::Alias { name, index } if expected.installed[index] => {
                    let name = format!("alias-{name}");
                    let canonical = installations[index].identity.canonical();
                    actual.aliases.insert(name.clone(), canonical.clone());
                    actual.save(&paths).unwrap();
                    expected.aliases.insert(name, canonical);
                }
                StateOperation::Uninstall { index } if expected.installed[index] => {
                    let canonical = installations[index].identity.canonical();
                    state::uninstall(&paths, &mut actual, &canonical).unwrap();
                    expected.installed[index] = false;
                    if expected.active.as_deref() == Some(&canonical) {
                        expected.active = None;
                    }
                    if expected.default.as_deref() == Some(&canonical) {
                        expected.default = None;
                    }
                    expected.aliases.retain(|_, value| value != &canonical);
                }
                _ => {}
            }

            assert_state_matches(&actual, &expected);
            assert_state_matches(&State::load(&paths).unwrap(), &expected);
            prop_assert!(!paths.pending().exists());
            for (index, installation) in installations.iter().enumerate() {
                prop_assert_eq!(
                    paths.install_dir(&installation.identity.canonical()).exists(),
                    expected.installed[index],
                );
            }
        }
    }
}
