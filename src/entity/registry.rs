//! Entity registry: maps [`EntityArchetypeId`] handles to [`EntityProperties`].
//!
//! The registry allows game-specific entities to be defined in data (JSON)
//! rather than requiring code changes for each new entity type. This mirrors
//! the feature registry pattern from [`crate::feature::registry`].

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::entity::plan::EntityArchetypeId;
use crate::tag::Tag;

// ─── EntityCategory ─────────────────────────────────────────────────────────

/// Broad category for gameplay behavior dispatch.
///
/// Unlike [`EntityArchetypeId`] (which can be any string), the category is a
/// fixed enum so that gameplay systems can branch on it without string matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityCategory {
    /// Hostile mobs (skeleton, rat, spider, etc.)
    Creature,
    /// Non-hostile named entities (smuggler, shopkeeper)
    NPC,
    /// Environmental threats (trap_mimic)
    Hazard,
    /// Elite creatures (giant_rat as boss, etc.)
    Boss,
}

// ─── EntityProperties ───────────────────────────────────────────────────────

/// Describes the properties of an entity type in the registry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityProperties {
    /// Human-readable name (same as the registry key, e.g. "skeleton").
    pub name: String,
    /// Which category for behavior dispatch.
    pub category: EntityCategory,
    /// Character used for ASCII debug rendering.
    pub ascii_char: char,
    /// Does this entity block movement?
    pub blocking: bool,
    /// Arbitrary tags for rule matching.
    #[serde(default)]
    pub tags: Vec<Tag>,
}

// ─── EntityRegistry ─────────────────────────────────────────────────────────

/// Maps entity archetype names → [`EntityProperties`].
///
/// Built-in entities are always registered at construction time.
/// Additional entities can be added via [`EntityRegistry::register`] or
/// loaded from a JSON asset file.
#[derive(Debug, Clone)]
pub struct EntityRegistry {
    entities: HashMap<String, EntityProperties>,
}

impl EntityRegistry {
    /// Creates a registry pre-populated with the built-in entity types.
    pub fn default_registry() -> Self {
        let builtins = vec![
            EntityProperties {
                name: "rat".into(),
                category: EntityCategory::Creature,
                ascii_char: 'r',
                blocking: false,
                tags: vec![Tag::from("vermin")],
            },
            EntityProperties {
                name: "tavern_rat".into(),
                category: EntityCategory::Creature,
                ascii_char: 'r',
                blocking: false,
                tags: vec![Tag::from("vermin")],
            },
            EntityProperties {
                name: "giant_rat".into(),
                category: EntityCategory::Boss,
                ascii_char: 'R',
                blocking: false,
                tags: vec![Tag::from("vermin"), Tag::from("boss")],
            },
            EntityProperties {
                name: "skeleton".into(),
                category: EntityCategory::Creature,
                ascii_char: 's',
                blocking: false,
                tags: vec![Tag::from("undead")],
            },
            EntityProperties {
                name: "skeleton_guardian".into(),
                category: EntityCategory::Creature,
                ascii_char: 'G',
                blocking: false,
                tags: vec![Tag::from("undead"), Tag::from("elite")],
            },
            EntityProperties {
                name: "gate_guardian".into(),
                category: EntityCategory::Creature,
                ascii_char: 'G',
                blocking: false,
                tags: vec![Tag::from("guardian"), Tag::from("elite")],
            },
            EntityProperties {
                name: "chest_mimic".into(),
                category: EntityCategory::Hazard,
                ascii_char: 'M',
                blocking: false,
                tags: vec![Tag::from("disguised"), Tag::from("trap")],
            },
            EntityProperties {
                name: "smuggler".into(),
                category: EntityCategory::NPC,
                ascii_char: '@',
                blocking: false,
                tags: vec![Tag::from("dialogue"), Tag::from("human")],
            },
            EntityProperties {
                name: "bat".into(),
                category: EntityCategory::Creature,
                ascii_char: 'b',
                blocking: false,
                tags: vec![Tag::from("flying"), Tag::from("natural")],
            },
            EntityProperties {
                name: "cave_spider".into(),
                category: EntityCategory::Creature,
                ascii_char: 'x',
                blocking: false,
                tags: vec![Tag::from("natural"), Tag::from("venomous")],
            },
            EntityProperties {
                name: "trap_mimic".into(),
                category: EntityCategory::Hazard,
                ascii_char: 'M',
                blocking: false,
                tags: vec![Tag::from("disguised"), Tag::from("trap")],
            },
        ];

        Self::from_properties(builtins)
    }

    /// Builds a registry from a vec of entity properties (e.g. loaded from JSON).
    ///
    /// Uses each property's `name` field as the key.
    pub fn from_properties(props: Vec<EntityProperties>) -> Self {
        let entities = props.into_iter().map(|p| (p.name.clone(), p)).collect();
        Self { entities }
    }

