use crate::intent::graph::{EdgeRole, NodeRole, ScenarioNodeId};
use crate::tag::Tag;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpaceId(pub u32);

// --- Archetype: what kind of space this is architecturally ---

/// Architectural archetype — describes the *character* of a space
/// independent of its role in the scenario graph.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpaceArchetype {
    /// A large open area (great hall, throne room, cavern)
    Hall,
    /// A standard enclosed room (bedroom, office, cell)
    Chamber,
    /// A narrow connecting space (hallway, tunnel, bridge)
    Corridor,
    /// A small transitional space (foyer, airlock, antechamber)
    Vestibule,
    /// A secured storage area (treasury, vault, armory)
    Vault,
    /// A vertical connection (stairwell, elevator shaft, pit)
    Shaft,
    /// An open or partially-open area (courtyard, balcony, clearing)
    Courtyard,
    /// A specialized functional space (kitchen, lab, forge, chapel)
    Workshop,
    /// A large open urban area (market square, town square)
    Plaza,
    /// A linear urban thoroughfare (main road, boulevard)
    Street,
    /// A narrow urban passage (back alley, side lane)
    Alley,
    /// A small commercial building (storefront, market stall)
    Shop,
    /// A large storage or industrial building
    Warehouse,
}

/// How connectors (doors) should be distributed along a space's boundary.
/// Without distribution control, N doors randomly placed on walls produces visual chaos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConnectorDistribution {
    /// Evenly spaced along the wall (for street-side building entrances).
    Uniform,
    /// Grouped at the center of the wall (for plazas with clustered access).
    Clustered,
    /// Snapped to regular grid intervals (for warehouse loading bays).
    GridAligned,
    /// Only at the short sides / ends (for street endpoints, corridor-like).
    Ends,
}

// --- SizeHint: replaces hardcoded role→size mapping ---

/// Hint for the spatial planner about how large a space should be.
/// The geometry planner will interpret these into concrete dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SizeHint {
    /// 3×3 to 5×5
    Tiny,
    /// 5×5 to 7×7
    Small,
    /// 7×7 to 9×7
    Medium,
    /// 9×7 to 11×9
    Large,
    /// 11×9 to 15×13
    Grand,
    /// Explicit min/max bounds
    Custom {
        min_w: i32,
        min_h: i32,
        max_w: i32,
        max_h: i32,
    },
}

// --- Alignment types ---

/// Which side of a space to align or attach to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlignSide {
    North,
    South,
    East,
    West,
}

/// Preferred elongation axis for a space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Axis {
    /// The space should be wider than tall (w > h).
    Horizontal,
    /// The space should be taller than wide (h > w).
    Vertical,
}

// --- SpatialConstraint: rules between spaces ---

/// Constraints on the spatial arrangement. The spatial planner records these;
/// downstream layers (geometry, validation) enforce them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpatialConstraint {
    /// Two spaces must share a direct link (be immediately adjacent).
    MustBeAdjacent { a: SpaceId, b: SpaceId },
    /// Two spaces must NOT share a direct link.
    MustBeSeparated { a: SpaceId, b: SpaceId },
    /// Maximum graph distance (in link hops) between two spaces.
    MaxDistance {
        a: SpaceId,
        b: SpaceId,
        max_hops: u32,
    },
    /// A space must be reachable only through a specific gate space.
    GatedBy { space: SpaceId, gate: SpaceId },
    /// A space should be placed near the perimeter (entry/exit feel).
    PreferPerimeter { space: SpaceId },
    /// A space should be placed centrally (hub feel).
    PreferCentral { space: SpaceId },

    // ── Urban alignment constraints (6.3) ──
    /// Space `b`'s wall on `side` should align flush against space `a`'s long edge.
    /// Used for buildings aligning to streets.
    AlignEdge {
        a: SpaceId,
        b: SpaceId,
        side: AlignSide,
    },
    /// Building connects to a specific side of the street.
    /// Stronger than `AlignEdge` — implies adjacency + door placement on that face.
    AttachToEdge {
        building: SpaceId,
        street: SpaceId,
        side: AlignSide,
    },
    /// Space should be elongated along the given axis.
    /// Streets prefer horizontal/vertical orientation; plazas are square (no constraint).
    PreferOrientation { space: SpaceId, axis: Axis },
}

// --- Existing types, updated ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealizationStyle {
    RoomLike,
}

#[derive(Debug, Clone)]
pub struct AtomicSpace {
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone)]
pub enum SpaceKind {
    Atomic(AtomicSpace),
}

