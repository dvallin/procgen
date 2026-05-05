//! Entity validation: checks an `EntityPlan` for correctness against
//! the tile map, geometry plan, spatial plan, and feature plan.
//!
//! Implemented checks:
//! - **Non-walkable tile**: no entity sits on a solid (Wall/Void) tile.
//! - **Feature overlap**: no entity sits on a feature cell.
//! - **Entity overlap**: no two entities share a position.
//! - **Room bounds**: every entity position falls within its room's rect.
//! - **Patrol walkability**: all patrol zone cells should be walkable.
//! - **Required entities**: rooms matched by rules with min_count > 0 have entities.
//! - **Door tile**: entities shouldn't sit on door tiles (soft warning).

use std::collections::{HashMap, HashSet};

use crate::entity::plan::EntityPlan;
use crate::entity::rules::{default_entity_rules, matching_entity_rules};
use crate::feature::plan::FeaturePlan;
use crate::geometry::geom::{GeometryPlan, Point};
use crate::spatial::plan::{SpaceId, SpatialPlan};
use crate::tile::map::TileMap;
use crate::tile::registry::Tile;
use crate::validate::{Severity, ValidationIssue, ValidationResult, Validator};

/// Input bundle for entity validation — references all the layers the
/// validator needs to cross-check.
pub struct EntityValidationInput<'a> {
    pub entities: &'a EntityPlan,
    pub features: &'a FeaturePlan,
    pub tiles: &'a TileMap,
    pub geometry: &'a GeometryPlan,
    pub spatial: &'a SpatialPlan,
}

/// Validates an [`EntityPlan`] for correctness.
#[derive(Debug, Default)]
pub struct EntityValidator;

impl<'a> Validator<EntityValidationInput<'a>> for EntityValidator {
    fn validate(&self, input: &EntityValidationInput<'a>) -> ValidationResult {
        let mut issues = Vec::new();

        check_walkable(input.entities, input.tiles, &mut issues);
        check_feature_overlap(input.entities, input.features, &mut issues);
        check_entity_overlap(input.entities, &mut issues);
        check_room_bounds(input.entities, input.geometry, &mut issues);
        check_patrol_walkability(input.entities, input.tiles, &mut issues);
        check_required_entities(input.entities, input.spatial, input.geometry, &mut issues);
        check_door_tile(input.entities, input.tiles, &mut issues);

        ValidationResult { issues }
    }
}

/// Error: no entity should sit on a non-walkable tile.
fn check_walkable(plan: &EntityPlan, tiles: &TileMap, issues: &mut Vec<ValidationIssue>) {
    for entity in &plan.entities {
        let pos = entity.position;
        if let Some(tile) = tiles.get(pos.x, pos.y)
            && tile.is_solid()
        {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                message: format!(
                    "entity \"{}\" at ({}, {}) in space {:?} is on a non-walkable tile ({:?})",
                    entity.archetype, pos.x, pos.y, entity.space_id, tile
                ),
            });
        }
    }
}

/// Error: no entity should overlap a feature cell.
fn check_feature_overlap(
    plan: &EntityPlan,
    features: &FeaturePlan,
    issues: &mut Vec<ValidationIssue>,
) {
    let feature_cells: HashSet<Point> = features
        .features
        .iter()
        .flat_map(|f| f.cells.iter().copied())
        .collect();

    for entity in &plan.entities {
        if feature_cells.contains(&entity.position) {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                message: format!(
                    "entity \"{}\" at ({}, {}) in space {:?} overlaps a feature cell",
                    entity.archetype, entity.position.x, entity.position.y, entity.space_id
                ),
            });
        }
    }
}

/// Error: no two entities should share a position.
fn check_entity_overlap(plan: &EntityPlan, issues: &mut Vec<ValidationIssue>) {
    let mut seen: HashMap<Point, usize> = HashMap::new();
    for (i, entity) in plan.entities.iter().enumerate() {
        if let Some(&prev) = seen.get(&entity.position) {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                message: format!(
                    "entity \"{}\" (index {}) and \"{}\" (index {}) overlap at ({}, {})",
                    plan.entities[prev].archetype,
                    prev,
                    entity.archetype,
                    i,
                    entity.position.x,
                    entity.position.y
                ),
            });
        } else {
            seen.insert(entity.position, i);
        }
    }
}

