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
use crate::tile::map::{Tile, TileMap};
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
                        "feature {:?} at ({}, {}) in space {:?} is on a solid tile ({:?})",
                        f.kind, cell.x, cell.y, f.space_id, tile
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
                && matches!(tile, Tile::Door | Tile::LockedDoor)
            {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    message: format!(
                        "feature {:?} at ({}, {}) in space {:?} is blocking a door",
                        f.kind, cell.x, cell.y, f.space_id
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
                        "feature {:?} (index {}) and {:?} (index {}) overlap at ({}, {})",
                        plan.features[prev].kind, prev, f.kind, i, cell.x, cell.y
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
                    "feature {:?} references unknown space {:?}",
                    f.kind, f.space_id
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
                        "feature {:?} cell ({}, {}) is outside room {:?} bounds",
                        f.kind, cell.x, cell.y, f.space_id
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
            let has_it = room_features.iter().any(|f| f.kind == rule.kind);
            if !has_it {
                issues.push(ValidationIssue {
                    severity: Severity::Warning,
                    message: format!(
                        "space {:?} is missing required feature {:?}",
                        placed.space_id, rule.kind
                    ),
                });
            }
        }
    }
}

/// Error: the map must remain fully connected when feature cells are
/// treated as impassable. A feature that cuts a corridor in half would
/// make parts of the dungeon unreachable.
fn check_connectivity(plan: &FeaturePlan, tiles: &TileMap, issues: &mut Vec<ValidationIssue>) {
    use std::collections::HashSet;

    // Collect all feature cells into a set.
    let feature_cells: HashSet<Point> = plan
        .features
        .iter()
        .flat_map(|f| f.cells.iter().copied())
        .collect();

    if feature_cells.is_empty() {
        return;
    }

    // Find all walkable tiles that are NOT blocked by features.
    let mut all_walkable: Vec<Point> = Vec::new();
    for y in 0..tiles.height as i32 {
        for x in 0..tiles.width as i32 {
            let p = Point { x, y };
            if let Some(tile) = tiles.get(x, y)
                && tile.is_walkable()
                && !feature_cells.contains(&p)
            {
                all_walkable.push(p);
            }
        }
    }

    if all_walkable.is_empty() {
        return;
    }

    // Flood fill from the first walkable non-feature tile, treating
    // feature cells as impassable.
    let start = all_walkable[0];
    let reachable_with_features = flood_fill_excluding(tiles, start, &feature_cells);

    let unreachable_count = all_walkable
        .iter()
        .filter(|p| !reachable_with_features.contains(p))
        .count();

    if unreachable_count > 0 {
        issues.push(ValidationIssue {
            severity: Severity::Error,
            message: format!(
                "features block connectivity: {} walkable tile(s) are unreachable \
                 when feature cells are treated as impassable",
                unreachable_count
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
    use crate::feature::plan::{FeatureKind, FeaturePlacement};
    use crate::geometry::geom::{Footprint, PlacedSpace, Rect};
    use crate::intent::graph::{NodeRole, ScenarioNodeId};
    use crate::spatial::plan::*;
    use crate::tile::map::Tile;

    fn make_5x5_map() -> TileMap {
        let mut map = TileMap::new(5, 5);
        for y in 0..5i32 {
            for x in 0..5i32 {
                let tile = if x == 0 || x == 4 || y == 0 || y == 4 {
                    Tile::Wall
                } else {
                    Tile::Floor
                };
                map.set(x, y, tile);
            }
        }
        map.set(2, 0, Tile::Door);
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
        SpatialPlan {
            spaces: vec![SpaceSpec {
                id: SpaceId(0),
                origin: ScenarioNodeId(0),
                role,
                tags: tags.iter().map(|s| crate::tag::Tag::from(*s)).collect(),
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

    fn placement(kind: FeatureKind, x: i32, y: i32) -> FeaturePlacement {
        FeaturePlacement {
            kind,
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
            features: vec![placement(FeatureKind::Table, 2, 2)],
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
            features: vec![placement(FeatureKind::Barrel, 0, 0)],
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
            features: vec![placement(FeatureKind::Chest, 2, 0)],
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
            features: vec![
                placement(FeatureKind::Table, 2, 2),
                placement(FeatureKind::Barrel, 2, 2),
            ],
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
            features: vec![placement(FeatureKind::Barrel, 10, 10)],
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
                    Tile::Wall
                } else {
                    Tile::Floor
                };
                map.set(x, y, tile);
            }
        }
        // Room 2: walls + floor
        for y in 0..5i32 {
            for x in 6..11i32 {
                let tile = if x == 6 || x == 10 || y == 0 || y == 4 {
                    Tile::Wall
                } else {
                    Tile::Floor
                };
                map.set(x, y, tile);
            }
        }
        // Corridor: single floor tile connecting the two rooms.
        map.set(4, 2, Tile::Door);
        map.set(5, 2, Tile::Floor);
        map.set(6, 2, Tile::Door);
        // Walls above and below corridor.
        map.set(5, 1, Tile::Wall);
        map.set(5, 3, Tile::Wall);

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
                kind: FeatureKind::Barrel,
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
