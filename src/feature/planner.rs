//! Feature planner — transforms spatial + geometry + tile data into placed features.
//!
//! The [`FeaturePlanner`] trait defines the boundary. [`SimpleFeaturePlanner`]
//! is the MVP implementation: it matches [`default_rules`](super::rules::default_rules)
//! against each room's role/archetype/tags and uses placement strategies to
//! find positions.

use std::collections::{HashMap, HashSet};

use rand::seq::SliceRandom;
use tracing::{debug, info, info_span};

use crate::feature::placement::*;
use crate::feature::plan::*;
use crate::feature::rules::{FeatureRule, default_rules, matching_rules};
use crate::geometry::geom::GeometryPlan;
use crate::spatial::plan::{SpaceId, SpaceSpec, SpatialPlan};
use crate::tile::map::TileMap;

/// Trait for producing a [`FeaturePlan`] from upstream layers.
pub trait FeaturePlanner {
    /// Plan feature placements.
    ///
    /// Reads room roles/tags from [`SpatialPlan`], room positions from
    /// [`GeometryPlan`], and tile walkability from [`TileMap`].
    fn plan(
        &self,
        spatial: &SpatialPlan,
        geometry: &GeometryPlan,
        tiles: &TileMap,
        rng: &mut dyn rand::RngCore,
    ) -> Result<FeaturePlan, FeaturePlanError>;
}

/// Simple rule-driven feature planner.
///
/// For each room in the geometry plan, it looks up the matching
/// [`SpaceSpec`], finds applicable [`FeatureRule`]s, and uses the
/// associated [`PlacementStrategy`] to find tile positions.
#[derive(Debug, Default)]
pub struct SimpleFeaturePlanner;

impl FeaturePlanner for SimpleFeaturePlanner {
    fn plan(
        &self,
        spatial: &SpatialPlan,
        geometry: &GeometryPlan,
        tiles: &TileMap,
        rng: &mut dyn rand::RngCore,
    ) -> Result<FeaturePlan, FeaturePlanError> {
        let _span = info_span!("feature_planning", rooms = geometry.spaces.len(),).entered();

        // Build SpaceId → SpaceSpec lookup.
        let spec_map: HashMap<SpaceId, &SpaceSpec> =
            spatial.spaces.iter().map(|s| (s.id, s)).collect();

        let rules = default_rules();
        let mut features = Vec::new();
        // Global occupied set — prevents cross-room overlap on shared boundaries.
        let mut global_occupied: HashSet<crate::geometry::geom::Point> = HashSet::new();

        for placed in &geometry.spaces {
            let Some(spec) = spec_map.get(&placed.space_id) else {
                debug!(
                    space_id = ?placed.space_id,
                    "no SpaceSpec found for placed space, skipping feature placement"
                );
                continue;
            };

            let matched = matching_rules(&rules, spec);
            if matched.is_empty() {
                debug!(
                    space_id = ?placed.space_id,
                    label = ?placed.label,
                    "no feature rules matched"
                );
                continue;
            }

            debug!(
                space_id = ?placed.space_id,
                label = ?placed.label,
                rule_count = matched.len(),
                "matched feature rules"
            );

            let rect = placed.rect;
            // Per-room occupied starts from the global set (so we don't place
            // on tiles already claimed by an adjacent room's features).
            let mut room_occupied = global_occupied.clone();

            for rule in &matched {
                let placed_count = place_rule(
                    rule,
                    placed.space_id,
                    rect,
                    tiles,
                    &mut room_occupied,
                    &mut features,
                    rng,
                )?;

                if placed_count > 0 {
                    debug!(
                        space_id = ?placed.space_id,
                        kind = ?rule.kind,
                        count = placed_count,
                        "placed features"
                    );
                }
            }

            // Merge room placements into global set.
            global_occupied = room_occupied;
        }

        info!(total_features = features.len(), "feature planning complete");

        Ok(FeaturePlan { features })
    }
}