/// Warning: every entity position should fall within its room's rect.
fn check_room_bounds(
    plan: &EntityPlan,
    geometry: &GeometryPlan,
    issues: &mut Vec<ValidationIssue>,
) {
    let rect_map: HashMap<SpaceId, _> = geometry
        .spaces
        .iter()
        .map(|s| (s.space_id, s.rect))
        .collect();

    for entity in &plan.entities {
        let Some(rect) = rect_map.get(&entity.space_id) else {
            issues.push(ValidationIssue {
                severity: Severity::Warning,
                message: format!(
                    "entity \"{}\" references unknown space {:?}",
                    entity.archetype, entity.space_id
                ),
            });
            continue;
        };

        let pos = entity.position;
        let in_bounds = pos.x >= rect.x
            && pos.x < rect.x + rect.w
            && pos.y >= rect.y
            && pos.y < rect.y + rect.h;
        if !in_bounds {
            issues.push(ValidationIssue {
                severity: Severity::Warning,
                message: format!(
                    "entity \"{}\" at ({}, {}) is outside room {:?} bounds",
                    entity.archetype, pos.x, pos.y, entity.space_id
                ),
            });
        }
    }
}

/// Warning: all patrol zone cells should be walkable.
fn check_patrol_walkability(plan: &EntityPlan, tiles: &TileMap, issues: &mut Vec<ValidationIssue>) {
    for entity in &plan.entities {
        if let Some(zone) = &entity.patrol_zone {
            for &cell in zone {
                if let Some(tile) = tiles.get(cell.x, cell.y)
                    && !tile.is_walkable()
                {
                    issues.push(ValidationIssue {
                        severity: Severity::Warning,
                        message: format!(
                            "entity \"{}\" at ({}, {}) has non-walkable patrol zone cell ({}, {}) ({:?})",
                            entity.archetype, entity.position.x, entity.position.y,
                            cell.x, cell.y, tile
                        ),
                    });
                }
            }
        }
    }
}

/// Warning: rooms matched by rules with min_count > 0 should have entities placed.
fn check_required_entities(
    plan: &EntityPlan,
    spatial: &SpatialPlan,
    geometry: &GeometryPlan,
    issues: &mut Vec<ValidationIssue>,
) {
    let rules = default_entity_rules();
    let spec_map: HashMap<SpaceId, _> = spatial.spaces.iter().map(|s| (s.id, s)).collect();

    for placed in &geometry.spaces {
        let Some(spec) = spec_map.get(&placed.space_id) else {
            continue;
        };

        let matched = matching_entity_rules(&rules, spec, None);
        let room_entities = plan.entities_in_space(placed.space_id);

        for rule in matched {
            if rule.min_count == 0 {
                continue;
            }
            let count = room_entities
                .iter()
                .filter(|e| e.archetype == rule.archetype)
                .count() as u32;
            if count < rule.min_count {
                issues.push(ValidationIssue {
                    severity: Severity::Warning,
                    message: format!(
                        "space {:?} has {} \"{}\" entities but rule requires at least {}",
                        placed.space_id, count, rule.archetype, rule.min_count
                    ),
                });
            }
        }
    }
}

