use crate::intent::graph::{EdgeRole, NodeRole, ScenarioNodeId};
use crate::tag::Tag;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpaceId(pub u32);

// --- Archetype: what kind of space this is architecturally ---

/// Architectural archetype — describes the *character* of a space
/// independent of its role in the scenario graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    pub tags: Vec<Tag>,
    pub style: RealizationStyle,
    pub kind: SpaceKind,
    pub label: Option<String>,
    /// Architectural archetype — informs geometry and feature placement.
    pub archetype: Option<SpaceArchetype>,
    /// Size hint — guides the geometry planner's dimension choices.
    pub size_hint: SizeHint,
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
}
