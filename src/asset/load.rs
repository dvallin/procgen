//! Asset loading utilities for JSON rule files.
//!
//! Provides generic `load_rules` functions that deserialize a JSON array
//! from either a file path or an embedded string (via `include_str!`).
//!
//! The [`load_rules_with_fallback`] function implements the preferred pattern:
//! try a file path first (allows user overrides), fall back to compiled-in defaults.

use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

use crate::entity::rules::EntityRule;
use crate::feature::rules::FeatureRule;
use crate::intent::pattern::NarrativePattern;
use crate::intent::vocabulary::ThemeVocabulary;
use crate::tile::registry::{TileProperties, TileRegistry};
use crate::tile::scatter::TileScatterRule;

// ─── Embedded defaults (compiled into the binary) ───────────────────────────

/// Default feature rules, embedded at compile time.
const EMBEDDED_FEATURE_RULES: &str = include_str!("../../assets/rules/features.json");

/// Default entity rules, embedded at compile time.
const EMBEDDED_ENTITY_RULES: &str = include_str!("../../assets/rules/entities.json");

/// Default narrative patterns, embedded at compile time.
const EMBEDDED_NARRATIVE_PATTERNS: &str = include_str!("../../assets/patterns/narrative.json");

/// Default theme vocabularies, embedded at compile time.
const EMBEDDED_THEME_VOCABULARIES: &str = include_str!("../../assets/vocabularies/themes.json");

/// Default tile registry, embedded at compile time.
const EMBEDDED_TILE_REGISTRY: &str = include_str!("../../assets/rules/tiles.json");

/// Default tile scatter rules, embedded at compile time.
const EMBEDDED_TILE_SCATTER_RULES: &str = include_str!("../../assets/rules/tile_scatter.json");

// ─── Error type ─────────────────────────────────────────────────────────────

/// Errors that can occur when loading asset files.
#[derive(Debug)]
pub enum AssetLoadError {
    /// The file could not be read from disk.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// The file content could not be parsed as valid JSON.
    Parse {
        path: String,
        source: serde_json::Error,
    },
}

impl std::fmt::Display for AssetLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(
                    f,
                    "failed to read asset file '{}': {}",
                    path.display(),
                    source
                )
            }
            Self::Parse { path, source } => {
                write!(f, "failed to parse asset '{}': {}", path, source)
            }
        }
    }
}

impl std::error::Error for AssetLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
        }
    }
}

// ─── Generic loaders ────────────────────────────────────────────────────────

/// Load a JSON array of rules from a file path.
///
/// # Example
/// ```no_run
/// use procgen::asset::load::load_rules;
/// use procgen::feature::rules::FeatureRule;
///
/// let rules: Vec<FeatureRule> = load_rules("assets/rules/features.json").unwrap();
/// ```
pub fn load_rules<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<Vec<T>, AssetLoadError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|e| AssetLoadError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    serde_json::from_str(&text).map_err(|e| AssetLoadError::Parse {
        path: path.display().to_string(),
        source: e,
    })
}

/// Load rules from a JSON string (for `include_str!` embedded usage).
///
/// The `label` argument is used in error messages to identify the source.
///
/// # Example
/// ```
/// use procgen::asset::load::load_rules_from_str;
/// use procgen::feature::rules::FeatureRule;
///
/// let json = r#"[{"kind":"Chest","strategy":"Center","required":true,"max_count":1,"match_role":null,"match_archetype":null,"match_tag":null}]"#;
/// let rules: Vec<FeatureRule> = load_rules_from_str(json, "inline").unwrap();
/// assert_eq!(rules.len(), 1);
/// ```
pub fn load_rules_from_str<T: DeserializeOwned>(
    json: &str,
    label: &str,
) -> Result<Vec<T>, AssetLoadError> {
    serde_json::from_str(json).map_err(|e| AssetLoadError::Parse {
        path: label.to_string(),
        source: e,
    })
}

/// Load rules from a file path if it exists, otherwise fall back to an
/// embedded JSON string.
///
/// This is the preferred loading strategy: it allows users to override
/// rules by placing a JSON file at the expected path, while ensuring
/// the application always works out-of-the-box with compiled-in defaults.
pub fn load_rules_with_fallback<T: DeserializeOwned>(
    path: impl AsRef<Path>,
    embedded_fallback: &str,
) -> Result<Vec<T>, AssetLoadError> {
    let path = path.as_ref();
    if path.exists() {
        load_rules(path)
    } else {
        load_rules_from_str(embedded_fallback, &path.display().to_string())
    }
}

// ─── Convenience functions for default rule sets ────────────────────────────

/// Default file path for feature rules, relative to the working directory.
pub const DEFAULT_FEATURE_RULES_PATH: &str = "assets/rules/features.json";

