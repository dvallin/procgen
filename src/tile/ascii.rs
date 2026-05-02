use std::collections::HashMap;

use crate::entity::plan::{EntityArchetypeId, EntityPlan};
use crate::feature::plan::{FeatureKind, FeaturePlan};
use crate::geometry::geom::Point;
use crate::tile::map::TileMap;
use crate::tile::registry::{Tile, TileId, TileRegistry};

/// Renders a `TileMap` with an overlay map applied on top.
/// Overlay characters take priority over the underlying tile.
/// When a registry is provided, it is used to look up tile characters;
/// otherwise the built-in `tile_char` fallback is used.
fn render_with_overlay(
    map: &TileMap,
    overlay: &HashMap<Point, char>,
    registry: Option<&TileRegistry>,
) -> String {
    let mut out = String::new();

    for y in 0..map.height as i32 {
        for x in 0..map.width as i32 {
            let pt = Point { x, y };
            let ch = if let Some(&oc) = overlay.get(&pt) {
                oc
            } else {
                match registry {
                    Some(reg) => reg.ascii_char(map.get(x, y).unwrap_or(Tile::VOID)),
                    None => tile_char(map.get(x, y).unwrap_or(Tile::VOID)),
                }
            };
            out.push(ch);
        }
        out.push('\n');
    }

    out
}

/// Returns the ASCII character for a tile.
fn tile_char(tile: TileId) -> char {
    if tile == Tile::VOID {
        ' '
    } else if tile == Tile::FLOOR {
        '.'
    } else if tile == Tile::WALL {
        '#'
    } else if tile == Tile::DOOR {
        '+'
    } else if tile == Tile::LOCKED_DOOR {
        '*'
    } else {
        '?'
    }
}

/// Builds a point→char overlay from a feature plan.
fn build_feature_overlay(features: &FeaturePlan) -> HashMap<Point, char> {
    let mut overlay = HashMap::new();
    for placement in &features.features {
        let ch = feature_char(&placement.kind);
        for &cell in &placement.cells {
            overlay.insert(cell, ch);
        }
    }
    overlay
}

/// Renders a `TileMap` as an ASCII string (one char per tile, newline per row).
pub fn render_ascii(map: &TileMap) -> String {
    render_with_overlay(map, &HashMap::new(), None)
}

/// Renders a `TileMap` as an ASCII string using the registry for tile characters.
pub fn render_ascii_with_registry(map: &TileMap, registry: &TileRegistry) -> String {
    render_with_overlay(map, &HashMap::new(), Some(registry))
}

/// Returns the ASCII character used to represent a given `FeatureKind`.
pub fn feature_char(kind: &FeatureKind) -> char {
    match kind {
        FeatureKind::Altar => '†',
        FeatureKind::Sarcophagus => 'S',
        FeatureKind::Chest => '$',
        FeatureKind::Barrel => 'o',
        FeatureKind::Shelf => '=',
        FeatureKind::Table => 'T',
        FeatureKind::Trap => '^',
        FeatureKind::Decoration(name) => match name.as_str() {
            "moss" | "vines" | "fungus" => ',',
            "stalagmite" | "stalactite" | "crystal" => '\u{00a4}',
            "torch" | "lantern" => '!',
            "banner" | "tapestry" => '|',
            _ => '*',
        },
    }
}

/// Renders a `TileMap` with a `FeaturePlan` overlay as an ASCII string.
///
/// Feature cells take priority over the underlying tile character. If multiple
/// features occupy the same cell, the last one in the plan wins.
pub fn render_ascii_with_features(map: &TileMap, features: &FeaturePlan) -> String {
    render_with_overlay(map, &build_feature_overlay(features), None)
}

/// Returns the ASCII character used to represent an entity archetype.
///
/// Uses substring matching on the archetype ID for MVP flexibility.
pub fn entity_char(archetype: &EntityArchetypeId) -> char {
    let id = archetype.0.as_str();
    if id.contains("rat") {
        'r'
    } else if id.contains("guardian") {
        'G'
    } else if id.contains("skeleton") {
        's'
    } else if id.contains("smuggler") {
        '@'
    } else if id.contains("mimic") {
        'M'
    } else {
        'E'
    }
}