#[derive(Debug, Clone)]
pub struct SpaceSpec {
    pub id: SpaceId,
    pub origin: ScenarioNodeId,
    pub role: NodeRole,
    /// Structural tags — hard triggers for gameplay mechanics (e.g. "main_goal", "locked", "contains_key", "optional", "secret", "hidden").
    pub structural_tags: Vec<Tag>,
    /// Atmosphere tags — soft flavor that influences scatter, features, decorations (e.g. "damp", "overgrown", "vast").
    pub atmosphere_tags: Vec<Tag>,
    /// Motif tags — cross-cutting directives applied by the narrative system (e.g. "rat_infestation").
    pub motifs: Vec<Tag>,
    pub style: RealizationStyle,
    pub kind: SpaceKind,
    pub label: Option<String>,
    /// Architectural archetype — informs geometry and feature placement.
    pub archetype: Option<SpaceArchetype>,
    /// Size hint — guides the geometry planner's dimension choices.
    pub size_hint: SizeHint,
    /// Maximum number of connectors (doors) this space can have.
    /// None means use the default (typically 2 for rooms, unlimited for streets/plazas).
    pub max_connectors: Option<u32>,
    /// How connectors should be distributed along this space's boundary.
    /// None means default behavior (legacy: place wherever convenient).
    pub connector_distribution: Option<ConnectorDistribution>,
}

impl SpaceSpec {
    /// Returns all tags combined (structural + atmosphere + motifs).
    /// Useful for backward-compatible rule matching that checks against any tag.
    pub fn all_tags(&self) -> Vec<&Tag> {
        self.structural_tags
            .iter()
            .chain(self.atmosphere_tags.iter())
            .chain(self.motifs.iter())
            .collect()
    }

    /// Check if any tag field contains the given tag.
    pub fn has_tag(&self, tag: &Tag) -> bool {
        self.structural_tags.contains(tag)
            || self.atmosphere_tags.contains(tag)
            || self.motifs.contains(tag)
    }

    /// Check if structural_tags contains the given tag.
    pub fn has_structural_tag(&self, tag: &Tag) -> bool {
        self.structural_tags.contains(tag)
    }

    /// Check if atmosphere_tags contains the given tag.
    pub fn has_atmosphere_tag(&self, tag: &Tag) -> bool {
        self.atmosphere_tags.contains(tag)
    }

    /// Returns the effective max connectors for this space.
    /// Defaults: Street/Alley = 10, Plaza = 8, Hall/Courtyard = 6, others = 4.
    pub fn effective_max_connectors(&self) -> u32 {
        if let Some(max) = self.max_connectors {
            return max;
        }
        match self.archetype {
            Some(SpaceArchetype::Street) | Some(SpaceArchetype::Alley) => 10,
            Some(SpaceArchetype::Plaza) => 8,
            Some(SpaceArchetype::Hall) | Some(SpaceArchetype::Courtyard) => 6,
            _ => 4,
        }
    }

    /// Returns the effective connector distribution for this space.
    /// Defaults: Street = Uniform, Plaza = Clustered, Corridor/Alley = Ends, Warehouse = GridAligned, others = None.
    pub fn effective_connector_distribution(&self) -> Option<ConnectorDistribution> {
        if self.connector_distribution.is_some() {
            return self.connector_distribution;
        }
        match self.archetype {
            Some(SpaceArchetype::Street) => Some(ConnectorDistribution::Uniform),
            Some(SpaceArchetype::Plaza) => Some(ConnectorDistribution::Clustered),
            Some(SpaceArchetype::Corridor) | Some(SpaceArchetype::Alley) => {
                Some(ConnectorDistribution::Ends)
            }
            Some(SpaceArchetype::Warehouse) => Some(ConnectorDistribution::GridAligned),
            _ => None,
        }
    }
}

/// Known structural tags — these trigger gameplay mechanics and should be
/// in `structural_tags`, not `atmosphere_tags`.
pub const STRUCTURAL_TAG_NAMES: &[&str] = &[
    "main_goal",
    "locked",
    "optional",
    "contains_key",
    "secret",
    "hidden",
    "rest_point",
];

/// Classify a list of raw tags into (structural, atmosphere) buckets.
/// Tags whose name is in `STRUCTURAL_TAG_NAMES` go to structural, all others to atmosphere.
pub fn classify_tags(raw_tags: &[Tag]) -> (Vec<Tag>, Vec<Tag>) {
    let mut structural = Vec::new();
    let mut atmosphere = Vec::new();
    for tag in raw_tags {
        if STRUCTURAL_TAG_NAMES.contains(&tag.0.as_str()) {
            structural.push(tag.clone());
        } else {
            atmosphere.push(tag.clone());
        }
    }
    (structural, atmosphere)
}

#[derive(Debug, Clone)]
pub struct SpaceLink {
    pub from: SpaceId,
    pub to: SpaceId,
    pub role: EdgeRole,
    /// Optional tags for link-specific metadata (e.g. "hidden", "trapped").
    pub tags: Vec<Tag>,
}

#[derive(Debug, Clone)]
pub struct SpatialPlan {
    pub spaces: Vec<SpaceSpec>,
    pub links: Vec<SpaceLink>,
    /// Constraints derived from the intent or inferred by the planner.
    pub constraints: Vec<SpatialConstraint>,
    /// The map's location kind — propagated from MapIntent for downstream use
    /// (e.g. geometry shape refinement, feature placement).
    pub location_kind: crate::intent::map_intent::LocationKind,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::graph::{NodeRole, ScenarioNodeId};

