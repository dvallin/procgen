//! Data model for placed features — the output of the feature planning stage.

use serde::{Deserialize, Serialize};

use crate::geometry::geom::Point;
use crate::spatial::plan::SpaceId;
use crate::tag::Tag;

/// The kind of feature being placed. Kept flat for MVP —
/// inner type enums (FurnitureType, ContainerType, etc.) deferred to Phase 8.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeatureKind {
    /// Religious/ritual centerpiece (chapel, shrine).
    Altar,
    /// Burial container (crypt, tomb).
    Sarcophagus,
    /// Lootable container (vault, reward room).
    Chest,
    /// Storage/scenery barrel.
    Barrel,
    /// Wall-adjacent storage (pantry, library).
    Shelf,
    /// Functional furniture (tavern, study).
    Table,
    /// Hidden hazard.
    Trap,
    /// Catch-all for torches, statues, banners, etc.
    Decoration(String),
}

/// A single placed feature within a room.
#[derive(Debug, Clone)]
pub struct FeaturePlacement {
    /// What kind of feature this is.
    pub kind: FeatureKind,
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
