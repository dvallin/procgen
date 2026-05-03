//! Feature validation: checks a `FeaturePlan` for correctness against
//! the tile map, geometry plan, and spatial plan.
//!
//! Implemented checks:
//! - **Solid tiles**: no feature cell sits on a solid (Wall/Void) tile.
//! - **Door blocking**: no feature cell sits on a Door or LockedDoor tile.
//! - **Overlap**: no two features share a cell.
//! - **Room bounds**: every feature cell falls within its room's rect.
//! - **Required features**: every room has the required features per its rules.

use std::collections::HashMap;

use crate::feature::plan::FeaturePlan;
use crate::feature::rules::{default_rules, matching_rules};
use crate::geometry::geom::{GeometryPlan, Point};
use crate::spatial::plan::{SpaceId, SpatialPlan};
use crate::tile::map::TileMap;
use crate::tile::registry::Tile;
use crate::validate::{Severity, ValidationIssue, ValidationResult, Validator};

/// Input bundle for feature validation — references all the layers the
/// validator needs to cross-check.
pub struct FeatureValidationInput<'a> {
    pub features: &'a FeaturePlan,
    pub tiles: &'a TileMap,
    pub geometry: &'a GeometryPlan,
    pub spatial: &'a SpatialPlan,
}

/// Validates a [`FeaturePlan`] for correctness.
#[derive(Debug, Default)]
pub struct FeatureValidator;

impl<'a> Validator<FeatureValidationInput<'a>> for FeatureValidator {
    fn validate(&self, input: &FeatureValidationInput<'a>) -> ValidationResult {
        let mut issues = Vec::new();

        check_solid_tiles(input.features, input.tiles, &mut issues);
        check_door_blocking(input.features, input.tiles, &mut issues);
        check_overlap(input.features, &mut issues);
        check_room_bounds(input.features, input.geometry, &mut issues);
        check_required_features(input.features, input.spatial, input.geometry, &mut issues);
        check_connectivity(input.features, input.tiles, &mut issues);

        ValidationResult { issues }
    }
}

/// Error: no feature cell should sit on a solid tile (Wall or Void).
fn check_solid_tiles(plan: &FeaturePlan, tiles: &TileMap, issues: &mut Vec<ValidationIssue>) {
    for f in &plan.features {
        for &cell in &f.cells {
            if let Some(tile) = tiles.get(cell.x, cell.y)
                && tile.is_solid()
            {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    message: format!(
                        "feature '{}' at ({}, {}) in space {:?} is on a solid tile ({:?})",
                        f.feature_type, cell.x, cell.y, f.space_id, tile
                    ),
                });
            }
        }
    }
}

/// Error: no feature cell should sit on a door tile.
fn check_door_blocking(plan: &FeaturePlan, tiles: &TileMap, issues: &mut Vec<ValidationIssue>) {
    for f in &plan.features {
        for &cell in &f.cells {
            if let Some(tile) = tiles.get(cell.x, cell.y)
                && (tile == Tile::DOOR || tile == Tile::LOCKED_DOOR)
            {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    message: format!(
                        "feature '{}' at ({}, {}) in space {:?} is blocking a door",
                        f.feature_type, cell.x, cell.y, f.space_id
                    ),
                });
            }
        }
    }
}

/// Error: no two features should share a cell.
fn check_overlap(plan: &FeaturePlan, issues: &mut Vec<ValidationIssue>) {
    let mut seen: HashMap<Point, usize> = HashMap::new();
    for (i, f) in plan.features.iter().enumerate() {
        for &cell in &f.cells {
            if let Some(&prev) = seen.get(&cell) {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    message: format!(
                        "feature '{}' (index {}) and '{}' (index {}) overlap at ({}, {})",
                        plan.features[prev].feature_type, prev, f.feature_type, i, cell.x, cell.y
                    ),
                });
            } else {
                seen.insert(cell, i);
            }
        }
    }
}

/// Warning: every feature cell should fall within its room's rect.
fn check_room_bounds(
    plan: &FeaturePlan,
    geometry: &GeometryPlan,
    issues: &mut Vec<ValidationIssue>,
) {
    let rect_map: HashMap<SpaceId, _> = geometry
        .spaces
        .iter()
        .map(|s| (s.space_id, s.rect))
        .collect();

    for f in &plan.features {
        let Some(rect) = rect_map.get(&f.space_id) else {
            issues.push(ValidationIssue {
                severity: Severity::Warning,
                message: format!(
                    "feature '{}' references unknown space {:?}",
                    f.feature_type, f.space_id
                ),
            });
            continue;
        };

        for &cell in &f.cells {
            let in_bounds = cell.x >= rect.x
                && cell.x < rect.x + rect.w
                && cell.y >= rect.y
                && cell.y < rect.y + rect.h;
            if !in_bounds {
                issues.push(ValidationIssue {
                    severity: Severity::Warning,
                    message: format!(
                        "feature '{}' cell ({}, {}) is outside room {:?} bounds",
                        f.feature_type, cell.x, cell.y, f.space_id
                    ),
                });
            }
        }
    }
}

