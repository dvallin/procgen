//! Entity planner — transforms spatial + geometry + tile + feature data into placed entities.
//!
//! The [`EntityPlanner`] trait defines the boundary. [`SimpleEntityPlanner`]
//! is the MVP implementation: it matches [`default_entity_rules`](super::rules::default_entity_rules)
//! against each room's role/archetype/tags and uses placement strategies to
//! find positions.

use std::collections::{HashMap, HashSet};

use rand::seq::SliceRandom;
use tracing::{debug, info, info_span};

use crate::entity::plan::*;
use crate::entity::rules::*;
use crate::feature::plan::{FeatureKind, FeaturePlan};
use crate::geometry::geom::{GeometryPlan, Point, Rect};
use crate::intent::graph::NodeRole;
use crate::spatial::plan::{SpaceId, SpaceSpec, SpatialPlan};
use crate::tile::map::TileMap;
use crate::tile::registry::Tile;

/// Trait for producing an [`EntityPlan`] from upstream layers.
pub trait EntityPlanner {
    /// Plan entity placements.
    ///
    /// Reads room roles/tags from [`SpatialPlan`], room positions from
    /// [`GeometryPlan`], tile walkability from [`TileMap`], and occupied
    /// cells from [`FeaturePlan`].
    /// The `rules` slice determines which entities get spawned in which rooms.
    fn plan(
        &self,
        spatial: &SpatialPlan,
        geometry: &GeometryPlan,
        tiles: &TileMap,
        features: &FeaturePlan,
        rules: &[EntityRule],
        rng: &mut dyn rand::RngCore,
    ) -> Result<EntityPlan, EntityPlanError>;
}

/// Simple rule-driven entity planner.
///
/// For each room in the geometry plan, it looks up the matching
/// [`SpaceSpec`], finds applicable [`EntityRule`]s, and uses the
/// associated [`EntityPlacementStrategy`] to find tile positions.
#[derive(Debug, Default)]
pub struct SimpleEntityPlanner;

impl EntityPlanner for SimpleEntityPlanner {
    fn plan(
        &self,
        spatial: &SpatialPlan,
        geometry: &GeometryPlan,
        tiles: &TileMap,
        features: &FeaturePlan,
        rules: &[EntityRule],
        rng: &mut dyn rand::RngCore,
    ) -> Result<EntityPlan, EntityPlanError> {
        let _span = info_span!("entity_planning", rooms = geometry.spaces.len()).entered();

        // Build SpaceId → SpaceSpec lookup.
        let spec_map: HashMap<SpaceId, &SpaceSpec> =
            spatial.spaces.iter().map(|s| (s.id, s)).collect();

        // Build the global occupied set pre-seeded from feature cells.
        let mut global_occupied: HashSet<Point> = HashSet::new();
        for feature in &features.features {
            for &cell in &feature.cells {
                global_occupied.insert(cell);
            }
        }

        // Build feature position lookup for NearFeature strategy.
        let feature_positions: HashMap<SpaceId, Vec<(FeatureKind, Point)>> = {
            let mut map: HashMap<SpaceId, Vec<(FeatureKind, Point)>> = HashMap::new();
            for f in &features.features {
                map.entry(f.space_id)
                    .or_default()
                    .push((f.kind.clone(), f.anchor));
            }
            map
        };

        let mut entities = Vec::new();

        for placed in &geometry.spaces {
            let Some(spec) = spec_map.get(&placed.space_id) else {
                debug!(
                    space_id = ?placed.space_id,
                    "no SpaceSpec found for placed space, skipping entity placement"
                );
                continue;
            };

            // Entry rooms get 0 entities by design — players need a safe spawn area.
            // This is a hard constraint regardless of tags or rules that might otherwise match.
            if spec.role == NodeRole::Entry {
                debug!(
                    space_id = ?placed.space_id,
                    label = ?placed.label,
                    "skipping entry room (safe spawn area)"
                );
                continue;
            }

            let matched = matching_entity_rules(rules, spec);
            if matched.is_empty() {
                debug!(
                    space_id = ?placed.space_id,
                    label = ?placed.label,
                    "no entity rules matched"
                );
                continue;
            }

            debug!(
                space_id = ?placed.space_id,
                label = ?placed.label,
                rule_count = matched.len(),
                "matched entity rules"
            );

            let rect = placed.rect;
            let density_cap = compute_density_cap(tiles, rect);

            // Per-room occupied starts from the global set.
            let mut room_occupied = global_occupied.clone();
            let mut room_entity_count = 0u32;

            let room_features = feature_positions
                .get(&placed.space_id)
                .cloned()
                .unwrap_or_default();

            let room_ctx = RoomContext {
                space_id: placed.space_id,
                rect,
                tiles,
                room_features: &room_features,
                density_cap,
            };

            for rule in &matched {
                let placed_count = place_entity_rule(
                    rule,
                    &room_ctx,
                    &mut room_occupied,
                    &mut entities,
                    room_entity_count,
                    rng,
                )?;

                room_entity_count += placed_count;

                if placed_count > 0 {
                    debug!(
                        space_id = ?placed.space_id,
                        archetype = %rule.archetype,
                        count = placed_count,
                        "placed entities"
                    );
                }
            }

            // Merge room placements into global set.
            global_occupied = room_occupied;
        }

        info!(total_entities = entities.len(), "entity planning complete");

        Ok(EntityPlan { entities })
    }
}

