//! Feature registry: maps [`FeatureType`] handles to [`FeatureProperties`].
//!
//! The registry allows game-specific features to be defined in data (JSON)
//! rather than requiring code changes for each new feature type. This mirrors
//! the tile registry pattern from [`crate::tile::registry`].

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::tag::Tag;

// ─── FeatureType ────────────────────────────────────────────────────────────

/// A data-driven feature type identifier.
///
/// This is the replacement for the old `FeatureKind` enum — any string can
/// name a feature type, and the [`FeatureRegistry`] maps it to properties.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FeatureType(pub String);

impl std::fmt::Display for FeatureType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for FeatureType {
    fn from(s: &str) -> Self {
        FeatureType(s.to_string())
    }
}

impl From<String> for FeatureType {
    fn from(s: String) -> Self {
        FeatureType(s)
    }
}

// ─── FeatureCategory ────────────────────────────────────────────────────────

/// Broad category for gameplay behavior dispatch.
///
/// Unlike [`FeatureType`] (which can be any string), the category is a fixed
/// enum so that gameplay systems can branch on it without string matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeatureCategory {
    Furniture,
    Container,
    Trap,
    Decoration,
    Interactable,
}

// ─── FeatureProperties ──────────────────────────────────────────────────────

/// Describes the properties of a feature type in the registry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureProperties {
    /// Human-readable name (same as the registry key, e.g. "altar").
    pub name: String,
    /// Which category for behavior dispatch.
    pub category: FeatureCategory,
    /// Character used for ASCII debug rendering.
    pub ascii_char: char,
    /// Does this feature block movement?
    pub blocking: bool,
    /// Arbitrary tags for rule matching.
    #[serde(default)]
    pub tags: Vec<Tag>,
}

// ─── FeatureRegistry ────────────────────────────────────────────────────────

/// Maps feature type names → [`FeatureProperties`].
///
/// Built-in features are always registered at construction time.
/// Additional features can be added via [`FeatureRegistry::register`] or
/// loaded from a JSON asset file.
#[derive(Debug, Clone)]
pub struct FeatureRegistry {
    features: HashMap<String, FeatureProperties>,
}

impl FeatureRegistry {
    /// Creates a registry pre-populated with the built-in feature types.
    pub fn default_registry() -> Self {
        let builtins = vec![
            FeatureProperties {
                name: "altar".into(),
                category: FeatureCategory::Interactable,
                ascii_char: '†',
                blocking: true,
                tags: vec![Tag::from("religious")],
            },
            FeatureProperties {
                name: "sarcophagus".into(),
                category: FeatureCategory::Container,
                ascii_char: 'S',
                blocking: true,
                tags: vec![Tag::from("burial")],
            },
            FeatureProperties {
                name: "chest".into(),
                category: FeatureCategory::Container,
                ascii_char: '$',
                blocking: true,
                tags: vec![Tag::from("loot")],
            },
            FeatureProperties {
                name: "barrel".into(),
                category: FeatureCategory::Container,
                ascii_char: 'o',
                blocking: true,
                tags: vec![Tag::from("storage")],
            },
            FeatureProperties {
                name: "shelf".into(),
                category: FeatureCategory::Furniture,
                ascii_char: '=',
                blocking: true,
                tags: vec![Tag::from("storage")],
            },
            FeatureProperties {
                name: "table".into(),
                category: FeatureCategory::Furniture,
                ascii_char: 'T',
                blocking: true,
                tags: vec![Tag::from("furniture")],
            },
            FeatureProperties {
                name: "trap".into(),
                category: FeatureCategory::Trap,
                ascii_char: '^',
                blocking: false,
                tags: vec![Tag::from("hazard")],
            },
            FeatureProperties {
                name: "torch".into(),
                category: FeatureCategory::Decoration,
                ascii_char: '!',
                blocking: false,
                tags: vec![Tag::from("light")],
            },
            FeatureProperties {
                name: "moss".into(),
                category: FeatureCategory::Decoration,
                ascii_char: ',',
                blocking: false,
                tags: vec![Tag::from("natural"), Tag::from("cosmetic")],
            },
            FeatureProperties {
                name: "vines".into(),
                category: FeatureCategory::Decoration,
                ascii_char: ',',
                blocking: false,
                tags: vec![Tag::from("natural"), Tag::from("cosmetic")],
            },
            FeatureProperties {
                name: "fungus".into(),
                category: FeatureCategory::Decoration,
                ascii_char: ',',
                blocking: false,
                tags: vec![
                    Tag::from("natural"),
                    Tag::from("cosmetic"),
                    Tag::from("luminous"),
                ],
            },
            FeatureProperties {
                name: "stalagmite".into(),
                category: FeatureCategory::Decoration,
                ascii_char: '\u{00a4}',
                blocking: true,
                tags: vec![Tag::from("natural")],
            },
            FeatureProperties {
                name: "stalactite".into(),
                category: FeatureCategory::Decoration,
                ascii_char: '\u{00a4}',
                blocking: false,
                tags: vec![Tag::from("natural")],
            },
            FeatureProperties {
                name: "crystal".into(),
                category: FeatureCategory::Decoration,
                ascii_char: '\u{00a4}',
                blocking: false,
                tags: vec![Tag::from("natural"), Tag::from("luminous")],
            },
            FeatureProperties {
                name: "banner".into(),
                category: FeatureCategory::Decoration,
                ascii_char: '|',
                blocking: false,
                tags: vec![Tag::from("noble"), Tag::from("cosmetic")],
            },
            FeatureProperties {
                name: "tapestry".into(),
                category: FeatureCategory::Decoration,
                ascii_char: '|',
                blocking: false,
                tags: vec![Tag::from("noble"), Tag::from("cosmetic")],
            },
            FeatureProperties {
                name: "lantern".into(),
                category: FeatureCategory::Decoration,
                ascii_char: '!',
                blocking: false,
                tags: vec![Tag::from("light")],
            },
            FeatureProperties {
                name: "rat_nest".into(),
                category: FeatureCategory::Decoration,
                ascii_char: '~',
                blocking: false,
                tags: vec![Tag::from("vermin")],
            },
        ];

        Self::from_properties(builtins)
    }