/// Warning: every room should have all required features placed.
fn check_required_features(
    plan: &FeaturePlan,
    spatial: &SpatialPlan,
    geometry: &GeometryPlan,
    issues: &mut Vec<ValidationIssue>,
) {
    let rules = default_rules();
    let spec_map: HashMap<SpaceId, _> = spatial.spaces.iter().map(|s| (s.id, s)).collect();

    for placed in &geometry.spaces {
        let Some(spec) = spec_map.get(&placed.space_id) else {
            continue;
        };

        let matched = matching_rules(&rules, spec);
        let room_features = plan.features_in_space(placed.space_id);

        for rule in matched {
            if !rule.required {
                continue;
            }
            let has_it = room_features
                .iter()
                .any(|f| f.feature_type == rule.feature_type);
            if !has_it {
                issues.push(ValidationIssue {
                    severity: Severity::Warning,
                    message: format!(
                        "space {:?} is missing required feature '{}'",
                        placed.space_id, rule.feature_type
                    ),
                });
            }
        }
    }
}

/// Error: all door/link tiles must remain reachable from each other when
/// blocking feature cells are treated as impassable.
///
/// This is the critical connectivity invariant: the dungeon must remain
/// traversable. Individual floor tiles becoming unreachable (e.g. behind
/// barrels in a corner) is fine — what matters is that every door can
/// reach every other door.
///
/// Note: `Decoration` features are purely cosmetic overlays and do not
/// block passage, so they are excluded from this check.
fn check_connectivity(plan: &FeaturePlan, tiles: &TileMap, issues: &mut Vec<ValidationIssue>) {
    use crate::feature::registry::FeatureRegistry;
    use std::collections::HashSet;

    let registry = FeatureRegistry::default_registry();

    // Collect cells from blocking features only (non-blocking features don't affect passage).
    let feature_cells: HashSet<Point> = plan
        .features
        .iter()
        .filter(|f| registry.is_blocking(&f.feature_type))
        .flat_map(|f| f.cells.iter().copied())
        .collect();

    if feature_cells.is_empty() {
        return;
    }

    // Find all door tiles — these are the link points between rooms.
    let mut door_tiles: Vec<Point> = Vec::new();
    for y in 0..tiles.height as i32 {
        for x in 0..tiles.width as i32 {
            if let Some(tile) = tiles.get(x, y)
                && (tile == Tile::DOOR || tile == Tile::LOCKED_DOOR)
            {
                door_tiles.push(Point { x, y });
            }
        }
    }

    if door_tiles.len() < 2 {
        return; // 0 or 1 doors — nothing to check.
    }

    // Flood fill from the first door, treating feature cells as impassable.
    let start = door_tiles[0];
    let reachable = flood_fill_excluding(tiles, start, &feature_cells);

    // Every other door must be in the reachable set.
    let unreachable_doors: Vec<&Point> = door_tiles
        .iter()
        .skip(1)
        .filter(|p| !reachable.contains(p))
        .collect();

    if !unreachable_doors.is_empty() {
        issues.push(ValidationIssue {
            severity: Severity::Error,
            message: format!(
                "features block door connectivity: {} door(s) are unreachable \
                 from other doors when blocking features are treated as impassable",
                unreachable_doors.len()
            ),
        });
    }
}