/// Try to place up to `rule.max_count` instances of a feature in a room.
///
/// Returns the number of features actually placed. If the rule is `required`
/// and no candidate position could be found, returns an error.
fn place_rule(
    rule: &FeatureRule,
    space_id: SpaceId,
    rect: crate::geometry::geom::Rect,
    tiles: &TileMap,
    occupied: &mut HashSet<crate::geometry::geom::Point>,
    features: &mut Vec<FeaturePlacement>,
    rng: &mut dyn rand::RngCore,
) -> Result<u32, FeaturePlanError> {
    let mut placed = 0u32;

    for _ in 0..rule.max_count {
        let candidate = pick_candidate(&rule.strategy, tiles, rect, occupied, rng);

        match candidate {
            Some(point) => {
                occupied.insert(point);
                features.push(FeaturePlacement {
                    kind: rule.kind.clone(),
                    anchor: point,
                    cells: vec![point],
                    space_id,
                    tags: vec![],
                });
                placed += 1;
            }
            None => {
                if placed == 0 && rule.required {
                    return Err(FeaturePlanError::NoWalkableTiles { space_id });
                }
                // No more candidates — stop trying for this rule.
                break;
            }
        }
    }

    Ok(placed)
}

/// Pick a single candidate point using the given strategy.
///
/// For strategies that return multiple candidates (`WallAdjacent`, `Corner`,
/// `RandomFloor`), shuffles them with the RNG before scoring by distance.
/// This introduces randomness while still spreading features around the room.
fn pick_candidate(
    strategy: &PlacementStrategy,
    tiles: &TileMap,
    rect: crate::geometry::geom::Rect,
    occupied: &HashSet<crate::geometry::geom::Point>,
    rng: &mut dyn rand::RngCore,
) -> Option<crate::geometry::geom::Point> {
    match strategy {
        PlacementStrategy::Center => find_center_tile(tiles, rect, occupied),
        PlacementStrategy::WallAdjacent => {
            let mut candidates = find_wall_adjacent_tiles(tiles, rect, occupied);
            candidates.shuffle(rng);
            pick_furthest_from_occupied(candidates, occupied)
        }
        PlacementStrategy::Corner => {
            let mut candidates = find_corner_tiles(tiles, rect, occupied);
            candidates.shuffle(rng);
            pick_furthest_from_occupied(candidates, occupied)
        }
        PlacementStrategy::RandomFloor => {
            let mut candidates = find_floor_tiles(tiles, rect, occupied);
            candidates.shuffle(rng);
            pick_furthest_from_occupied(candidates, occupied)
        }
    }
}