/// Compute the maximum number of entities allowed in a room.
/// Based on walkable floor tile count / 4 (minimum 1).
fn compute_density_cap(tiles: &TileMap, rect: Rect) -> u32 {
    let walkable = rect
        .iter_points()
        .filter(|p| tiles.get(p.x, p.y) == Some(Tile::FLOOR))
        .count() as u32;
    (walkable / 4).max(1)
}

/// Immutable context for a single room during entity placement.
struct RoomContext<'a> {
    space_id: SpaceId,
    rect: Rect,
    tiles: &'a TileMap,
    room_features: &'a [(FeatureKind, Point)],
    density_cap: u32,
}

/// Try to place up to `rule.max_count` entities in a room.
///
/// Returns the number of entities actually placed. If the rule's `min_count > 0`
/// and no candidate position could be found, returns an error.
fn place_entity_rule(
    rule: &EntityRule,
    ctx: &RoomContext,
    occupied: &mut HashSet<Point>,
    entities: &mut Vec<EntityPlacement>,
    current_count: u32,
    rng: &mut dyn rand::RngCore,
) -> Result<u32, EntityPlanError> {
    let mut placed = 0u32;

    for _ in 0..rule.max_count {
        // Check density cap before each placement attempt.
        if current_count + placed >= ctx.density_cap {
            if placed == 0 && rule.min_count > 0 {
                return Err(EntityPlanError::DensityExceeded {
                    space_id: ctx.space_id,
                    max: ctx.density_cap,
                    actual: current_count + rule.min_count,
                });
            }
            break;
        }

        let candidate = pick_entity_candidate(
            &rule.placement,
            ctx.tiles,
            ctx.rect,
            occupied,
            ctx.room_features,
            rng,
        );

        match candidate {
            Some(point) => {
                let patrol_zone = if rule.patrol {
                    Some(compute_patrol_zone(point, ctx.tiles, ctx.rect, occupied))
                } else {
                    None
                };

                occupied.insert(point);
                entities.push(EntityPlacement {
                    archetype: rule.archetype.clone(),
                    position: point,
                    space_id: ctx.space_id,
                    behavior_tags: rule.behavior_tags.clone(),
                    patrol_zone,
                });
                placed += 1;
            }
            None => {
                if placed == 0 && rule.min_count > 0 {
                    return Err(EntityPlanError::NoWalkableTiles {
                        space_id: ctx.space_id,
                    });
                }
                break;
            }
        }
    }

    Ok(placed)
}

/// Pick a single candidate point using the given strategy.
fn pick_entity_candidate(
    strategy: &EntityPlacementStrategy,
    tiles: &TileMap,
    rect: Rect,
    occupied: &HashSet<Point>,
    room_features: &[(FeatureKind, Point)],
    rng: &mut dyn rand::RngCore,
) -> Option<Point> {
    match strategy {
        EntityPlacementStrategy::Center => find_center_floor(tiles, rect, occupied),
        EntityPlacementStrategy::RandomFloor => {
            let mut candidates = find_walkable_floor(tiles, rect, occupied);
            candidates.shuffle(rng);
            pick_furthest_from_occupied(candidates, occupied)
        }
        EntityPlacementStrategy::NearEntrance => {
            let mut candidates = find_near_entrance(tiles, rect, occupied);
            candidates.shuffle(rng);
            if candidates.is_empty() {
                // Fallback to any floor tile.
                let mut fallback = find_walkable_floor(tiles, rect, occupied);
                fallback.shuffle(rng);
                pick_furthest_from_occupied(fallback, occupied)
            } else {
                // Pick the nearest-to-door candidate that isn't too close to occupied.
                Some(candidates[0])
            }
        }
        EntityPlacementStrategy::NearFeature(kind) => {
            let target = room_features
                .iter()
                .find(|(k, _)| k == kind)
                .map(|(_, p)| *p);
            match target {
                Some(feature_pos) => {
                    let mut candidates = find_walkable_floor(tiles, rect, occupied);
                    candidates.shuffle(rng);
                    pick_nearest_to(candidates, feature_pos)
                }
                None => {
                    // Feature not present — fallback to random floor.
                    let mut candidates = find_walkable_floor(tiles, rect, occupied);
                    candidates.shuffle(rng);
                    pick_furthest_from_occupied(candidates, occupied)
                }
            }
        }
    }
}