/// BFS flood fill that treats both solid tiles and feature cells as impassable.
fn flood_fill_excluding(
    tiles: &TileMap,
    start: Point,
    blocked: &std::collections::HashSet<Point>,
) -> std::collections::HashSet<Point> {
    use std::collections::{HashSet, VecDeque};

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    if let Some(tile) = tiles.get(start.x, start.y)
        && tile.is_walkable()
        && !blocked.contains(&start)
    {
        visited.insert(start);
        queue.push_back(start);
    }

    while let Some(pos) = queue.pop_front() {
        for neighbor in pos.cardinals() {
            if visited.contains(&neighbor) {
                continue;
            }
            if blocked.contains(&neighbor) {
                continue;
            }
            if let Some(tile) = tiles.get(neighbor.x, neighbor.y)
                && tile.is_walkable()
            {
                visited.insert(neighbor);
                queue.push_back(neighbor);
            }
        }
    }

    visited
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::plan::FeaturePlacement;
    use crate::feature::registry::FeatureType;
    use crate::geometry::geom::{Footprint, PlacedSpace, Rect};
    use crate::intent::graph::{NodeRole, ScenarioNodeId};
    use crate::spatial::plan::*;
    use crate::tile::registry::Tile;

    fn make_5x5_map() -> TileMap {
        let mut map = TileMap::new(5, 5);
        for y in 0..5i32 {
            for x in 0..5i32 {
                let tile = if x == 0 || x == 4 || y == 0 || y == 4 {
                    Tile::WALL
                } else {
                    Tile::FLOOR
                };
                map.set(x, y, tile);
            }
        }
        map.set(2, 0, Tile::DOOR);
        map
    }

    fn make_geometry() -> GeometryPlan {
        GeometryPlan {
            spaces: vec![PlacedSpace {
                space_id: SpaceId(0),
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: 5,
                    h: 5,
                },
                footprint: Footprint::Rect(Rect {
                    x: 0,
                    y: 0,
                    w: 5,
                    h: 5,
                }),
                style: RealizationStyle::RoomLike,
                label: None,
            }],
            links: vec![],
        }
    }

    fn make_spatial(role: NodeRole, tags: &[&str]) -> SpatialPlan {
        let raw_tags: Vec<crate::tag::Tag> =
            tags.iter().map(|s| crate::tag::Tag::from(*s)).collect();
        let (structural, atmosphere) = classify_tags(&raw_tags);
        SpatialPlan {
            spaces: vec![SpaceSpec {
                id: SpaceId(0),
                origin: ScenarioNodeId(0),
                role,
                structural_tags: structural,
                atmosphere_tags: atmosphere,
                motifs: vec![],
                style: RealizationStyle::RoomLike,
                kind: SpaceKind::Atomic(AtomicSpace {
                    width: 5,
                    height: 5,
                }),
                label: None,
                archetype: Some(SpaceArchetype::Chamber),
                size_hint: SizeHint::Medium,
            }],
            links: vec![],
            constraints: vec![],
        }
    }

    fn placement(feature_type: &str, x: i32, y: i32) -> FeaturePlacement {
        FeaturePlacement {
            feature_type: FeatureType::from(feature_type),
            anchor: Point { x, y },
            cells: vec![Point { x, y }],
            space_id: SpaceId(0),
            tags: vec![],
        }
    }

    #[test]
    fn valid_plan_passes() {
        let tiles = make_5x5_map();
        let geometry = make_geometry();
        let spatial = make_spatial(NodeRole::Hub, &[]);
        let features = FeaturePlan {
            features: vec![placement("table", 2, 2)],
        };

        let result = FeatureValidator.validate(&FeatureValidationInput {
            features: &features,
            tiles: &tiles,
            geometry: &geometry,
            spatial: &spatial,
        });

        assert!(
            result.is_ok(),
            "valid plan should pass, got: {:?}",
            result.issues
        );
    }

    #[test]
    fn feature_on_wall_is_error() {
        let tiles = make_5x5_map();
        let geometry = make_geometry();
        let spatial = make_spatial(NodeRole::Hub, &[]);
        let features = FeaturePlan {
            features: vec![placement("barrel", 0, 0)],
        };

        let result = FeatureValidator.validate(&FeatureValidationInput {
            features: &features,
            tiles: &tiles,
            geometry: &geometry,
            spatial: &spatial,
        });

        let errors: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        assert!(!errors.is_empty(), "feature on wall should be an error");
        assert!(
            errors[0].message.contains("solid"),
            "message should mention solid tile"
        );
    }

    #[test]
    fn feature_on_door_is_error() {
        let tiles = make_5x5_map();
        let geometry = make_geometry();
        let spatial = make_spatial(NodeRole::Hub, &[]);
        // Door is at (2, 0).
        let features = FeaturePlan {
            features: vec![placement("chest", 2, 0)],
        };

        let result = FeatureValidator.validate(&FeatureValidationInput {
            features: &features,
            tiles: &tiles,
            geometry: &geometry,
            spatial: &spatial,
        });

        let errors: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        assert!(!errors.is_empty(), "feature on door should be an error");
        assert!(
            errors[0].message.contains("door"),
            "message should mention door"
        );
    }

    #[test]
    fn overlapping_features_is_error() {
        let tiles = make_5x5_map();
        let geometry = make_geometry();
        let spatial = make_spatial(NodeRole::Hub, &[]);
        let features = FeaturePlan {
            features: vec![placement("table", 2, 2), placement("barrel", 2, 2)],
        };

        let result = FeatureValidator.validate(&FeatureValidationInput {
            features: &features,
            tiles: &tiles,
            geometry: &geometry,
            spatial: &spatial,
        });

        let errors: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        assert!(
            !errors.is_empty(),
            "overlapping features should produce an error"
        );
        assert!(
            errors[0].message.contains("overlap"),
            "message should mention overlap"
        );
    }

    #[test]
    fn feature_outside_room_is_warning() {
        let tiles = make_5x5_map();
        let geometry = make_geometry();
        let spatial = make_spatial(NodeRole::Hub, &[]);
        // Place a feature well outside the 5×5 room.
        let features = FeaturePlan {
            features: vec![placement("barrel", 10, 10)],
        };

        let result = FeatureValidator.validate(&FeatureValidationInput {
            features: &features,
            tiles: &tiles,
            geometry: &geometry,
            spatial: &spatial,
        });

        let warnings: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .collect();
        assert!(
            !warnings.is_empty(),
            "feature outside room bounds should be a warning"
        );
        assert!(
            warnings[0].message.contains("outside"),
            "message should mention outside bounds"
        );
    }

    #[test]
    fn missing_required_feature_is_warning() {
        let tiles = make_5x5_map();
        let geometry = make_geometry();
        // Reward role requires a Chest via default_rules.
        let spatial = make_spatial(NodeRole::Reward, &["optional"]);
        // Empty feature plan — chest is missing.
        let features = FeaturePlan::empty();

        let result = FeatureValidator.validate(&FeatureValidationInput {
            features: &features,
            tiles: &tiles,
            geometry: &geometry,
            spatial: &spatial,
        });

        let warnings: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .collect();
        assert!(
            !warnings.is_empty(),
            "missing required chest should be a warning"
        );
        assert!(
            warnings[0].message.contains("missing required"),
            "message should mention missing required feature, got: {}",
            warnings[0].message
        );
    }

    #[test]
    fn feature_blocking_corridor_is_error() {
        // Build a map with two rooms connected by a 1-wide corridor.
        // Room1: (0,0)-(4,4), Corridor at (5,2), Room2: (6,0)-(10,4)
        let mut map = TileMap::new(11, 5);
        // Room 1: walls + floor
        for y in 0..5i32 {
            for x in 0..5i32 {
                let tile = if x == 0 || x == 4 || y == 0 || y == 4 {
                    Tile::WALL
                } else {
                    Tile::FLOOR
                };
                map.set(x, y, tile);
            }
        }
        // Room 2: walls + floor
        for y in 0..5i32 {
            for x in 6..11i32 {
                let tile = if x == 6 || x == 10 || y == 0 || y == 4 {
                    Tile::WALL
                } else {
                    Tile::FLOOR
                };
                map.set(x, y, tile);
            }
        }
        // Corridor: single floor tile connecting the two rooms.
        map.set(4, 2, Tile::DOOR);
        map.set(5, 2, Tile::FLOOR);
        map.set(6, 2, Tile::DOOR);
        // Walls above and below corridor.
        map.set(5, 1, Tile::WALL);
        map.set(5, 3, Tile::WALL);

        let geometry = GeometryPlan {
            spaces: vec![PlacedSpace {
                space_id: SpaceId(0),
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: 11,
                    h: 5,
                },
                footprint: Footprint::Rect(Rect {
                    x: 0,
                    y: 0,
                    w: 11,
                    h: 5,
                }),
                style: RealizationStyle::RoomLike,
                label: None,
            }],
            links: vec![],
        };
        let spatial = make_spatial(NodeRole::Hub, &[]);

        // Place a feature on the corridor tile — blocks connectivity.
        let features = FeaturePlan {
            features: vec![FeaturePlacement {
                feature_type: FeatureType::from("barrel"),
                anchor: Point { x: 5, y: 2 },
                cells: vec![Point { x: 5, y: 2 }],
                space_id: SpaceId(0),
                tags: vec![],
            }],
        };

        let result = FeatureValidator.validate(&FeatureValidationInput {
            features: &features,
            tiles: &map,
            geometry: &geometry,
            spatial: &spatial,
        });

        let errors: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        let has_connectivity_error = errors.iter().any(|e| e.message.contains("connectivity"));
        assert!(
            has_connectivity_error,
            "feature blocking the only corridor should produce a connectivity error, got: {:?}",
            result.issues
        );
    }
}