/// Default file path for entity rules, relative to the working directory.
pub const DEFAULT_ENTITY_RULES_PATH: &str = "assets/rules/entities.json";

/// Default file path for narrative patterns, relative to the working directory.
pub const DEFAULT_PATTERNS_PATH: &str = "assets/patterns/narrative.json";

/// Default file path for theme vocabularies, relative to the working directory.
pub const DEFAULT_VOCABULARIES_PATH: &str = "assets/vocabularies/themes.json";

/// Load the default feature rules.
///
/// Tries `assets/rules/features.json` on disk first, falls back to
/// the compiled-in version if the file doesn't exist.
pub fn load_default_feature_rules() -> Result<Vec<FeatureRule>, AssetLoadError> {
    load_rules_with_fallback(DEFAULT_FEATURE_RULES_PATH, EMBEDDED_FEATURE_RULES)
}

/// Load the default entity rules.
///
/// Tries `assets/rules/entities.json` on disk first, falls back to
/// the compiled-in version if the file doesn't exist.
pub fn load_default_entity_rules() -> Result<Vec<EntityRule>, AssetLoadError> {
    load_rules_with_fallback(DEFAULT_ENTITY_RULES_PATH, EMBEDDED_ENTITY_RULES)
}

/// Load the default narrative patterns.
///
/// Tries `assets/patterns/narrative.json` on disk first, falls back to
/// the compiled-in version if the file doesn't exist.
pub fn load_default_patterns() -> Result<Vec<NarrativePattern>, AssetLoadError> {
    load_rules_with_fallback(DEFAULT_PATTERNS_PATH, EMBEDDED_NARRATIVE_PATTERNS)
}

/// Load the default theme vocabularies.
///
/// Tries `assets/vocabularies/themes.json` on disk first, falls back to
/// the compiled-in version if the file doesn't exist.
pub fn load_default_vocabularies() -> Result<Vec<ThemeVocabulary>, AssetLoadError> {
    load_rules_with_fallback(DEFAULT_VOCABULARIES_PATH, EMBEDDED_THEME_VOCABULARIES)
}

/// Default file path for the tile registry, relative to the working directory.
pub const DEFAULT_TILE_REGISTRY_PATH: &str = "assets/rules/tiles.json";

/// Load the default tile registry.
///
/// Tries `assets/rules/tiles.json` on disk first, falls back to the
/// compiled-in version if the file doesn't exist. Builds a [`TileRegistry`]
/// from the loaded tile properties.
pub fn load_default_tile_registry() -> Result<TileRegistry, AssetLoadError> {
    let props: Vec<TileProperties> =
        load_rules_with_fallback(DEFAULT_TILE_REGISTRY_PATH, EMBEDDED_TILE_REGISTRY)?;
    let mut registry = TileRegistry::from_properties(props);
    // Ensure we have at least the builtin count (pad with defaults if JSON is short)
    let builtin = TileRegistry::default_registry();
    while registry.len() < builtin.len() {
        let id_val = registry.len() as u16;
        if let Some(p) = builtin.get(crate::tile::registry::TileId(id_val)) {
            registry.register(p.clone());
        } else {
            break;
        }
    }
    Ok(registry)
}

/// Default file path for tile scatter rules, relative to the working directory.
pub const DEFAULT_TILE_SCATTER_RULES_PATH: &str = "assets/rules/tile_scatter.json";

