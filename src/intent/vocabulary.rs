//! Theme vocabulary — maps (NodeRole, theme) to concrete slot content.
//!
//! A vocabulary provides themed labels, tags, and archetypes for pattern slots.
//! E.g., theme "undead_nobility": Hub → ("Great Hall", [noble, sealed], Hall).

use serde::{Deserialize, Serialize};

use crate::intent::graph::NodeRole;
use crate::intent::map_intent::LocationKind;
use crate::spatial::plan::SpaceArchetype;

/// A single vocabulary entry — one possible way to fill a slot with given role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VocabularyEntry {
    /// Human-readable label for the space (e.g. "Great Hall", "Chapel Vestry").
    pub label: String,
    /// Tags to attach to the instantiated node.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Architectural archetype for the space.
    pub archetype: SpaceArchetype,
}

/// A role binding within a theme — all entries available for a specific NodeRole.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleVocabulary {
    /// Which role these entries can fill.
    pub role: NodeRole,
    /// Available entries (one will be selected per slot during instantiation).
    pub entries: Vec<VocabularyEntry>,
}

/// A complete theme vocabulary — all role bindings for a single theme.
///
/// Themes are the primary variety knob: adding a new scenario means adding
/// vocabulary entries + situation tags. The `id` matches the "theme" binding
/// in `SituationContext`.
///
/// **Overlay composition:** When `base` is set, the vocabulary inherits from
/// the referenced base vocabulary. Roles defined in the overlay *extend* the
/// base's entries for that role (both pools are available to the slot filler).
/// `location_kind` and `motifs` in the overlay override the base if specified.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeVocabulary {
    /// Unique identifier matching the situation's "theme" binding (e.g. "undead_nobility").
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Optional description.
    #[serde(default)]
    pub description: String,
    /// Optional base vocabulary to inherit from.
    /// When set, roles from the base are merged with this vocabulary's roles.
    #[serde(default)]
    pub base: Option<String>,
    /// What kind of location this theme produces (Dungeon, Building, etc.).
    pub location_kind: LocationKind,
    /// Style motifs associated with this theme (e.g. ["gothic", "stone"]).
    #[serde(default)]
    pub motifs: Vec<String>,
    /// Role-indexed vocabulary entries.
    pub roles: Vec<RoleVocabulary>,
}

impl ThemeVocabulary {
    /// Find all entries for a given role in this vocabulary.
    pub fn entries_for_role(&self, role: NodeRole) -> Vec<&VocabularyEntry> {
        self.roles
            .iter()
            .filter(|rv| rv.role == role)
            .flat_map(|rv| rv.entries.iter())
            .collect()
    }