/// Renders a `TileMap` with both a `FeaturePlan` and `EntityPlan` overlay.
///
/// Priority: entity > feature > tile base. If multiple entities/features
/// occupy the same cell, the last one in the plan wins.
pub fn render_ascii_with_entities(
    map: &TileMap,
    features: &FeaturePlan,
    entities: &EntityPlan,
) -> String {
    let mut overlay = build_feature_overlay(features);
    // Entity overlay takes priority over features.
    for entity in &entities.entities {
        let ch = entity_char(&entity.archetype);
        overlay.insert(entity.position, ch);
    }
    render_with_overlay(map, &overlay, None)
}

/// Renders a `TileMap` with feature and entity overlays using the tile registry.
pub fn render_ascii_full(
    map: &TileMap,
    features: &FeaturePlan,
    entities: &EntityPlan,
    registry: &TileRegistry,
) -> String {
    let mut overlay = build_feature_overlay(features);
    for entity in &entities.entities {
        let ch = entity_char(&entity.archetype);
        overlay.insert(entity.position, ch);
    }
    render_with_overlay(map, &overlay, Some(registry))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::plan::{FeatureKind, FeaturePlacement, FeaturePlan};
    use crate::geometry::geom::Point;
    use crate::spatial::plan::SpaceId;
    use crate::tile::map::TileMap;
    use crate::tile::registry::Tile;

    /// Helper: builds a 5×5 room with walls on the border, a door on the north
    /// edge at (2,0), and floor tiles inside.
    fn make_5x5_room() -> TileMap {
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
        // Place a door on the north wall.
        map.set(2, 0, Tile::DOOR);
        map
    }

    /// Helper: builds a small `FeaturePlan` with an altar at (2,2) and a chest
    /// at (1,3).
    fn make_test_features() -> FeaturePlan {
        FeaturePlan {
            features: vec![
                FeaturePlacement {
                    kind: FeatureKind::Altar,
                    anchor: Point { x: 2, y: 2 },
                    cells: vec![Point { x: 2, y: 2 }],
                    space_id: SpaceId(0),
                    tags: vec![],
                },
                FeaturePlacement {
                    kind: FeatureKind::Chest,
                    anchor: Point { x: 1, y: 3 },
                    cells: vec![Point { x: 1, y: 3 }],
                    space_id: SpaceId(0),
                    tags: vec![],
                },
            ],
        }
    }

    #[test]
    fn render_ascii_base_map() {
        let map = make_5x5_room();
        let output = render_ascii(&map);
        let expected = "\
##+##\n\
#...#\n\
#...#\n\
#...#\n\
#####\n";
        assert_eq!(output, expected);
    }

    #[test]
    fn render_ascii_with_features_overlays_correctly() {
        let map = make_5x5_room();
        let features = make_test_features();
        let output = render_ascii_with_features(&map, &features);
        // Row 0: ##+##
        // Row 1: #...#
        // Row 2: #.†.#   (altar at 2,2)
        // Row 3: #$..#   (chest at 1,3)
        // Row 4: #####
        let expected = "\
##+##\n\
#...#\n\
#.†.#\n\
#$..#\n\
#####\n";
        assert_eq!(output, expected);
    }

    #[test]
    fn render_ascii_without_features_has_no_overlay() {
        let map = make_5x5_room();
        let base = render_ascii(&map);
        let with_empty = render_ascii_with_features(&map, &FeaturePlan::empty());
        assert_eq!(
            base, with_empty,
            "empty FeaturePlan should match base render"
        );
    }

    #[test]
    fn feature_char_mapping_is_complete() {
        assert_eq!(feature_char(&FeatureKind::Altar), '†');
        assert_eq!(feature_char(&FeatureKind::Sarcophagus), 'S');
        assert_eq!(feature_char(&FeatureKind::Chest), '$');
        assert_eq!(feature_char(&FeatureKind::Barrel), 'o');
        assert_eq!(feature_char(&FeatureKind::Shelf), '=');
        assert_eq!(feature_char(&FeatureKind::Table), 'T');
        assert_eq!(feature_char(&FeatureKind::Trap), '^');
        assert_eq!(feature_char(&FeatureKind::Decoration("torch".into())), '!');
    }

    #[test]
    fn cosmetic_decoration_chars() {
        assert_eq!(feature_char(&FeatureKind::Decoration("moss".into())), ',');
        assert_eq!(feature_char(&FeatureKind::Decoration("vines".into())), ',');
        assert_eq!(feature_char(&FeatureKind::Decoration("fungus".into())), ',');
        assert_eq!(
            feature_char(&FeatureKind::Decoration("stalagmite".into())),
            '\u{00a4}'
        );
        assert_eq!(
            feature_char(&FeatureKind::Decoration("stalactite".into())),
            '\u{00a4}'
        );
        assert_eq!(
            feature_char(&FeatureKind::Decoration("crystal".into())),
            '\u{00a4}'
        );
        assert_eq!(feature_char(&FeatureKind::Decoration("torch".into())), '!');
        assert_eq!(feature_char(&FeatureKind::Decoration("banner".into())), '|');
    }

    #[test]
    fn multi_cell_feature_renders_all_cells() {
        let map = make_5x5_room();
        let features = FeaturePlan {
            features: vec![FeaturePlacement {
                kind: FeatureKind::Table,
                anchor: Point { x: 1, y: 1 },
                cells: vec![Point { x: 1, y: 1 }, Point { x: 2, y: 1 }],
                space_id: SpaceId(0),
                tags: vec![],
            }],
        };
        let output = render_ascii_with_features(&map, &features);
        let expected = "\
##+##\n\
#TT.#\n\
#...#\n\
#...#\n\
#####\n";
        assert_eq!(output, expected);
    }

    #[test]
    fn entity_char_mapping() {
        assert_eq!(entity_char(&EntityArchetypeId::from("rat")), 'r');
        assert_eq!(entity_char(&EntityArchetypeId::from("tavern_rat")), 'r');
        assert_eq!(entity_char(&EntityArchetypeId::from("skeleton")), 's');
        assert_eq!(
            entity_char(&EntityArchetypeId::from("skeleton_guardian")),
            'G'
        );
        assert_eq!(entity_char(&EntityArchetypeId::from("gate_guardian")), 'G');
        assert_eq!(entity_char(&EntityArchetypeId::from("smuggler")), '@');
        assert_eq!(entity_char(&EntityArchetypeId::from("chest_mimic")), 'M');
        assert_eq!(entity_char(&EntityArchetypeId::from("unknown_thing")), 'E');
    }

    #[test]
    fn render_ascii_with_entities_shows_entities() {
        let map = make_5x5_room();
        let features = FeaturePlan::empty();
        let entities = EntityPlan {
            entities: vec![
                crate::entity::plan::EntityPlacement {
                    archetype: EntityArchetypeId::from("skeleton"),
                    position: Point { x: 2, y: 2 },
                    space_id: SpaceId(0),
                    behavior_tags: vec![],
                    patrol_zone: None,
                },
                crate::entity::plan::EntityPlacement {
                    archetype: EntityArchetypeId::from("rat"),
                    position: Point { x: 3, y: 3 },
                    space_id: SpaceId(0),
                    behavior_tags: vec![],
                    patrol_zone: None,
                },
            ],
        };
        let output = render_ascii_with_entities(&map, &features, &entities);
        let expected = "\
##+##\n\
#...#\n\
#.s.#\n\
#..r#\n\
#####\n";
        assert_eq!(output, expected);
    }

    #[test]
    fn entities_override_features_in_render() {
        let map = make_5x5_room();
        let features = make_test_features(); // altar at (2,2), chest at (1,3)
        let entities = EntityPlan {
            entities: vec![crate::entity::plan::EntityPlacement {
                archetype: EntityArchetypeId::from("skeleton_guardian"),
                position: Point { x: 2, y: 2 }, // same cell as the altar
                space_id: SpaceId(0),
                behavior_tags: vec![],
                patrol_zone: None,
            }],
        };
        let output = render_ascii_with_entities(&map, &features, &entities);
        // Entity 'G' should override the altar '†' at (2,2).
        // Chest '$' at (1,3) remains.
        let expected = "\
##+##\n\
#...#\n\
#.G.#\n\
#$..#\n\
#####\n";
        assert_eq!(output, expected);
    }

    #[test]
    fn empty_entity_plan_matches_features_only() {
        let map = make_5x5_room();
        let features = make_test_features();
        let entities = EntityPlan::empty();
        let with_entities = render_ascii_with_entities(&map, &features, &entities);
        let features_only = render_ascii_with_features(&map, &features);
        assert_eq!(
            with_entities, features_only,
            "empty EntityPlan should produce same output as features-only render"
        );
    }
}