/// Find the walkable Floor tile closest to the room center.
fn find_center_floor(tiles: &TileMap, rect: Rect, occupied: &HashSet<Point>) -> Option<Point> {
    let center = rect.center();

    rect.iter_points()
        .filter(|p| !occupied.contains(p))
        .filter(|p| tiles.get(p.x, p.y) == Some(Tile::FLOOR))
        .min_by_key(|p| {
            let dx = p.x - center.x;
            let dy = p.y - center.y;
            dx * dx + dy * dy
        })
}

/// Find all walkable Floor tiles in the room rect that are not occupied.
fn find_walkable_floor(tiles: &TileMap, rect: Rect, occupied: &HashSet<Point>) -> Vec<Point> {
    rect.iter_points()
        .filter(|p| !occupied.contains(p))
        .filter(|p| tiles.get(p.x, p.y) == Some(Tile::FLOOR))
        .collect()
}

/// Find walkable Floor tiles adjacent to door tiles (near entrance).
fn find_near_entrance(tiles: &TileMap, rect: Rect, occupied: &HashSet<Point>) -> Vec<Point> {
    rect.iter_points()
        .filter(|p| !occupied.contains(p))
        .filter(|p| tiles.get(p.x, p.y) == Some(Tile::FLOOR))
        .filter(|p| {
            p.cardinals().iter().any(|n| {
                let t = tiles.get(n.x, n.y);
                t == Some(Tile::DOOR) || t == Some(Tile::LOCKED_DOOR)
            })
        })
        .collect()
}

/// From a list of candidates, pick the one with the greatest minimum
/// squared distance to any occupied cell.
fn pick_furthest_from_occupied(candidates: Vec<Point>, occupied: &HashSet<Point>) -> Option<Point> {
    if candidates.is_empty() {
        return None;
    }
    if occupied.is_empty() {
        return candidates.into_iter().next();
    }

    candidates.into_iter().max_by_key(|candidate| {
        occupied
            .iter()
            .map(|occ| {
                let dx = candidate.x - occ.x;
                let dy = candidate.y - occ.y;
                dx * dx + dy * dy
            })
            .min()
            .unwrap_or(i32::MAX)
    })
}

/// From a list of candidates, pick the one nearest to a target point.
fn pick_nearest_to(candidates: Vec<Point>, target: Point) -> Option<Point> {
    if candidates.is_empty() {
        return None;
    }
    candidates.into_iter().min_by_key(|p| {
        let dx = p.x - target.x;
        let dy = p.y - target.y;
        dx * dx + dy * dy
    })
}