/// From a list of candidates, pick the one with the greatest minimum
/// squared distance to any occupied cell. If `occupied` is empty,
/// picks the last candidate (bottom-right bias — spreads from center).
fn pick_furthest_from_occupied(
    candidates: Vec<crate::geometry::geom::Point>,
    occupied: &HashSet<crate::geometry::geom::Point>,
) -> Option<crate::geometry::geom::Point> {
    if candidates.is_empty() {
        return None;
    }
    if occupied.is_empty() {
        // No occupied cells yet — pick the first candidate.
        return candidates.into_iter().next();
    }

    candidates.into_iter().max_by_key(|candidate| {
        // Minimum squared distance to any occupied cell.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::geom::*;
    use crate::intent::graph::{NodeRole, ScenarioNodeId};
    use crate::spatial::plan::*;
    use crate::tile::map::Tile;
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
        // Walls.
        for x in rect.x..rect.x + rect.w {
            map.set(x, rect.y, Tile::Wall);
            map.set(x, rect.y + rect.h - 1, Tile::Wall);
        }
        for y in rect.y..rect.y + rect.h {
            map.set(rect.x, y, Tile::Wall);
            map.set(rect.x + rect.w - 1, y, Tile::Wall);
        }
        // Floor interior.
        for y in (rect.y + 1)..(rect.y + rect.h - 1) {
            for x in (rect.x + 1)..(rect.x + rect.w - 1) {
                map.set(x, y, Tile::Floor);
            }
        }
        // Door on east wall.
        map.set(5, 3, Tile::Door);
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
            tags: tags.iter().map(|s| crate::tag::Tag::from(*s)).collect(),
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
    fn single_hub_room_gets_table_and_barrels() {
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
        let mut rng = StdRng::seed_from_u64(42);

        let plan = SimpleFeaturePlanner
            .plan(&spatial, &geometry, &tiles, &mut rng)
            .unwrap();

        let kinds: Vec<&FeatureKind> = plan.features.iter().map(|f| &f.kind).collect();
        assert!(
            kinds.contains(&&FeatureKind::Table),
            "hub should get a table, got: {:?}",
            kinds
        );
        let barrel_count = kinds.iter().filter(|k| ***k == FeatureKind::Barrel).count();
        assert!(
            barrel_count >= 1,
            "hub should get at least one barrel, got: {}",
            barrel_count
        );
    }

    #[test]
    fn reward_room_gets_required_chest() {
        let tiles = make_room_map();
        let spatial = SpatialPlan {
            spaces: vec![make_spec(
                0,
                NodeRole::Reward,
                Some(SpaceArchetype::Chamber),
                &["optional"],
            )],
            links: vec![],
            constraints: vec![],
        };
        let geometry = GeometryPlan {
            spaces: vec![make_placed(0)],
            links: vec![],
        };
        let mut rng = StdRng::seed_from_u64(42);

        let plan = SimpleFeaturePlanner
            .plan(&spatial, &geometry, &tiles, &mut rng)
            .unwrap();

        let has_chest = plan.features.iter().any(|f| f.kind == FeatureKind::Chest);
        assert!(has_chest, "reward room must have a chest");
    }

    #[test]
    fn no_features_overlap() {
        let tiles = make_room_map();
        // Hub with barrels tag -> multiple barrel rules + table = several features.
        let spatial = SpatialPlan {
            spaces: vec![make_spec(
                0,
                NodeRole::Hub,
                Some(SpaceArchetype::Hall),
                &["barrels"],
            )],
            links: vec![],
            constraints: vec![],
        };
        let geometry = GeometryPlan {
            spaces: vec![make_placed(0)],
            links: vec![],
        };
        let mut rng = StdRng::seed_from_u64(42);

        let plan = SimpleFeaturePlanner
            .plan(&spatial, &geometry, &tiles, &mut rng)
            .unwrap();

        let mut all_cells: Vec<crate::geometry::geom::Point> = Vec::new();
        for f in &plan.features {
            for &c in &f.cells {
                assert!(
                    !all_cells.contains(&c),
                    "feature cell {:?} is occupied by two features",
                    c
                );
                all_cells.push(c);
            }
        }
    }

    #[test]
    fn required_feature_in_tiny_room_errors() {
        // Build a 3×3 map where the interior is only 1 floor tile.
        let mut map = TileMap::new(3, 3);
        for x in 0..3 {
            map.set(x, 0, Tile::Wall);
            map.set(x, 2, Tile::Wall);
        }
        for y in 0..3 {
            map.set(0, y, Tile::Wall);
            map.set(2, y, Tile::Wall);
        }
        map.set(1, 1, Tile::Floor);

        let spatial = SpatialPlan {
            spaces: vec![
                // Vault + main_goal triggers sarcophagus (required, center)
                // AND torch decoration (optional, wall-adj). Sarcophagus will
                // take the only floor tile.
                make_spec(
                    0,
                    NodeRole::Goal,
                    Some(SpaceArchetype::Vault),
                    &["main_goal"],
                ),
                // Reward is required chest — but room 1 is the same tiny 3×3.
                make_spec(
                    1,
                    NodeRole::Reward,
                    Some(SpaceArchetype::Chamber),
                    &["optional"],
                ),
            ],
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
            spaces: vec![
                PlacedSpace {
                    space_id: SpaceId(0),
                    rect,
                    footprint: Footprint::Rect(rect),
                    style: RealizationStyle::RoomLike,
                    label: None,
                },
                PlacedSpace {
                    space_id: SpaceId(1),
                    rect,
                    footprint: Footprint::Rect(rect),
                    style: RealizationStyle::RoomLike,
                    label: None,
                },
            ],
            links: vec![],
        };

        let mut rng = StdRng::seed_from_u64(42);

        let result = SimpleFeaturePlanner.plan(&spatial, &geometry, &map, &mut rng);
        // The first room claims the only floor tile for the sarcophagus.
        // The second room's required chest has no tiles left → error.
        assert!(
            result.is_err(),
            "should error when required feature can't be placed"
        );
    }

    #[test]
    fn gate_with_no_matching_rules_gets_no_features() {
        let tiles = make_room_map();
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
        let geometry = GeometryPlan {
            spaces: vec![make_placed(0)],
            links: vec![],
        };
        let mut rng = StdRng::seed_from_u64(42);

        let plan = SimpleFeaturePlanner
            .plan(&spatial, &geometry, &tiles, &mut rng)
            .unwrap();
        assert!(
            plan.features.is_empty(),
            "gate/vestibule with only 'locked' tag should have no features"
        );
    }
}
