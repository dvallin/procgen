//! Data model for interior plans — spatial partitions of room interiors.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::geometry::geom::{Point, Rect};
use crate::spatial::plan::SpaceId;

/// A spatial partition of a room's interior.
#[derive(Debug, Clone)]
pub struct InteriorPlan {
    /// Which room this plan belongs to.
    pub space_id: SpaceId,
    /// The room's bounding rect (from GeometryPlan).
    pub rect: Rect,
    /// Classified zones within the room.
    pub zones: Vec<Zone>,
    /// Doors belonging to this room (on its boundary).
    pub doors: Vec<Point>,
    /// Reserved path cells: door-to-door routes that must never be blocked.
    /// This is the union of all PathIntent cells.
    pub reserved_paths: HashSet<Point>,
    /// Individual door-to-door path intents.
    pub path_intents: Vec<PathIntent>,
}

impl InteriorPlan {
    /// Get all cells belonging to a specific zone kind.
    pub fn zone_cells(&self, kind: ZoneKind) -> Vec<&Point> {
        self.zones
            .iter()
            .filter(|z| z.kind == kind)
            .flat_map(|z| z.cells.iter())
            .collect()
    }

    /// Get available cells for a zone kind, excluding occupied and reserved cells.
    pub fn available_zone_cells(&self, kind: ZoneKind, occupied: &HashSet<Point>) -> Vec<Point> {
        self.zones
            .iter()
            .filter(|z| z.kind == kind)
            .flat_map(|z| z.cells.iter())
            .filter(|p| !occupied.contains(p))
            .filter(|p| !self.reserved_paths.contains(p))
            .copied()
            .collect()
    }

    /// Check if a point is on a reserved path.
    pub fn is_reserved(&self, point: &Point) -> bool {
        self.reserved_paths.contains(point)
    }
}

/// A classified region of a room's interior.
#[derive(Debug, Clone)]
pub struct Zone {
    /// What part of the room this zone represents.
    pub kind: ZoneKind,
    /// All cells belonging to this zone.
    pub cells: Vec<Point>,
}

/// What part of the room a zone represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ZoneKind {
    /// The geometric center area (1–4 cells for small rooms, larger for big ones).
    Center,
    /// Cells adjacent to at least one wall but not in a corner.
    WallBand,
    /// Cells near two perpendicular walls (within 2 tiles of each).
    Corner,
    /// Cells on a reserved door-to-door path.
    DoorPath,
    /// Remaining floor cells not classified above.
    Open,
}

/// A door-to-door traversal intent that must remain clear.
#[derive(Debug, Clone)]
pub struct PathIntent {
    /// Start door position.
    pub from: Point,
    /// End door position.
    pub to: Point,
    /// The computed walkable path cells (BFS shortest path), excluding the door cells themselves.
    pub cells: Vec<Point>,
}