/// Warning: entities on door tiles may block passage.
fn check_door_tile(plan: &EntityPlan, tiles: &TileMap, issues: &mut Vec<ValidationIssue>) {
    for entity in &plan.entities {
        let pos = entity.position;
        if let Some(tile) = tiles.get(pos.x, pos.y)
            && (tile == Tile::DOOR || tile == Tile::LOCKED_DOOR)
        {
            issues.push(ValidationIssue {
                severity: Severity::Warning,
                message: format!(
                    "entity \"{}\" at ({}, {}) in space {:?} is on a door tile",
                    entity.archetype, pos.x, pos.y, entity.space_id
                ),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::plan::{EntityArchetypeId, EntityPlacement};
    use crate::feature::plan::FeaturePlacement;
    use crate::feature::registry::FeatureType;
    use crate::geometry::geom::*;
    use crate::intent::graph::{NodeRole, ScenarioNodeId};
    use crate::intent::map_intent::LocationKind;
    use crate::spatial::plan::*;
    use crate::tag::Tag;

    fn make_5x5_map() -> TileMap {
        let mut map = TileMap::new(7, 7);
        let rect = Rect {
            x: 1,
            y: 1,
            w: 5,
            h: 5,
        };
        for x in rect.x..rect.x + rect.w {
            map.set(x, rect.y, Tile::WALL);
            map.set(x, rect.y + rect.h - 1, Tile::WALL);
        }
        for y in rect.y..rect.y + rect.h {
            map.set(rect.x, y, Tile::WALL);
            map.set(rect.x + rect.w - 1, y, Tile::WALL);
        }
        for y in (rect.y + 1)..(rect.y + rect.h - 1) {
            for x in (rect.x + 1)..(rect.x + rect.w - 1) {
                map.set(x, y, Tile::FLOOR);
            }
        }
        map.set(5, 3, Tile::DOOR);
        map
    }

    fn make_geometry() -> GeometryPlan {
        GeometryPlan {
            spaces: vec![PlacedSpace {
                space_id: SpaceId(0),
                rect: Rect {
                    x: 1,
                    y: 1,
                    w: 5,
                    h: 5,
                },
                footprint: Footprint::Rect(Rect {
                    x: 1,
                    y: 1,
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
        let raw_tags: Vec<Tag> = tags.iter().map(|s| Tag::from(*s)).collect();
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
                archetype: Some(SpaceArchetype::Hall),
                size_hint: SizeHint::Medium,
            }],
            links: vec![],
            constraints: vec![],
            location_kind: LocationKind::Dungeon,
        }
    }

    fn make_entity(archetype: &str, x: i32, y: i32) -> EntityPlacement {
        EntityPlacement {
            archetype: EntityArchetypeId::from(archetype),
            name: None,
            position: Point { x, y },
            space_id: SpaceId(0),
            behavior_tags: vec![],
            patrol_zone: None,
        }
    }

    fn make_input<'a>(
        entities: &'a EntityPlan,
        features: &'a FeaturePlan,
        tiles: &'a TileMap,
        geometry: &'a GeometryPlan,
        spatial: &'a SpatialPlan,
    ) -> EntityValidationInput<'a> {
        EntityValidationInput {
            entities,
            features,
            tiles,
            geometry,
            spatial,
        }
    }

    #[test]
    fn valid_plan_passes() {
        let tiles = make_5x5_map();
        let geometry = make_geometry();
        let spatial = make_spatial(NodeRole::Hub, &["noble"]);
        let features = FeaturePlan::empty();
        let entities = EntityPlan {
            entities: vec![make_entity("skeleton", 3, 3)],
        };

        let result = EntityValidator.validate(&make_input(
            &entities, &features, &tiles, &geometry, &spatial,
        ));
        assert!(
            result.is_ok(),
            "valid entity plan should pass, got: {:?}",
            result.issues
        );
    }

    #[test]
    fn entity_on_wall_is_error() {
        let tiles = make_5x5_map();
        let geometry = make_geometry();
        let spatial = make_spatial(NodeRole::Hub, &["noble"]);
        let features = FeaturePlan::empty();
        let entities = EntityPlan {
            entities: vec![make_entity("skeleton", 1, 1)], // wall tile
        };

        let result = EntityValidator.validate(&make_input(
            &entities, &features, &tiles, &geometry, &spatial,
        ));
        let errors: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        assert!(!errors.is_empty(), "entity on wall should produce an error");
        assert!(errors[0].message.contains("non-walkable"));
    }

    #[test]
    fn entity_on_feature_is_error() {
        let tiles = make_5x5_map();
        let geometry = make_geometry();
        let spatial = make_spatial(NodeRole::Hub, &["noble"]);
        let features = FeaturePlan {
            features: vec![FeaturePlacement {
                feature_type: FeatureType::from("barrel"),
                anchor: Point { x: 3, y: 3 },
                cells: vec![Point { x: 3, y: 3 }],
                space_id: SpaceId(0),
                tags: vec![],
            }],
        };
        let entities = EntityPlan {
            entities: vec![make_entity("skeleton", 3, 3)], // same as barrel
        };

        let result = EntityValidator.validate(&make_input(
            &entities, &features, &tiles, &geometry, &spatial,
        ));
        let errors: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        assert!(
            !errors.is_empty(),
            "entity on feature cell should produce an error"
        );
        assert!(errors[0].message.contains("overlaps a feature"));
    }

    #[test]
    fn overlapping_entities_is_error() {
        let tiles = make_5x5_map();
        let geometry = make_geometry();
        let spatial = make_spatial(NodeRole::Hub, &["noble"]);
        let features = FeaturePlan::empty();
        let entities = EntityPlan {
            entities: vec![
                make_entity("skeleton", 3, 3),
                make_entity("rat", 3, 3), // same position
            ],
        };

        let result = EntityValidator.validate(&make_input(
            &entities, &features, &tiles, &geometry, &spatial,
        ));
        let errors: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        assert!(
            !errors.is_empty(),
            "overlapping entities should produce an error"
        );
        assert!(errors[0].message.contains("overlap"));
    }

    #[test]
    fn entity_outside_room_is_warning() {
        let tiles = make_5x5_map();
        let geometry = make_geometry();
        let spatial = make_spatial(NodeRole::Hub, &["noble"]);
        let features = FeaturePlan::empty();
        let entities = EntityPlan {
            entities: vec![make_entity("skeleton", 0, 0)], // outside room rect
        };

        let result = EntityValidator.validate(&make_input(
            &entities, &features, &tiles, &geometry, &spatial,
        ));
        let warnings: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .collect();
        assert!(
            !warnings.is_empty(),
            "entity outside room should produce a warning"
        );
        assert!(warnings[0].message.contains("outside room"));
    }

    #[test]
    fn entity_on_door_is_warning() {
        let tiles = make_5x5_map();
        let geometry = make_geometry();
        let spatial = make_spatial(NodeRole::Hub, &["noble"]);
        let features = FeaturePlan::empty();
        let entities = EntityPlan {
            entities: vec![make_entity("skeleton", 5, 3)], // door tile
        };

        let result = EntityValidator.validate(&make_input(
            &entities, &features, &tiles, &geometry, &spatial,
        ));
        // Should be a warning, NOT an error (entities can move).
        let warnings: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .collect();
        assert!(
            warnings.iter().any(|w| w.message.contains("door")),
            "entity on door should produce a warning about door"
        );
        // Should not trigger the non-walkable error (doors are walkable).
        let errors: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "entity on door should not produce an error (doors are walkable)"
        );
    }

    #[test]
    fn patrol_zone_with_wall_is_warning() {
        let tiles = make_5x5_map();
        let geometry = make_geometry();
        let spatial = make_spatial(NodeRole::Hub, &["noble"]);
        let features = FeaturePlan::empty();
        let entities = EntityPlan {
            entities: vec![EntityPlacement {
                archetype: EntityArchetypeId::from("skeleton"),
                name: None,
                position: Point { x: 3, y: 3 },
                space_id: SpaceId(0),
                behavior_tags: vec![],
                patrol_zone: Some(vec![
                    Point { x: 3, y: 3 },
                    Point { x: 1, y: 1 }, // wall tile — invalid patrol cell
                ]),
            }],
        };

        let result = EntityValidator.validate(&make_input(
            &entities, &features, &tiles, &geometry, &spatial,
        ));
        let warnings: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .collect();
        assert!(
            warnings.iter().any(|w| w.message.contains("patrol zone")),
            "non-walkable patrol cell should produce a warning"
        );
    }
}
