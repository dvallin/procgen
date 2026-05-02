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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeVocabulary {
    /// Unique identifier matching the situation's "theme" binding (e.g. "undead_nobility").
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Optional description.
    #[serde(default)]
    pub description: String,
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
}
