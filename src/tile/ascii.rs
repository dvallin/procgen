use crate::tile::map::{Tile, TileMap};

pub fn render_ascii(map: &TileMap) -> String {
    let mut out = String::new();

    for y in 0..map.height as i32 {
        for x in 0..map.width as i32 {
            let ch = match map.get(x, y).unwrap_or(Tile::Void) {
                Tile::Void => ' ',
                Tile::Floor => '.',
                Tile::Wall => '#',
                Tile::Door => '+',
                Tile::LockedDoor => '*',
            };
            out.push(ch);
        }
        out.push('\n');
    }

    out
}
