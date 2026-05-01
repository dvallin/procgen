#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tile {
    Void,
    Floor,
    Wall,
    Door,
    LockedDoor,
}

impl Tile {
    /// Can a creature walk on this tile?
    pub fn is_walkable(self) -> bool {
        matches!(self, Tile::Floor | Tile::Door | Tile::LockedDoor)
    }

    /// Is this tile "solid" — blocks movement and line of sight?
    pub fn is_solid(self) -> bool {
        matches!(self, Tile::Void | Tile::Wall)
    }
}

#[derive(Debug, Clone)]
pub struct TileMap {
    pub width: u32,
    pub height: u32,
    pub tiles: Vec<Tile>,
}

impl TileMap {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            tiles: vec![Tile::Void; (width * height) as usize],
        }
    }

    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32
    }

    pub fn get(&self, x: i32, y: i32) -> Option<Tile> {
        if !self.in_bounds(x, y) {
            return None;
        }
        let idx = (y as u32 * self.width + x as u32) as usize;
        Some(self.tiles[idx])
    }

    pub fn set(&mut self, x: i32, y: i32, tile: Tile) {
        if !self.in_bounds(x, y) {
            return;
        }
        let idx = (y as u32 * self.width + x as u32) as usize;
        self.tiles[idx] = tile;
    }

    /// BFS flood fill from `start`, visiting only tiles that satisfy `passable`.
    /// Returns all reachable positions (including `start` if it satisfies the predicate).
    pub fn flood_fill(
        &self,
        start: crate::geometry::geom::Point,
        passable: impl Fn(Tile) -> bool,
    ) -> std::collections::HashSet<crate::geometry::geom::Point> {
        use std::collections::{HashSet, VecDeque};

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        if let Some(tile) = self.get(start.x, start.y)
            && passable(tile)
        {
            visited.insert(start);
            queue.push_back(start);
        }

        while let Some(pos) = queue.pop_front() {
            for neighbor in pos.cardinals() {
                if visited.contains(&neighbor) {
                    continue;
                }
                if let Some(tile) = self.get(neighbor.x, neighbor.y)
                    && passable(tile)
                {
                    visited.insert(neighbor);
                    queue.push_back(neighbor);
                }
            }
        }

        visited
    }
}