/// Load the default tile scatter rules.
///
/// Tries `assets/rules/tile_scatter.json` on disk first, falls back to
/// the compiled-in version if the file doesn't exist.
pub fn load_default_tile_scatter_rules() -> Result<Vec<TileScatterRule>, AssetLoadError> {
    load_rules_with_fallback(DEFAULT_TILE_SCATTER_RULES_PATH, EMBEDDED_TILE_SCATTER_RULES)
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::plan::EntityArchetypeId;
    use crate::feature::plan::FeatureKind;
    use crate::intent::graph::NodeRole;
    use crate::spatial::plan::SpaceArchetype;
    use crate::tag::Tag;

    #[test]
    fn load_embedded_feature_rules_succeeds() {
        let rules = load_default_feature_rules().unwrap();
        assert_eq!(rules.len(), 13);

        // Spot-check a known rule.
        let sarcophagus = rules.iter().find(|r| r.kind == FeatureKind::Sarcophagus);
        assert!(sarcophagus.is_some());
        let rule = sarcophagus.unwrap();
        assert!(rule.required);
        assert_eq!(rule.match_archetype, Some(SpaceArchetype::Vault));
        assert_eq!(rule.match_tag, Some(Tag::from("main_goal")));
    }

    #[test]
    fn load_embedded_entity_rules_succeeds() {
        let rules = load_default_entity_rules().unwrap();
        assert_eq!(rules.len(), 7);

        // Spot-check a known rule.
        let guardian = rules
            .iter()
            .find(|r| r.archetype == EntityArchetypeId::from("skeleton_guardian"));
        assert!(guardian.is_some());
        let rule = guardian.unwrap();
        assert_eq!(rule.min_count, 1);
        assert_eq!(rule.role_match, Some(NodeRole::Goal));
    }

    #[test]
    fn load_rules_from_str_parses_minimal_json() {
        let json = r#"[
            {
                "kind": "Table",
                "strategy": "Center",
                "required": false,
                "max_count": 1,
                "match_role": "Hub",
                "match_archetype": null,
                "match_tag": null
            }
        ]"#;
        let rules: Vec<FeatureRule> = load_rules_from_str(json, "test").unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].kind, FeatureKind::Table);
    }

    #[test]
    fn load_rules_from_str_invalid_json_gives_parse_error() {
        let result: Result<Vec<FeatureRule>, _> =
            load_rules_from_str("not valid json", "bad_input");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, AssetLoadError::Parse { .. }));
        assert!(err.to_string().contains("bad_input"));
    }

    #[test]
    fn load_rules_nonexistent_file_gives_io_error() {
        let result: Result<Vec<FeatureRule>, _> = load_rules("/nonexistent/path.json");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, AssetLoadError::Io { .. }));
        assert!(err.to_string().contains("nonexistent"));
    }

    #[test]
    fn load_rules_with_fallback_uses_file_when_present() {
        // The assets/rules/features.json file exists in our project, so
        // load_rules_with_fallback should use it rather than the fallback.
        let rules: Vec<FeatureRule> =
            load_rules_with_fallback("assets/rules/features.json", "[]").unwrap();
        assert_eq!(rules.len(), 13);
    }

    #[test]
    fn load_rules_with_fallback_uses_embedded_when_file_missing() {
        let rules: Vec<FeatureRule> =
            load_rules_with_fallback("nonexistent/path/features.json", EMBEDDED_FEATURE_RULES)
                .unwrap();
        assert_eq!(rules.len(), 13);
    }

    #[test]
    fn asset_load_error_display_io() {
        let err = AssetLoadError::Io {
            path: PathBuf::from("some/file.json"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
        };
        let msg = err.to_string();
        assert!(msg.contains("some/file.json"));
        assert!(msg.contains("file not found"));
    }

    #[test]
    fn asset_load_error_display_parse() {
        let bad_json = "{{invalid";
        let serde_err = serde_json::from_str::<Vec<FeatureRule>>(bad_json).unwrap_err();
        let err = AssetLoadError::Parse {
            path: "test.json".to_string(),
            source: serde_err,
        };
        let msg = err.to_string();
        assert!(msg.contains("test.json"));
        assert!(msg.contains("parse"));
    }

    #[test]
    fn load_embedded_patterns_succeeds() {
        let patterns = load_default_patterns().unwrap();
        assert_eq!(patterns.len(), 5);

        // Verify pattern IDs.
        let ids: Vec<&str> = patterns.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"lock_and_key"));
        assert!(ids.contains(&"hub_and_spoke"));
        assert!(ids.contains(&"linear_descent"));
        assert!(ids.contains(&"gauntlet"));
        assert!(ids.contains(&"branching_exploration"));
    }

    #[test]
    fn lock_and_key_pattern_structure() {
        use crate::intent::graph::EdgeRole;

        let patterns = load_default_patterns().unwrap();
        let lak = patterns.iter().find(|p| p.id == "lock_and_key").unwrap();

        // Should have 6 slots: entry, hub, key_area (optional), gate, goal, reward (optional)
        assert_eq!(lak.slots.len(), 6);
        let optional_count = lak.slots.iter().filter(|s| s.optional).count();
        assert_eq!(optional_count, 2);

        // Should have 5 edges.
        assert_eq!(lak.edges.len(), 5);

        // Gate→goal edge should be RestrictedTraversal.
        let gate_goal = lak
            .edges
            .iter()
            .find(|e| e.from == "gate" && e.to == "goal");
        assert!(gate_goal.is_some());
        assert_eq!(gate_goal.unwrap().role, EdgeRole::RestrictedTraversal);

        // No max_depth for lock_and_key.
        assert_eq!(lak.max_depth, None);

        // Should have votes.
        assert!(!lak.votes.is_empty());
        let vault_vote = lak.votes.iter().find(|v| v.tag == "locked_vault");
        assert!(vault_vote.is_some());
        assert_eq!(vault_vote.unwrap().weight, 3);
    }

    #[test]
    fn all_patterns_have_entry_slot() {
        let patterns = load_default_patterns().unwrap();
        for pattern in &patterns {
            let has_entry = pattern.slots.iter().any(|s| s.role == NodeRole::Entry);
            assert!(
                has_entry,
                "pattern '{}' is missing an Entry slot",
                pattern.id
            );
        }
    }

    #[test]
    fn all_pattern_edges_reference_valid_slots() {
        let patterns = load_default_patterns().unwrap();
        for pattern in &patterns {
            let keys: Vec<&str> = pattern.slots.iter().map(|s| s.key.as_str()).collect();
            for edge in &pattern.edges {
                assert!(
                    keys.contains(&edge.from.as_str()),
                    "pattern '{}' edge references unknown slot '{}'",
                    pattern.id,
                    edge.from
                );
                assert!(
                    keys.contains(&edge.to.as_str()),
                    "pattern '{}' edge references unknown slot '{}'",
                    pattern.id,
                    edge.to
                );
            }
        }
    }

    // ─── Vocabulary tests ────────────────────────────────────────────────────

    #[test]
    fn load_embedded_vocabularies_succeeds() {
        let vocabs = load_default_vocabularies().unwrap();
        assert_eq!(vocabs.len(), 3);

        let ids: Vec<&str> = vocabs.iter().map(|v| v.id.as_str()).collect();
        assert!(ids.contains(&"undead_nobility"));
        assert!(ids.contains(&"urban_underground"));
        assert!(ids.contains(&"natural_cave"));
    }

    #[test]
    fn undead_nobility_has_all_roles() {
        let vocabs = load_default_vocabularies().unwrap();
        let undead = vocabs.iter().find(|v| v.id == "undead_nobility").unwrap();

        // Should have entries for all 7 roles.
        let roles: Vec<NodeRole> = undead.roles.iter().map(|r| r.role).collect();
        assert!(roles.contains(&NodeRole::Entry));
        assert!(roles.contains(&NodeRole::Hub));
        assert!(roles.contains(&NodeRole::Gate));
        assert!(roles.contains(&NodeRole::Goal));
        assert!(roles.contains(&NodeRole::Reward));
        assert!(roles.contains(&NodeRole::Branch));
        assert!(roles.contains(&NodeRole::Transition));
    }

    #[test]
    fn urban_underground_has_all_roles() {
        let vocabs = load_default_vocabularies().unwrap();
        let urban = vocabs.iter().find(|v| v.id == "urban_underground").unwrap();

        let roles: Vec<NodeRole> = urban.roles.iter().map(|r| r.role).collect();
        assert!(roles.contains(&NodeRole::Entry));
        assert!(roles.contains(&NodeRole::Hub));
        assert!(roles.contains(&NodeRole::Gate));
        assert!(roles.contains(&NodeRole::Goal));
        assert!(roles.contains(&NodeRole::Reward));
        assert!(roles.contains(&NodeRole::Branch));
        assert!(roles.contains(&NodeRole::Transition));
    }

    #[test]
    fn entries_for_role_filters_correctly() {
        let vocabs = load_default_vocabularies().unwrap();
        let undead = vocabs.iter().find(|v| v.id == "undead_nobility").unwrap();

        let hub_entries = undead.entries_for_role(NodeRole::Hub);
        assert_eq!(hub_entries.len(), 2);
        let labels: Vec<&str> = hub_entries.iter().map(|e| e.label.as_str()).collect();
        assert!(labels.contains(&"Great Hall"));
        assert!(labels.contains(&"Ossuary"));
    }

    #[test]
    fn all_vocabulary_entries_have_non_empty_labels() {
        let vocabs = load_default_vocabularies().unwrap();
        for vocab in &vocabs {
            for role_vocab in &vocab.roles {
                for entry in &role_vocab.entries {
                    assert!(
                        !entry.label.is_empty(),
                        "vocabulary '{}' role {:?} has empty label",
                        vocab.id,
                        role_vocab.role
                    );
                }
            }
        }
    }

    #[test]
    fn load_embedded_tile_registry_succeeds() {
        let registry = load_default_tile_registry().unwrap();
        assert_eq!(registry.len(), 10);
    }

    #[test]
    fn tile_registry_from_json_matches_defaults() {
        use crate::tile::registry::Tile;
        let registry = load_default_tile_registry().unwrap();
        let default = TileRegistry::default_registry();

        // Same number of tiles
        assert_eq!(registry.len(), default.len());

        // Same properties for each built-in tile
        for id_val in 0..Tile::BUILTIN_COUNT {
            let id = crate::tile::registry::TileId(id_val);
            assert_eq!(
                registry.name(id),
                default.name(id),
                "name mismatch for {}",
                id
            );
            assert_eq!(
                registry.is_walkable(id),
                default.is_walkable(id),
                "walkable mismatch for {}",
                id
            );
            assert_eq!(
                registry.is_opaque(id),
                default.is_opaque(id),
                "opaque mismatch for {}",
                id
            );
            assert_eq!(
                registry.ascii_char(id),
                default.ascii_char(id),
                "ascii_char mismatch for {}",
                id
            );
        }
    }
}
