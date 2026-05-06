//! Feature planner — transforms spatial + geometry + tile data into placed features.
//!
//! The [`FeaturePlanner`] trait defines the boundary. [`SimpleFeaturePlanner`]
//! is the MVP implementation: it matches [`default_rules`](super::rules::default_rules)
//! against each room's role/archetype/tags and uses placement strategies to
//! find positions.
//!
//! When interior templates are provided, the planner uses zone-aware placement
//! for rooms that match a template, respecting `Clear` directives and placing
//! features in the specified zones.

use std::collections::{HashMap, HashSet};

use rand::seq::SliceRandom;
use tracing::{debug, info, info_span};

use crate::feature::placement::*;
use crate::feature::plan::*;
use crate::feature::rules::{FeatureRule, matching_rules};
use crate::geometry::geom::GeometryPlan;
use crate::interior::plan::{InteriorPlan, ZoneKind};
use crate::interior::template::{InteriorTemplate, ZoneAction, match_template};
use crate::spatial::plan::{SpaceId, SpaceSpec, SpatialPlan};
use crate::tile::map::TileMap;

/// Trait for producing a [`FeaturePlan`] from upstream layers.
pub trait FeaturePlanner {
    /// Plan feature placements.
    ///
    /// Reads room roles/tags from [`SpatialPlan`], room positions from
    /// [`GeometryPlan`], and tile walkability from [`TileMap`].
    /// The `rules` slice determines which features get placed in which rooms.
    /// The `per_room_rules` map provides atmosphere-contributed rules that
    /// apply only to specific rooms (keyed by SpaceId).
    /// The `templates` slice provides zone-aware interior templates that
    /// override generic rule placement for matched rooms.
    fn plan(
        &self,
        spatial: &SpatialPlan,
        geometry: &GeometryPlan,
        tiles: &TileMap,
        rules: &[FeatureRule],
        per_room_rules: &HashMap<SpaceId, Vec<FeatureRule>>,
        interiors: &[InteriorPlan],
        templates: &[InteriorTemplate],
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
        rules: &[FeatureRule],
        per_room_rules: &HashMap<SpaceId, Vec<FeatureRule>>,
        interiors: &[InteriorPlan],
        templates: &[InteriorTemplate],
        rng: &mut dyn rand::RngCore,
    ) -> Result<FeaturePlan, FeaturePlanError> {
        let _span = info_span!("feature_planning", rooms = geometry.spaces.len(),).entered();

        // Build SpaceId → SpaceSpec lookup.
        let spec_map: HashMap<SpaceId, &SpaceSpec> =
            spatial.spaces.iter().map(|s| (s.id, s)).collect();

        // Build SpaceId → InteriorPlan lookup.
        let interior_map: HashMap<SpaceId, &InteriorPlan> =
            interiors.iter().map(|p| (p.space_id, p)).collect();
        let empty_reserved: HashSet<crate::geometry::geom::Point> = HashSet::new();

        let mut features = Vec::new();
        // Global occupied set — prevents cross-room overlap on shared boundaries.
        let mut global_occupied: HashSet<crate::geometry::geom::Point> = HashSet::new();
        let feature_registry = crate::feature::registry::FeatureRegistry::default_registry();

        for placed in &geometry.spaces {
            let Some(spec) = spec_map.get(&placed.space_id) else {
                debug!(
                    space_id = ?placed.space_id,
                    "no SpaceSpec found for placed space, skipping feature placement"
                );
                continue;
            };

            let rect = placed.rect;
            // Per-room occupied starts from the global set (so we don't place
            // on tiles already claimed by an adjacent room's features).
            let mut room_occupied = global_occupied.clone();

            // Check if an interior template matches this room.
            let matched_template = match_template(templates, spec);

            if let Some(template) = matched_template {
                // Template-driven placement: use zone cells.
                debug!(
                    space_id = ?placed.space_id,
                    label = ?placed.label,
                    template = %template.name,
                    "applying interior template"
                );

                let interior = interior_map.get(&placed.space_id);

                // Collect clear zones for blocking-feature exclusion.
                let clear_zones: HashSet<ZoneKind> = template
                    .directives
                    .iter()
                    .filter_map(|d| match &d.action {
                        ZoneAction::Clear => Some(d.zone),
                        _ => None,
                    })
                    .collect();

                // Execute Place directives.
                for directive in &template.directives {
                    let ZoneAction::Place {
                        ref feature_type,
                        max_count,
                    } = directive.action
                    else {
                        continue;
                    };

                    let placed_count = place_in_zone(
                        feature_type,
                        max_count,
                        directive.zone,
                        placed.space_id,
                        interior,
                        tiles,
                        rect,
                        &mut room_occupied,
                        &mut features,
                        rng,
                    );

                    if placed_count > 0 {
                        debug!(
                            space_id = ?placed.space_id,
                            feature_type = %feature_type,
                            zone = ?directive.zone,
                            count = placed_count,
                            "placed features via template"
                        );
                    }
                }

                // After template directives, also apply atmosphere rules
                // (but respect clear zones for blocking features).
                if let Some(atmo_rules) = per_room_rules.get(&placed.space_id) {
                    for rule in atmo_rules {
                        let is_blocking = feature_registry.is_blocking(&rule.feature_type);
                        let room_reserved = interior
                            .map(|ip| &ip.reserved_paths)
                            .unwrap_or(&empty_reserved);

                        // Skip blocking features targeted at clear zones.
                        if is_blocking && would_violate_clear_zone(rule, &clear_zones) {
                            continue;
                        }

                        let placed_count = place_rule(
                            rule,
                            placed.space_id,
                            rect,
                            tiles,
                            &mut room_occupied,
                            &feature_registry,
                            room_reserved,
                            &mut features,
                            rng,
                        )?;

                        if placed_count > 0 {
                            debug!(
                                space_id = ?placed.space_id,
                                feature_type = %rule.feature_type,
                                count = placed_count,
                                "placed atmosphere features (template room)"
                            );
                        }
                    }
                }
            } else {
                // No template matched — use generic rule-based placement.
                let matched = matching_rules(rules, spec);

                // Also include per-room atmosphere rules for this space.
                let atmosphere_rules = per_room_rules.get(&placed.space_id);
                let all_matched: Vec<&FeatureRule> = matched
                    .into_iter()
                    .chain(
                        atmosphere_rules
                            .map(|v| v.iter().collect::<Vec<_>>())
                            .unwrap_or_default(),
                    )
                    .collect();

                if all_matched.is_empty() {
                    debug!(
                        space_id = ?placed.space_id,
                        label = ?placed.label,
                        "no feature rules matched"
                    );
                    global_occupied = room_occupied;
                    continue;
                }

                debug!(
                    space_id = ?placed.space_id,
                    label = ?placed.label,
                    rule_count = all_matched.len(),
                    "matched feature rules"
                );

                for rule in &all_matched {
                    let room_reserved = interior_map
                        .get(&placed.space_id)
                        .map(|ip| &ip.reserved_paths)
                        .unwrap_or(&empty_reserved);
                    let placed_count = place_rule(
                        rule,
                        placed.space_id,
                        rect,
                        tiles,
                        &mut room_occupied,
                        &feature_registry,
                        room_reserved,
                        &mut features,
                        rng,
                    )?;

                    if placed_count > 0 {
                        debug!(
                            space_id = ?placed.space_id,
                            feature_type = %rule.feature_type,
                            count = placed_count,
                            "placed features"
                        );
                    }
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
///
/// Blocking features are excluded from reserved door-to-door path cells
/// (via the `reserved` set passed to `pick_candidate`), guaranteeing that
/// traversal between doors is never obstructed.
fn place_rule(
    rule: &FeatureRule,
    space_id: SpaceId,
    rect: crate::geometry::geom::Rect,
    tiles: &TileMap,
    occupied: &mut HashSet<crate::geometry::geom::Point>,
    feature_registry: &crate::feature::registry::FeatureRegistry,
    reserved: &HashSet<crate::geometry::geom::Point>,
    features: &mut Vec<FeaturePlacement>,
    rng: &mut dyn rand::RngCore,
) -> Result<u32, FeaturePlanError> {
    let mut placed = 0u32;
    let is_blocking = feature_registry.is_blocking(&rule.feature_type);
    let empty_excluded: HashSet<crate::geometry::geom::Point> = HashSet::new();

    for _ in 0..rule.max_count {
        let excluded = if is_blocking {
            reserved
        } else {
            &empty_excluded
        };
        let candidate = pick_candidate(&rule.strategy, tiles, rect, occupied, excluded, rng);

        match candidate {
            Some(point) => {
                occupied.insert(point);

                features.push(FeaturePlacement {
                    feature_type: rule.feature_type.clone(),
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
    excluded: &HashSet<crate::geometry::geom::Point>,
    rng: &mut dyn rand::RngCore,
) -> Option<crate::geometry::geom::Point> {
    let combined_excluded: HashSet<crate::geometry::geom::Point> =
        occupied.union(excluded).copied().collect();
    match strategy {
        PlacementStrategy::Center => find_center_tile(tiles, rect, &combined_excluded),
        PlacementStrategy::WallAdjacent => {
            let mut candidates = find_wall_adjacent_tiles(tiles, rect, &combined_excluded);
            candidates.shuffle(rng);
            pick_furthest_from_occupied(candidates, occupied)
        }
        PlacementStrategy::Corner => {
            let mut candidates = find_corner_tiles(tiles, rect, &combined_excluded);
            candidates.shuffle(rng);
            pick_furthest_from_occupied(candidates, occupied)
        }
        PlacementStrategy::RandomFloor => {
            let mut candidates = find_floor_tiles(tiles, rect, &combined_excluded);
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

// ─── Zone-aware placement helpers ───────────────────────────────────────────

/// Place features within a specific zone using the `InteriorPlan`'s zone cells.
///
/// Falls back to the generic placement strategy if the zone has no available
/// cells (e.g., in a very small room where the zone might not exist).
fn place_in_zone(
    feature_type: &crate::feature::registry::FeatureType,
    max_count: u32,
    zone: ZoneKind,
    space_id: SpaceId,
    interior: Option<&&InteriorPlan>,
    tiles: &TileMap,
    rect: crate::geometry::geom::Rect,
    occupied: &mut HashSet<crate::geometry::geom::Point>,
    features: &mut Vec<FeaturePlacement>,
    rng: &mut dyn rand::RngCore,
) -> u32 {
    let mut placed = 0u32;

    for _ in 0..max_count {
        let candidate = if let Some(ip) = interior {
            // Get available zone cells.
            let mut candidates = ip.available_zone_cells(zone, occupied);
            // Also exclude door-adjacent tiles.
            candidates.retain(|p| !is_door_adjacent_point(tiles, *p));
            if candidates.is_empty() {
                // Fallback: try generic strategy based on zone kind.
                let strategy = zone_to_strategy(zone);
                pick_candidate(&strategy, tiles, rect, occupied, &ip.reserved_paths, rng)
            } else {
                candidates.shuffle(rng);
                pick_furthest_from_occupied(candidates, occupied)
            }
        } else {
            // No interior plan — fall back to strategy.
            let strategy = zone_to_strategy(zone);
            pick_candidate(&strategy, tiles, rect, occupied, &HashSet::new(), rng)
        };

        match candidate {
            Some(point) => {
                occupied.insert(point);
                features.push(FeaturePlacement {
                    feature_type: feature_type.clone(),
                    anchor: point,
                    cells: vec![point],
                    space_id,
                    tags: vec![],
                });
                placed += 1;
            }
            None => break,
        }
    }

    placed
}

/// Map a zone kind to a fallback placement strategy.
fn zone_to_strategy(zone: ZoneKind) -> PlacementStrategy {
    match zone {
        ZoneKind::Center => PlacementStrategy::Center,
        ZoneKind::WallBand => PlacementStrategy::WallAdjacent,
        ZoneKind::Corner => PlacementStrategy::Corner,
        ZoneKind::Open | ZoneKind::DoorPath => PlacementStrategy::RandomFloor,
        // Urban zone kinds: edge zones and frontage map to wall-adjacent placement
        // (features along edges/storefronts are conceptually similar to wall placement).
        ZoneKind::EdgeZone | ZoneKind::Frontage => PlacementStrategy::WallAdjacent,
    }
}

/// Check if a feature rule's placement strategy targets a zone that is marked Clear.
fn would_violate_clear_zone(rule: &FeatureRule, clear_zones: &HashSet<ZoneKind>) -> bool {
    let target_zone = match rule.strategy {
        PlacementStrategy::Center => ZoneKind::Center,
        PlacementStrategy::WallAdjacent => ZoneKind::WallBand,
        PlacementStrategy::Corner => ZoneKind::Corner,
        PlacementStrategy::RandomFloor => return false, // could land anywhere, don't block
    };
    clear_zones.contains(&target_zone)
}

/// Returns true if any cardinal neighbor of `p` is a Door or LockedDoor.
fn is_door_adjacent_point(map: &TileMap, p: crate::geometry::geom::Point) -> bool {
    use crate::tile::registry::Tile;
    p.cardinals().iter().any(|n| {
        let t = map.get(n.x, n.y);
        t == Some(Tile::DOOR) || t == Some(Tile::LOCKED_DOOR)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::registry::FeatureType;
    use crate::feature::rules::default_rules;
    use crate::geometry::geom::*;
    use crate::intent::graph::{NodeRole, ScenarioNodeId};
    use crate::intent::map_intent::LocationKind;
    use crate::spatial::plan::*;
    use crate::tile::registry::Tile;
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
            map.set(x, rect.y, Tile::WALL);
            map.set(x, rect.y + rect.h - 1, Tile::WALL);
        }
        for y in rect.y..rect.y + rect.h {
            map.set(rect.x, y, Tile::WALL);
            map.set(rect.x + rect.w - 1, y, Tile::WALL);
        }
        // Floor interior.
        for y in (rect.y + 1)..(rect.y + rect.h - 1) {
            for x in (rect.x + 1)..(rect.x + rect.w - 1) {
                map.set(x, y, Tile::FLOOR);
            }
        }
        // Door on east wall.
        map.set(5, 3, Tile::DOOR);
        map
    }

    fn make_spec(
        id: u32,
        role: NodeRole,
        archetype: Option<SpaceArchetype>,
        tags: &[&str],
    ) -> SpaceSpec {
        let raw_tags: Vec<crate::tag::Tag> =
            tags.iter().map(|s| crate::tag::Tag::from(*s)).collect();
        let (structural, atmosphere) = classify_tags(&raw_tags);
        SpaceSpec {
            id: SpaceId(id),
            origin: ScenarioNodeId(id),
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
            archetype,
            size_hint: SizeHint::Medium,
            max_connectors: None,
            connector_distribution: None,
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
    fn single_hub_room_gets_table() {
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
            location_kind: LocationKind::Dungeon,
        };
        let geometry = GeometryPlan {
            spaces: vec![make_placed(0)],
            links: vec![],
        };
        let mut rng = StdRng::seed_from_u64(42);
        let rules = default_rules();

        let plan = SimpleFeaturePlanner
            .plan(
                &spatial,
                &geometry,
                &tiles,
                &rules,
                &std::collections::HashMap::new(),
                &[],
                &[],
                &mut rng,
            )
            .unwrap();

        let types: Vec<&FeatureType> = plan.features.iter().map(|f| &f.feature_type).collect();
        assert!(
            types.contains(&&FeatureType::from("table")),
            "hub should get a table, got: {:?}",
            types
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
            location_kind: LocationKind::Dungeon,
        };
        let geometry = GeometryPlan {
            spaces: vec![make_placed(0)],
            links: vec![],
        };
        let mut rng = StdRng::seed_from_u64(42);
        let rules = default_rules();

        let plan = SimpleFeaturePlanner
            .plan(
                &spatial,
                &geometry,
                &tiles,
                &rules,
                &std::collections::HashMap::new(),
                &[],
                &[],
                &mut rng,
            )
            .unwrap();

        let has_chest = plan
            .features
            .iter()
            .any(|f| f.feature_type == FeatureType::from("chest"));
        assert!(has_chest, "reward room must have a chest");
    }

    #[test]
    fn no_features_overlap() {
        let tiles = make_room_map();
        // Hub room gets table; overlap check is valid even with a single feature.
        let spatial = SpatialPlan {
            spaces: vec![make_spec(
                0,
                NodeRole::Hub,
                Some(SpaceArchetype::Hall),
                &["noble"],
            )],
            links: vec![],
            constraints: vec![],
            location_kind: LocationKind::Dungeon,
        };
        let geometry = GeometryPlan {
            spaces: vec![make_placed(0)],
            links: vec![],
        };
        let mut rng = StdRng::seed_from_u64(42);
        let rules = default_rules();

        let plan = SimpleFeaturePlanner
            .plan(
                &spatial,
                &geometry,
                &tiles,
                &rules,
                &std::collections::HashMap::new(),
                &[],
                &[],
                &mut rng,
            )
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
            map.set(x, 0, Tile::WALL);
            map.set(x, 2, Tile::WALL);
        }
        for y in 0..3 {
            map.set(0, y, Tile::WALL);
            map.set(2, y, Tile::WALL);
        }
        map.set(1, 1, Tile::FLOOR);

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
            location_kind: LocationKind::Dungeon,
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
        let rules = default_rules();

        let result = SimpleFeaturePlanner.plan(
            &spatial,
            &geometry,
            &map,
            &rules,
            &std::collections::HashMap::new(),
            &[],
            &[],
            &mut rng,
        );
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
            location_kind: LocationKind::Dungeon,
        };
        let geometry = GeometryPlan {
            spaces: vec![make_placed(0)],
            links: vec![],
        };
        let mut rng = StdRng::seed_from_u64(42);
        let rules = default_rules();

        let plan = SimpleFeaturePlanner
            .plan(
                &spatial,
                &geometry,
                &tiles,
                &rules,
                &std::collections::HashMap::new(),
                &[],
                &[],
                &mut rng,
            )
            .unwrap();
        assert!(
            plan.features.is_empty(),
            "gate/vestibule with only 'locked' tag should have no features"
        );
    }
}
