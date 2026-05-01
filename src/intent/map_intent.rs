use crate::intent::graph::ScenarioGraph;
use crate::tag::Tag;

/// What kind of location this map represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationKind {
    Dungeon,
    Building,
    Cave,
    Outdoors,
    Sewer,
}

/// Rough scale of the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapScale {
    Tiny,   // 3-5 spaces
    Small,  // 5-8 spaces
    Medium, // 8-15 spaces
    Large,  // 15-25 spaces
    Huge,   // 25+ spaces
}

/// A motif that influences generation choices (style, decorations, etc).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MotifId(pub String);

impl From<&str> for MotifId {
    fn from(s: &str) -> Self {
        MotifId(s.to_string())
    }
}

/// A high-level constraint on the map structure or layout.
#[derive(Debug, Clone)]
pub enum IntentConstraint {
    /// Two spaces must be adjacent (connected by a single link).
    MustConnect { from: String, to: String },
    /// A space must not be directly reachable without passing through a gate.
    MustGate { space: String, gate: String },
    /// Global maximum depth from entry.
    MaxDepth(u32),
}

/// The complete map intent — what this map should contain and feel like.
/// Combines structural graph with non-structural metadata (scale, motifs, constraints).
#[derive(Debug, Clone)]
pub struct MapIntent {
    pub location_kind: LocationKind,
    pub scale: MapScale,
    pub tags: Vec<Tag>,
    pub motifs: Vec<MotifId>,
    pub structural_graph: ScenarioGraph,
    pub constraints: Vec<IntentConstraint>,
}