    /// Helper: build a minimal SpaceSpec with the given archetype.
    fn make_spec_with_archetype(archetype: Option<SpaceArchetype>) -> SpaceSpec {
        SpaceSpec {
            id: SpaceId(0),
            origin: ScenarioNodeId(0),
            role: NodeRole::Hub,
            structural_tags: vec![],
            atmosphere_tags: vec![],
            motifs: vec![],
            style: RealizationStyle::RoomLike,
            kind: SpaceKind::Atomic(AtomicSpace {
                width: 5,
                height: 5,
            }),
            label: None,
            archetype,
            size_hint: SizeHint::Medium,
            max_connectors: None,
            connector_distribution: None,
        }
    }

    // --- effective_max_connectors tests ---

    #[test]
    fn effective_max_connectors_street() {
        let spec = make_spec_with_archetype(Some(SpaceArchetype::Street));
        assert_eq!(spec.effective_max_connectors(), 10);
    }

    #[test]
    fn effective_max_connectors_alley() {
        let spec = make_spec_with_archetype(Some(SpaceArchetype::Alley));
        assert_eq!(spec.effective_max_connectors(), 10);
    }

    #[test]
    fn effective_max_connectors_plaza() {
        let spec = make_spec_with_archetype(Some(SpaceArchetype::Plaza));
        assert_eq!(spec.effective_max_connectors(), 8);
    }

    #[test]
    fn effective_max_connectors_hall() {
        let spec = make_spec_with_archetype(Some(SpaceArchetype::Hall));
        assert_eq!(spec.effective_max_connectors(), 6);
    }

    #[test]
    fn effective_max_connectors_courtyard() {
        let spec = make_spec_with_archetype(Some(SpaceArchetype::Courtyard));
        assert_eq!(spec.effective_max_connectors(), 6);
    }

    #[test]
    fn effective_max_connectors_chamber_default() {
        let spec = make_spec_with_archetype(Some(SpaceArchetype::Chamber));
        assert_eq!(spec.effective_max_connectors(), 4);
    }

    #[test]
    fn effective_max_connectors_none_archetype() {
        let spec = make_spec_with_archetype(None);
        assert_eq!(spec.effective_max_connectors(), 4);
    }

    #[test]
    fn effective_max_connectors_explicit_override() {
        let mut spec = make_spec_with_archetype(Some(SpaceArchetype::Street));
        spec.max_connectors = Some(2);
        // Explicit override takes precedence over archetype default (10).
        assert_eq!(spec.effective_max_connectors(), 2);
    }

    // --- effective_connector_distribution tests ---

    #[test]
    fn effective_connector_distribution_street() {
        let spec = make_spec_with_archetype(Some(SpaceArchetype::Street));
        assert_eq!(
            spec.effective_connector_distribution(),
            Some(ConnectorDistribution::Uniform)
        );
    }

    #[test]
    fn effective_connector_distribution_plaza() {
        let spec = make_spec_with_archetype(Some(SpaceArchetype::Plaza));
        assert_eq!(
            spec.effective_connector_distribution(),
            Some(ConnectorDistribution::Clustered)
        );
    }

    #[test]
    fn effective_connector_distribution_corridor() {
        let spec = make_spec_with_archetype(Some(SpaceArchetype::Corridor));
        assert_eq!(
            spec.effective_connector_distribution(),
            Some(ConnectorDistribution::Ends)
        );
    }

    #[test]
    fn effective_connector_distribution_alley() {
        let spec = make_spec_with_archetype(Some(SpaceArchetype::Alley));
        assert_eq!(
            spec.effective_connector_distribution(),
            Some(ConnectorDistribution::Ends)
        );
    }

    #[test]
    fn effective_connector_distribution_warehouse() {
        let spec = make_spec_with_archetype(Some(SpaceArchetype::Warehouse));
        assert_eq!(
            spec.effective_connector_distribution(),
            Some(ConnectorDistribution::GridAligned)
        );
    }

    #[test]
    fn effective_connector_distribution_chamber_none() {
        let spec = make_spec_with_archetype(Some(SpaceArchetype::Chamber));
        assert_eq!(spec.effective_connector_distribution(), None);
    }

    #[test]
    fn effective_connector_distribution_none_archetype() {
        let spec = make_spec_with_archetype(None);
        assert_eq!(spec.effective_connector_distribution(), None);
    }

    #[test]
    fn effective_connector_distribution_explicit_override() {
        let mut spec = make_spec_with_archetype(Some(SpaceArchetype::Chamber));
        spec.connector_distribution = Some(ConnectorDistribution::Uniform);
        // Explicit override takes precedence over archetype default (None).
        assert_eq!(
            spec.effective_connector_distribution(),
            Some(ConnectorDistribution::Uniform)
        );
    }
}