    /// Register a new entity type and return its [`EntityArchetypeId`].
    pub fn register(&mut self, props: EntityProperties) -> EntityArchetypeId {
        let id = EntityArchetypeId::from(props.name.clone());
        self.entities.insert(props.name.clone(), props);
        id
    }

    /// Look up properties for an entity archetype. Returns `None` if unknown.
    pub fn get(&self, archetype: &EntityArchetypeId) -> Option<&EntityProperties> {
        self.entities.get(&archetype.0)
    }

    /// Convenience: returns the category for an entity archetype, or `None` if unknown.
    pub fn category(&self, archetype: &EntityArchetypeId) -> Option<EntityCategory> {
        self.get(archetype).map(|p| p.category)
    }

    /// Returns the ASCII character for rendering. Returns `'?'` for unknown archetypes.
    pub fn ascii_char(&self, archetype: &EntityArchetypeId) -> char {
        self.get(archetype).map_or('?', |p| p.ascii_char)
    }

    /// Returns whether the entity blocks movement. Returns `false` for unknown archetypes.
    pub fn is_blocking(&self, archetype: &EntityArchetypeId) -> bool {
        self.get(archetype).is_some_and(|p| p.blocking)
    }

    /// The total number of registered entity types.
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Whether the registry is empty (should never be true after construction).
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }
}

impl Default for EntityRegistry {
    fn default() -> Self {
        Self::default_registry()
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_has_builtin_entities() {
        let reg = EntityRegistry::default_registry();
        assert_eq!(reg.len(), 11);
        assert!(reg.get(&EntityArchetypeId::from("rat")).is_some());
        assert!(reg.get(&EntityArchetypeId::from("skeleton")).is_some());
        assert!(reg.get(&EntityArchetypeId::from("smuggler")).is_some());
        assert!(reg.get(&EntityArchetypeId::from("giant_rat")).is_some());
        assert!(reg.get(&EntityArchetypeId::from("trap_mimic")).is_some());
    }

    #[test]
    fn entity_archetype_lookup() {
        let reg = EntityRegistry::default_registry();
        let props = reg.get(&EntityArchetypeId::from("skeleton")).unwrap();
        assert_eq!(props.name, "skeleton");
        assert_eq!(props.category, EntityCategory::Creature);
        assert_eq!(props.ascii_char, 's');
        assert!(!props.blocking);
        assert!(props.tags.contains(&Tag::from("undead")));
    }

    #[test]
    fn registry_category_lookup() {
        let reg = EntityRegistry::default_registry();
        assert_eq!(
            reg.category(&EntityArchetypeId::from("rat")),
            Some(EntityCategory::Creature)
        );
        assert_eq!(
            reg.category(&EntityArchetypeId::from("giant_rat")),
            Some(EntityCategory::Boss)
        );
        assert_eq!(
            reg.category(&EntityArchetypeId::from("smuggler")),
            Some(EntityCategory::NPC)
        );
        assert_eq!(
            reg.category(&EntityArchetypeId::from("chest_mimic")),
            Some(EntityCategory::Hazard)
        );
    }

    #[test]
    fn registry_ascii_char() {
        let reg = EntityRegistry::default_registry();
        assert_eq!(reg.ascii_char(&EntityArchetypeId::from("rat")), 'r');
        assert_eq!(reg.ascii_char(&EntityArchetypeId::from("giant_rat")), 'R');
        assert_eq!(
            reg.ascii_char(&EntityArchetypeId::from("skeleton_guardian")),
            'G'
        );
        assert_eq!(reg.ascii_char(&EntityArchetypeId::from("smuggler")), '@');
        assert_eq!(reg.ascii_char(&EntityArchetypeId::from("chest_mimic")), 'M');
    }

    #[test]
    fn unknown_entity_returns_defaults() {
        let reg = EntityRegistry::default_registry();
        let unknown = EntityArchetypeId::from("nonexistent");
        assert_eq!(reg.get(&unknown), None);
        assert_eq!(reg.ascii_char(&unknown), '?');
        assert!(!reg.is_blocking(&unknown));
        assert_eq!(reg.category(&unknown), None);
    }

    #[test]
    fn register_custom_entity() {
        let mut reg = EntityRegistry::default_registry();
        let prev_len = reg.len();
        let id = reg.register(EntityProperties {
            name: "lich_king".into(),
            category: EntityCategory::Boss,
            ascii_char: 'L',
            blocking: true,
            tags: vec![Tag::from("undead"), Tag::from("boss"), Tag::from("elite")],
        });
        assert_eq!(id, EntityArchetypeId::from("lich_king"));
        assert_eq!(reg.len(), prev_len + 1);
        assert!(reg.get(&id).is_some());
        assert_eq!(reg.ascii_char(&id), 'L');
        assert!(reg.is_blocking(&id));
        assert_eq!(reg.category(&id), Some(EntityCategory::Boss));
    }
}
