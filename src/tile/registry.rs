//! Tile registry: maps [`TileId`] handles to [`TileProperties`].
//!
//! The registry allows game-specific terrain to be defined in data (JSON)
//! rather than requiring code changes for each new tile type.

use crate::tag::Tag;
use serde::{Deserialize, Serialize};

// ─── TileId ─────────────────────────────────────────────────────────────────

/// A lightweight handle into the tile registry.
///
/// Well-known IDs for the built-in tiles are available as associated constants
/// on the [`Tile`] namespace struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileId(pub u16);

impl std::fmt::Display for TileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TileId({})", self.0)
    }
}

impl TileId {
    /// Quick walkability check for built-in tiles.
    ///
    /// For custom (data-driven) tiles, use [`TileRegistry::is_walkable`] instead.
    pub fn is_walkable(self) -> bool {
        self == Tile::FLOOR
            || self == Tile::DOOR
            || self == Tile::LOCKED_DOOR
            || self == Tile::STAIRS
            || self == Tile::RUBBLE
            || self == Tile::GRASS
    }

    /// Quick solidity check for built-in tiles.
    ///
    /// For custom (data-driven) tiles, use [`TileRegistry::is_solid`] instead.
    pub fn is_solid(self) -> bool {
        self == Tile::VOID || self == Tile::WALL
    }
}

// ─── Tile namespace (backward compat) ───────────────────────────────────────

/// Named constants for the built-in tile types.
///
/// This replaces the former `Tile` enum. Use these constants wherever you
/// previously matched on enum variants:
///
/// ```
/// use procgen::tile::registry::{Tile, TileId};
///
/// let floor: TileId = Tile::FLOOR;
/// assert!(floor.is_walkable());
/// ```
pub struct Tile;

impl Tile {
    /// Empty/uncarved space.
    pub const VOID: TileId = TileId(0);
    /// Walkable floor.
    pub const FLOOR: TileId = TileId(1);
    /// Solid wall.
    pub const WALL: TileId = TileId(2);
    /// Normal (unlocked) door — walkable.
    pub const DOOR: TileId = TileId(3);
    /// Locked door — walkable (once unlocked).
    pub const LOCKED_DOOR: TileId = TileId(4);
    /// Water — not walkable, not opaque.
    pub const WATER: TileId = TileId(5);
    /// Pit — not walkable, not opaque.
    pub const PIT: TileId = TileId(6);
    /// Stairs — walkable, transition point.
    pub const STAIRS: TileId = TileId(7);
    /// Rubble — walkable but rough terrain.
    pub const RUBBLE: TileId = TileId(8);
    /// Grass — walkable natural ground.
    pub const GRASS: TileId = TileId(9);

    /// The number of built-in tile types.
    pub const BUILTIN_COUNT: u16 = 10;
}

// ─── TileProperties ─────────────────────────────────────────────────────────

/// Describes the properties of a tile type in the registry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TileProperties {
    /// Human-readable name (e.g. "floor", "locked_door").
    pub name: String,
    /// Can a creature walk on this tile?
    pub walkable: bool,
    /// Does this tile block line of sight?
    pub opaque: bool,
    /// Character used for ASCII debug rendering.
    pub ascii_char: char,
    /// Arbitrary tags for rule matching (e.g. "liquid", "hazard").
    #[serde(default)]
    pub tags: Vec<Tag>,
}

// ─── TileRegistry ───────────────────────────────────────────────────────────

/// Maps [`TileId`] → [`TileProperties`].
///
/// Built-in tiles are always registered at construction time.
/// Additional tiles can be added via [`TileRegistry::register`] or loaded
/// from a JSON asset file.
#[derive(Debug, Clone)]
pub struct TileRegistry {
    tiles: Vec<TileProperties>,
}

impl TileRegistry {
    /// Creates a registry pre-populated with the built-in tile types.
    pub fn default_registry() -> Self {
        let tiles = vec![
            // 0: Void
            TileProperties {
                name: "void".into(),
                walkable: false,
                opaque: true,
                ascii_char: ' ',
                tags: vec![],
            },
            // 1: Floor
            TileProperties {
                name: "floor".into(),
                walkable: true,
                opaque: false,
                ascii_char: '.',
                tags: vec![],
            },
            // 2: Wall
            TileProperties {
                name: "wall".into(),
                walkable: false,
                opaque: true,
                ascii_char: '#',
                tags: vec![],
            },
            // 3: Door
            TileProperties {
                name: "door".into(),
                walkable: true,
                opaque: false,
                ascii_char: '+',
                tags: vec![],
            },
            // 4: LockedDoor
            TileProperties {
                name: "locked_door".into(),
                walkable: true,
                opaque: false,
                ascii_char: '*',
                tags: vec![],
            },
            // 5: Water
            TileProperties {
                name: "water".into(),
                walkable: false,
                opaque: false,
                ascii_char: '~',
                tags: vec![Tag::from("liquid"), Tag::from("hazard")],
            },
            // 6: Pit
            TileProperties {
                name: "pit".into(),
                walkable: false,
                opaque: false,
                ascii_char: 'v',
                tags: vec![Tag::from("hazard")],
            },
            // 7: Stairs
            TileProperties {
                name: "stairs".into(),
                walkable: true,
                opaque: false,
                ascii_char: '>',
                tags: vec![Tag::from("transition")],
            },
            // 8: Rubble
            TileProperties {
                name: "rubble".into(),
                walkable: true,
                opaque: false,
                ascii_char: ':',
                tags: vec![Tag::from("rough")],
            },
            // 9: Grass
            TileProperties {
                name: "grass".into(),
                walkable: true,
                opaque: false,
                ascii_char: ',',
                tags: vec![Tag::from("natural")],
            },
        ];

        Self { tiles }
    }