    /// Builds a registry from a vec of feature properties (e.g. loaded from JSON).
    ///
    /// Uses each property's `name` field as the key.
    pub fn from_properties(props: Vec<FeatureProperties>) -> Self {
        let features = props.into_iter().map(|p| (p.name.clone(), p)).collect();
        Self { features }
    }

    /// Register a new feature type and return its [`FeatureType`].
    pub fn register(&mut self, props: FeatureProperties) -> FeatureType {
        let ft = FeatureType::from(props.name.clone());
        self.features.insert(props.name.clone(), props);
        ft
    }

    /// Look up properties for a feature type. Returns `None` if unknown.
    pub fn get(&self, feature_type: &FeatureType) -> Option<&FeatureProperties> {
        self.features.get(&feature_type.0)
    }

    /// Convenience: returns the category for a feature type, or `None` if unknown.
    pub fn category(&self, feature_type: &FeatureType) -> Option<FeatureCategory> {
        self.get(feature_type).map(|p| p.category)
    }

    /// Returns the ASCII character for rendering. Returns `'?'` for unknown types.
    pub fn ascii_char(&self, feature_type: &FeatureType) -> char {
        self.get(feature_type).map_or('?', |p| p.ascii_char)
    }

    /// Returns whether the feature blocks movement. Returns `false` for unknown types.
    pub fn is_blocking(&self, feature_type: &FeatureType) -> bool {
        self.get(feature_type).is_some_and(|p| p.blocking)
    }

    /// The total number of registered feature types.
    pub fn len(&self) -> usize {
        self.features.len()
    }

    /// Whether the registry is empty (should never be true after construction).
    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }
}

impl Default for FeatureRegistry {
    fn default() -> Self {
        Self::default_registry()
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_has_builtin_features() {
        let reg = FeatureRegistry::default_registry();
        assert_eq!(reg.len(), 18);
        assert!(reg.get(&FeatureType::from("altar")).is_some());
        assert!(reg.get(&FeatureType::from("chest")).is_some());
        assert!(reg.get(&FeatureType::from("torch")).is_some());
        assert!(reg.get(&FeatureType::from("stalagmite")).is_some());
        assert!(reg.get(&FeatureType::from("rat_nest")).is_some());
    }

    #[test]
    fn feature_type_from_str() {
        let ft = FeatureType::from("altar");
        assert_eq!(ft.0, "altar");
        assert_eq!(ft.to_string(), "altar");
    }

    #[test]
    fn registry_category_lookup() {
        let reg = FeatureRegistry::default_registry();
        assert_eq!(
            reg.category(&FeatureType::from("chest")),
            Some(FeatureCategory::Container)
        );
        assert_eq!(
            reg.category(&FeatureType::from("altar")),
            Some(FeatureCategory::Interactable)
        );
        assert_eq!(
            reg.category(&FeatureType::from("trap")),
            Some(FeatureCategory::Trap)
        );
        assert_eq!(
            reg.category(&FeatureType::from("table")),
            Some(FeatureCategory::Furniture)
        );
        assert_eq!(
            reg.category(&FeatureType::from("torch")),
            Some(FeatureCategory::Decoration)
        );
    }

    #[test]
    fn registry_ascii_char() {
        let reg = FeatureRegistry::default_registry();
        assert_eq!(reg.ascii_char(&FeatureType::from("altar")), '†');
        assert_eq!(reg.ascii_char(&FeatureType::from("chest")), '$');
        assert_eq!(reg.ascii_char(&FeatureType::from("trap")), '^');
        assert_eq!(reg.ascii_char(&FeatureType::from("torch")), '!');
        assert_eq!(reg.ascii_char(&FeatureType::from("stalagmite")), '\u{00a4}');
    }

    #[test]
    fn unknown_feature_type_returns_defaults() {
        let reg = FeatureRegistry::default_registry();
        let unknown = FeatureType::from("nonexistent");
        assert_eq!(reg.get(&unknown), None);
        assert_eq!(reg.ascii_char(&unknown), '?');
        assert!(!reg.is_blocking(&unknown));
        assert_eq!(reg.category(&unknown), None);
    }

    #[test]
    fn register_custom_feature() {
        let mut reg = FeatureRegistry::default_registry();
        let prev_len = reg.len();
        let ft = reg.register(FeatureProperties {
            name: "magic_circle".into(),
            category: FeatureCategory::Interactable,
            ascii_char: '@',
            blocking: false,
            tags: vec![Tag::from("magic"), Tag::from("arcane")],
        });
        assert_eq!(ft, FeatureType::from("magic_circle"));
        assert_eq!(reg.len(), prev_len + 1);
        assert!(reg.get(&ft).is_some());
        assert_eq!(reg.ascii_char(&ft), '@');
        assert!(!reg.is_blocking(&ft));
        assert_eq!(reg.category(&ft), Some(FeatureCategory::Interactable));
    }
}
