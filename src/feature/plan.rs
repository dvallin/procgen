//! Data model for placed features — the output of the feature planning stage.

use crate::feature::registry::FeatureType;
use crate::geometry::geom::Point;
use crate::spatial::plan::SpaceId;
use crate::tag::Tag;

/// A single placed feature within a room.
#[derive(Debug, Clone)]
pub struct FeaturePlacement {
    /// What type of feature this is (data-driven identifier).
    pub feature_type: FeatureType,
    /// The primary interaction point / origin cell.
    pub anchor: Point,
    /// All tiles occupied by this feature (for single-cell features, `cells == vec![anchor]`).
    pub cells: Vec<Point>,
    /// Which room this feature belongs to.
    pub space_id: SpaceId,
    /// Optional tags for contextual metadata (e.g. "locked", "trapped", "ornate").
    pub tags: Vec<Tag>,
}

/// The complete set of placed features for a generated map.
#[derive(Debug, Clone)]
pub struct FeaturePlan {
    pub features: Vec<FeaturePlacement>,
}

impl FeaturePlan {
    /// Creates an empty feature plan.
    pub fn empty() -> Self {
        Self {
            features: Vec::new(),
        }
    }

    /// Returns all features placed in a specific room.
    pub fn features_in_space(&self, space_id: SpaceId) -> Vec<&FeaturePlacement> {
        self.features
            .iter()
            .filter(|f| f.space_id == space_id)
            .collect()
    }
}

/// Errors that can occur during feature placement.
#[derive(Debug, Clone)]
pub enum FeaturePlanError {
    /// A required feature could not be placed because the room has no suitable walkable tiles.
    NoWalkableTiles { space_id: SpaceId },
    /// A feature placement conflicts with an already-occupied position.
    PlacementConflict { space_id: SpaceId, position: Point },
}

impl std::fmt::Display for FeaturePlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoWalkableTiles { space_id } => {
                write!(
                    f,
                    "no walkable tiles available for required feature in space {:?}",
                    space_id
                )
            }
            Self::PlacementConflict { space_id, position } => {
                write!(
                    f,
                    "feature placement conflict at ({}, {}) in space {:?}",
                    position.x, position.y, space_id
                )
            }
        }
    }
}

impl std::error::Error for FeaturePlanError {}

// ─── Backward compatibility ─────────────────────────────────────────────────

/// Named constants for common feature types (backward compatibility).
///
/// This provides a migration path from the old `FeatureKind` enum.
/// Use these constants wherever you previously matched on enum variants.
pub struct Feature;

#[allow(non_upper_case_globals)]
impl Feature {
    pub const ALTAR: &'static str = "altar";
    pub const SARCOPHAGUS: &'static str = "sarcophagus";
    pub const CHEST: &'static str = "chest";
    pub const BARREL: &'static str = "barrel";
    pub const SHELF: &'static str = "shelf";
    pub const TABLE: &'static str = "table";
    pub const TRAP: &'static str = "trap";
}