/// Compute a patrol zone: all walkable tiles within Manhattan distance 2
/// of the entity position, within the room rect, excluding occupied cells.
fn compute_patrol_zone(
    origin: Point,
    tiles: &TileMap,
    rect: Rect,
    occupied: &HashSet<Point>,
) -> Vec<Point> {
    // Clip the Manhattan-2 diamond's bounding box to the room rect.
    let min_x = (origin.x - 2).max(rect.x);
    let min_y = (origin.y - 2).max(rect.y);
    let max_x = (origin.x + 2).min(rect.x + rect.w - 1);
    let max_y = (origin.y + 2).min(rect.y + rect.h - 1);

    let clipped = Rect {
        x: min_x,
        y: min_y,
        w: max_x - min_x + 1,
        h: max_y - min_y + 1,
    };

    let mut zone: Vec<Point> = clipped
        .iter_points()
        .filter(|p| (p.x - origin.x).abs() + (p.y - origin.y).abs() <= 2)
        .filter(|p| tiles.get(p.x, p.y).unwrap().is_walkable())
        .filter(|p| !occupied.contains(p))
        .collect();

    // Always include the origin itself.
    if !zone.contains(&origin) {
        zone.push(origin);
    }
    zone
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::rules::default_entity_rules;
    use crate::feature::plan::FeaturePlacement;
    use crate::geometry::geom::*;
    use crate::intent::graph::{NodeRole, ScenarioNodeId};
    use crate::spatial::plan::*;
    use crate::tag::Tag;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    /// Build a 7×7 TileMap with a single room (walls + floor + door).
    fn make_room_map() -> TileMap {
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

    fn make_spec(
        id: u32,
        role: NodeRole,
        archetype: Option<SpaceArchetype>,
        tags: &[&str],
    ) -> SpaceSpec {
        SpaceSpec {
            id: SpaceId(id),
            origin: ScenarioNodeId(id),
            role,
            tags: tags.iter().map(|s| Tag::from(*s)).collect(),
            style: RealizationStyle::RoomLike,
            kind: SpaceKind::Atomic(AtomicSpace {
                width: 5,
                height: 5,
            }),
            label: None,
            archetype,
            size_hint: SizeHint::Medium,
        }
    }

    fn make_placed(id: u32) -> PlacedSpace {
        PlacedSpace {
            space_id: SpaceId(id),
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
        }
    }

    #[test]
    fn hub_room_gets_skeletons() {
        let tiles = make_room_map();
        let spatial = SpatialPlan {
            spaces: vec![make_spec(
                0,
                NodeRole::Hub,
                Some(SpaceArchetype::Hall),
                &["noble"],
            )],
            links: vec![],
            constraints: vec![],
        };
        let geometry = GeometryPlan {
            spaces: vec![make_placed(0)],
            links: vec![],
        };
        let features = FeaturePlan::empty();
        let mut rng = StdRng::seed_from_u64(42);
        let rules = default_entity_rules();

        let plan = SimpleEntityPlanner
            .plan(&spatial, &geometry, &tiles, &features, &rules, &mut rng)
            .unwrap();

        let archetypes: Vec<&EntityArchetypeId> =
            plan.entities.iter().map(|e| &e.archetype).collect();
        assert!(
            archetypes.contains(&&EntityArchetypeId::from("skeleton")),
            "hub should get skeletons, got: {:?}",
            archetypes
        );
        // Verify patrol zones are present for skeletons.
        for entity in &plan.entities {
            if entity.archetype == EntityArchetypeId::from("skeleton") {
                assert!(
                    entity.patrol_zone.is_some(),
                    "skeletons should have patrol zones"
                );
                assert!(
                    !entity.patrol_zone.as_ref().unwrap().is_empty(),
                    "patrol zone should not be empty"
                );
            }
        }
    }

    #[test]
    fn goal_main_goal_gets_guardians() {
        let tiles = make_room_map();
        let spatial = SpatialPlan {
            spaces: vec![make_spec(
                0,
                NodeRole::Goal,
                Some(SpaceArchetype::Vault),
                &["main_goal"],
            )],
            links: vec![],
            constraints: vec![],
        };
        let geometry = GeometryPlan {
            spaces: vec![make_placed(0)],
            links: vec![],
        };
        let features = FeaturePlan::empty();
        let mut rng = StdRng::seed_from_u64(42);
        let rules = default_entity_rules();

        let plan = SimpleEntityPlanner
            .plan(&spatial, &geometry, &tiles, &features, &rules, &mut rng)
            .unwrap();

        let has_guardian = plan
            .entities
            .iter()
            .any(|e| e.archetype == EntityArchetypeId::from("skeleton_guardian"));
        assert!(has_guardian, "goal with main_goal must have a guardian");
    }

    #[test]
    fn entities_do_not_overlap_features() {
        let tiles = make_room_map();
        let spatial = SpatialPlan {
            spaces: vec![make_spec(
                0,
                NodeRole::Hub,
                Some(SpaceArchetype::Hall),
                &["noble"],
            )],
            links: vec![],
            constraints: vec![],
        };
        let geometry = GeometryPlan {
            spaces: vec![make_placed(0)],
            links: vec![],
        };

        // Place features on most floor tiles to force tight placement.
        let feature_cells: Vec<Point> = vec![
            Point { x: 2, y: 2 },
            Point { x: 3, y: 2 },
            Point { x: 4, y: 2 },
            Point { x: 2, y: 3 },
            Point { x: 3, y: 3 },
        ];
        let features = FeaturePlan {
            features: feature_cells
                .iter()
                .map(|&p| FeaturePlacement {
                    kind: FeatureKind::Barrel,
                    anchor: p,
                    cells: vec![p],
                    space_id: SpaceId(0),
                    tags: vec![],
                })
                .collect(),
        };
        let mut rng = StdRng::seed_from_u64(42);
        let rules = default_entity_rules();

        let plan = SimpleEntityPlanner
            .plan(&spatial, &geometry, &tiles, &features, &rules, &mut rng)
            .unwrap();

        let feature_set: HashSet<Point> = feature_cells.into_iter().collect();
        for entity in &plan.entities {
            assert!(
                !feature_set.contains(&entity.position),
                "entity at {:?} overlaps a feature",
                entity.position
            );
        }
    }

    #[test]
    fn entry_room_gets_no_entities() {
        let tiles = make_room_map();
        let spatial = SpatialPlan {
            spaces: vec![make_spec(
                0,
                NodeRole::Entry,
                Some(SpaceArchetype::Vestibule),
                &["noble", "sealed"],
            )],
            links: vec![],
            constraints: vec![],
        };
        let geometry = GeometryPlan {
            spaces: vec![make_placed(0)],
            links: vec![],
        };
        let features = FeaturePlan::empty();
        let mut rng = StdRng::seed_from_u64(42);
        let rules = default_entity_rules();

        let plan = SimpleEntityPlanner
            .plan(&spatial, &geometry, &tiles, &features, &rules, &mut rng)
            .unwrap();

        assert!(
            plan.entities.is_empty(),
            "entry room should have no entities"
        );
    }

    /// Even if an entry room carries tags that would otherwise trigger entity rules,
    /// the hard constraint "entry rooms get 0 entities" must prevail.
    #[test]
    fn entry_room_with_matching_tags_still_gets_no_entities() {
        let tiles = make_room_map();
        let spatial = SpatialPlan {
            spaces: vec![make_spec(
                0,
                NodeRole::Entry,
                Some(SpaceArchetype::Vestibule),
                // "barrels" would normally trigger the rat rule
                &["barrels", "food"],
            )],
            links: vec![],
            constraints: vec![],
        };
        let geometry = GeometryPlan {
            spaces: vec![make_placed(0)],
            links: vec![],
        };
        let features = FeaturePlan::empty();
        let mut rng = StdRng::seed_from_u64(42);
        let rules = default_entity_rules();

        let plan = SimpleEntityPlanner
            .plan(&spatial, &geometry, &tiles, &features, &rules, &mut rng)
            .unwrap();

        assert!(
            plan.entities.is_empty(),
            "entry room should have no entities even with matching tags"
        );
    }

    #[test]
    fn required_entity_in_fully_occupied_room_errors() {
        // Build a 3×3 map with only 1 floor tile.
        let mut map = TileMap::new(3, 3);
        for x in 0..3 {
            map.set(x, 0, Tile::WALL);
            map.set(x, 2, Tile::WALL);
        }
        for y in 0..3 {
            map.set(0, y, Tile::WALL);
            map.set(2, y, Tile::WALL);
        }
        map.set(1, 1, Tile::FLOOR);

        let spatial = SpatialPlan {
            spaces: vec![make_spec(
                0,
                NodeRole::Gate,
                Some(SpaceArchetype::Vestibule),
                &["locked"],
            )],
            links: vec![],
            constraints: vec![],
        };

        let rect = Rect {
            x: 0,
            y: 0,
            w: 3,
            h: 3,
        };
        let geometry = GeometryPlan {
            spaces: vec![PlacedSpace {
                space_id: SpaceId(0),
                rect,
                footprint: Footprint::Rect(rect),
                style: RealizationStyle::RoomLike,
                label: None,
            }],
            links: vec![],
        };

        // Occupy the only floor tile with a feature.
        let features = FeaturePlan {
            features: vec![FeaturePlacement {
                kind: FeatureKind::Barrel,
                anchor: Point { x: 1, y: 1 },
                cells: vec![Point { x: 1, y: 1 }],
                space_id: SpaceId(0),
                tags: vec![],
            }],
        };
        let mut rng = StdRng::seed_from_u64(42);
        let rules = default_entity_rules();

        let result =
            SimpleEntityPlanner.plan(&spatial, &geometry, &map, &features, &rules, &mut rng);
        assert!(
            result.is_err(),
            "should error when required entity can't be placed"
        );
    }
}