    /// Creates a registry from a vec of tile properties (e.g. loaded from JSON).
    ///
    /// The position in the vec determines the [`TileId`] (index = id).
    pub fn from_properties(tiles: Vec<TileProperties>) -> Self {
        Self { tiles }
    }

    /// Register a new tile type and return its assigned [`TileId`].
    pub fn register(&mut self, props: TileProperties) -> TileId {
        let id = TileId(self.tiles.len() as u16);
        self.tiles.push(props);
        id
    }

    /// Look up properties for a tile ID. Returns `None` if the ID is out of range.
    pub fn get(&self, id: TileId) -> Option<&TileProperties> {
        self.tiles.get(id.0 as usize)
    }

    /// Returns true if the tile is walkable according to the registry.
    pub fn is_walkable(&self, id: TileId) -> bool {
        self.get(id).is_some_and(|p| p.walkable)
    }

    /// Returns true if the tile is solid (not walkable) according to the registry.
    pub fn is_solid(&self, id: TileId) -> bool {
        self.get(id).is_some_and(|p| !p.walkable)
    }

    /// Returns true if the tile is opaque (blocks line of sight).
    pub fn is_opaque(&self, id: TileId) -> bool {
        self.get(id).is_some_and(|p| p.opaque)
    }

    /// Returns the ASCII character for rendering.
    pub fn ascii_char(&self, id: TileId) -> char {
        self.get(id).map_or('?', |p| p.ascii_char)
    }

    /// Returns the name of a tile type.
    pub fn name(&self, id: TileId) -> &str {
        self.get(id).map_or("unknown", |p| p.name.as_str())
    }

    /// The total number of registered tile types.
    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    /// Whether the registry is empty (should never be true after construction).
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }
}

impl Default for TileRegistry {
    fn default() -> Self {
        Self::default_registry()
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_tile_constants_are_correct() {
        assert_eq!(Tile::VOID.0, 0);
        assert_eq!(Tile::FLOOR.0, 1);
        assert_eq!(Tile::WALL.0, 2);
        assert_eq!(Tile::DOOR.0, 3);
        assert_eq!(Tile::LOCKED_DOOR.0, 4);
    }

    #[test]
    fn tile_id_walkability() {
        assert!(!Tile::VOID.is_walkable());
        assert!(Tile::FLOOR.is_walkable());
        assert!(!Tile::WALL.is_walkable());
        assert!(Tile::DOOR.is_walkable());
        assert!(Tile::LOCKED_DOOR.is_walkable());
    }

    #[test]
    fn tile_id_solidity() {
        assert!(Tile::VOID.is_solid());
        assert!(!Tile::FLOOR.is_solid());
        assert!(Tile::WALL.is_solid());
        assert!(!Tile::DOOR.is_solid());
        assert!(!Tile::LOCKED_DOOR.is_solid());
    }

    #[test]
    fn default_registry_has_builtin_tiles() {
        let reg = TileRegistry::default_registry();
        assert_eq!(reg.len(), Tile::BUILTIN_COUNT as usize);
        assert_eq!(reg.name(Tile::VOID), "void");
        assert_eq!(reg.name(Tile::FLOOR), "floor");
        assert_eq!(reg.name(Tile::WALL), "wall");
        assert_eq!(reg.name(Tile::DOOR), "door");
        assert_eq!(reg.name(Tile::LOCKED_DOOR), "locked_door");
        assert_eq!(reg.name(Tile::WATER), "water");
        assert_eq!(reg.name(Tile::PIT), "pit");
        assert_eq!(reg.name(Tile::STAIRS), "stairs");
        assert_eq!(reg.name(Tile::RUBBLE), "rubble");
        assert_eq!(reg.name(Tile::GRASS), "grass");
    }

    #[test]
    fn registry_walkability_matches_tile_id() {
        let reg = TileRegistry::default_registry();
        for id_val in 0..Tile::BUILTIN_COUNT {
            let id = TileId(id_val);
            assert_eq!(
                reg.is_walkable(id),
                id.is_walkable(),
                "mismatch for {:?}",
                id
            );
        }
    }

    #[test]
    fn registry_ascii_chars() {
        let reg = TileRegistry::default_registry();
        assert_eq!(reg.ascii_char(Tile::VOID), ' ');
        assert_eq!(reg.ascii_char(Tile::FLOOR), '.');
        assert_eq!(reg.ascii_char(Tile::WALL), '#');
        assert_eq!(reg.ascii_char(Tile::DOOR), '+');
        assert_eq!(reg.ascii_char(Tile::LOCKED_DOOR), '*');
    }

    #[test]
    fn register_custom_tile() {
        let mut reg = TileRegistry::default_registry();
        let lava_id = reg.register(TileProperties {
            name: "lava".into(),
            walkable: false,
            opaque: false,
            ascii_char: '%',
            tags: vec![Tag::from("liquid"), Tag::from("hazard")],
        });
        assert_eq!(lava_id.0, Tile::BUILTIN_COUNT);
        assert_eq!(reg.name(lava_id), "lava");
        assert!(!reg.is_walkable(lava_id));
        assert!(!reg.is_opaque(lava_id));
        assert_eq!(reg.ascii_char(lava_id), '%');
    }

    #[test]
    fn unknown_tile_id_returns_defaults() {
        let reg = TileRegistry::default_registry();
        let bad_id = TileId(999);
        assert_eq!(reg.get(bad_id), None);
        assert!(!reg.is_walkable(bad_id));
        assert!(!reg.is_solid(bad_id));
        assert_eq!(reg.ascii_char(bad_id), '?');
        assert_eq!(reg.name(bad_id), "unknown");
    }
}