    /// Resolve this vocabulary against a library of vocabularies, flattening
    /// any `base` inheritance into a single vocabulary with merged roles.
    ///
    /// - Roles defined in the overlay extend (add to) the base's entries for that role.
    /// - Roles only in the base are inherited as-is.
    /// - `location_kind` and `motifs` from the overlay take precedence.
    ///
    /// Returns `None` if `base` references a vocabulary that doesn't exist in the library.
    /// Returns `Some(self.clone())` if no base is set.
    pub fn resolve(&self, library: &[ThemeVocabulary]) -> Option<ThemeVocabulary> {
        let base_id = match &self.base {
            Some(id) => id,
            None => return Some(self.clone()),
        };

        let base = library.iter().find(|v| v.id == *base_id)?;

        // Start with base roles, then merge overlay roles.
        let mut merged_roles: Vec<RoleVocabulary> = base.roles.clone();

        for overlay_role in &self.roles {
            if let Some(existing) = merged_roles
                .iter_mut()
                .find(|r| r.role == overlay_role.role)
            {
                // Extend the base role's entries with overlay entries.
                existing.entries.extend(overlay_role.entries.clone());
            } else {
                // Role not in base — add it fresh.
                merged_roles.push(overlay_role.clone());
            }
        }

        Some(ThemeVocabulary {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            base: None, // resolved — no longer an overlay
            location_kind: self.location_kind,
            motifs: if self.motifs.is_empty() {
                base.motifs.clone()
            } else {
                self.motifs.clone()
            },
            roles: merged_roles,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_vocabulary() -> ThemeVocabulary {
        ThemeVocabulary {
            id: "base".to_string(),
            name: "Base".to_string(),
            description: String::new(),
            base: None,
            location_kind: LocationKind::Dungeon,
            motifs: vec!["dark".to_string(), "stone".to_string()],
            roles: vec![
                RoleVocabulary {
                    role: NodeRole::Entry,
                    entries: vec![VocabularyEntry {
                        label: "Main Gate".to_string(),
                        tags: vec!["stone".to_string()],
                        archetype: SpaceArchetype::Vestibule,
                    }],
                },
                RoleVocabulary {
                    role: NodeRole::Hub,
                    entries: vec![VocabularyEntry {
                        label: "Great Hall".to_string(),
                        tags: vec!["noble".to_string()],
                        archetype: SpaceArchetype::Hall,
                    }],
                },
                RoleVocabulary {
                    role: NodeRole::Goal,
                    entries: vec![VocabularyEntry {
                        label: "Vault".to_string(),
                        tags: vec!["main_goal".to_string()],
                        archetype: SpaceArchetype::Vault,
                    }],
                },
            ],
        }
    }

    fn overlay_vocabulary() -> ThemeVocabulary {
        ThemeVocabulary {
            id: "overlay".to_string(),
            name: "Overlay".to_string(),
            description: String::new(),
            base: Some("base".to_string()),
            location_kind: LocationKind::Building,
            motifs: vec!["vermin".to_string()],
            roles: vec![
                RoleVocabulary {
                    role: NodeRole::Hub,
                    entries: vec![VocabularyEntry {
                        label: "Infested Hall".to_string(),
                        tags: vec!["infested".to_string()],
                        archetype: SpaceArchetype::Hall,
                    }],
                },
                RoleVocabulary {
                    role: NodeRole::Branch,
                    entries: vec![VocabularyEntry {
                        label: "Rat Tunnel".to_string(),
                        tags: vec!["infested".to_string()],
                        archetype: SpaceArchetype::Corridor,
                    }],
                },
            ],
        }
    }

    #[test]
    fn resolve_without_base_returns_self() {
        let base = base_vocabulary();
        let library = vec![base.clone()];
        let resolved = base.resolve(&library).unwrap();
        assert_eq!(resolved.id, "base");
        assert_eq!(resolved.roles.len(), 3);
    }

    #[test]
    fn resolve_extends_base_roles() {
        let library = vec![base_vocabulary(), overlay_vocabulary()];
        let overlay = &library[1];
        let resolved = overlay.resolve(&library).unwrap();

        // Hub should have 2 entries: base "Great Hall" + overlay "Infested Hall".
        let hub_entries = resolved.entries_for_role(NodeRole::Hub);
        assert_eq!(hub_entries.len(), 2);
        let labels: Vec<&str> = hub_entries.iter().map(|e| e.label.as_str()).collect();
        assert!(labels.contains(&"Great Hall"));
        assert!(labels.contains(&"Infested Hall"));
    }

    #[test]
    fn resolve_inherits_base_roles_not_in_overlay() {
        let library = vec![base_vocabulary(), overlay_vocabulary()];
        let overlay = &library[1];
        let resolved = overlay.resolve(&library).unwrap();

        // Entry and Goal are only in base — should be inherited.
        let entry_entries = resolved.entries_for_role(NodeRole::Entry);
        assert_eq!(entry_entries.len(), 1);
        assert_eq!(entry_entries[0].label, "Main Gate");

        let goal_entries = resolved.entries_for_role(NodeRole::Goal);
        assert_eq!(goal_entries.len(), 1);
        assert_eq!(goal_entries[0].label, "Vault");
    }

    #[test]
    fn resolve_adds_new_roles_from_overlay() {
        let library = vec![base_vocabulary(), overlay_vocabulary()];
        let overlay = &library[1];
        let resolved = overlay.resolve(&library).unwrap();

        // Branch is only in overlay — should be added.
        let branch_entries = resolved.entries_for_role(NodeRole::Branch);
        assert_eq!(branch_entries.len(), 1);
        assert_eq!(branch_entries[0].label, "Rat Tunnel");
    }

    #[test]
    fn resolve_overlay_overrides_location_kind_and_motifs() {
        let library = vec![base_vocabulary(), overlay_vocabulary()];
        let overlay = &library[1];
        let resolved = overlay.resolve(&library).unwrap();

        assert_eq!(resolved.location_kind, LocationKind::Building);
        assert_eq!(resolved.motifs, vec!["vermin".to_string()]);
    }

    #[test]
    fn resolve_inherits_motifs_when_overlay_empty() {
        let library = vec![base_vocabulary()];
        let overlay = ThemeVocabulary {
            id: "empty_overlay".to_string(),
            name: "Empty".to_string(),
            description: String::new(),
            base: Some("base".to_string()),
            location_kind: LocationKind::Dungeon,
            motifs: vec![], // empty — should inherit from base
            roles: vec![],
        };
        let mut lib_with_overlay = library.clone();
        lib_with_overlay.push(overlay.clone());

        let resolved = overlay.resolve(&lib_with_overlay).unwrap();
        assert_eq!(
            resolved.motifs,
            vec!["dark".to_string(), "stone".to_string()]
        );
    }

    #[test]
    fn resolve_missing_base_returns_none() {
        let overlay = overlay_vocabulary();
        let empty_library: Vec<ThemeVocabulary> = vec![];
        assert!(overlay.resolve(&empty_library).is_none());
    }

    #[test]
    fn resolve_clears_base_field() {
        let library = vec![base_vocabulary(), overlay_vocabulary()];
        let overlay = &library[1];
        let resolved = overlay.resolve(&library).unwrap();
        assert!(resolved.base.is_none());
    }
}
