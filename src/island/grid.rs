//! The island's data model: which kind of tile sits in which grid cell.
//!
//! Plain data with no rendering, so it can be unit-tested and reused by gameplay code.
//! Cells are `IVec2(x, y)` where +x is east and +y is south (world +z). The hearth is at the origin.

use crate::voxel::TILE_SIZE;
use bevy::prelude::*;
use std::collections::{HashMap, VecDeque};

/// The starting island. `H` hearth, `=` road, `T` forest, `.` meadow, space for no tile.
/// The top row is north. The road's open end must touch the island's edge.
const STARTING_ISLAND: &[&str] = &[
    "TT.=T", //
    "T..=.", //
    "..H=.", //
    "T...T", //
    ".T.TT", //
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TileKind {
    Meadow,
    Forest,
    Road,
    Hearth,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    North,
    East,
    South,
    West,
}

impl Side {
    pub const ALL: [Side; 4] = [Side::North, Side::East, Side::South, Side::West];

    pub fn offset(self) -> IVec2 {
        match self {
            Side::North => IVec2::NEG_Y,
            Side::East => IVec2::X,
            Side::South => IVec2::Y,
            Side::West => IVec2::NEG_X,
        }
    }

    pub fn opposite(self) -> Side {
        match self {
            Side::North => Side::South,
            Side::East => Side::West,
            Side::South => Side::North,
            Side::West => Side::East,
        }
    }

    /// The side of `from` that faces the neighboring cell `to`, if they are adjacent.
    pub fn between(from: IVec2, to: IVec2) -> Option<Side> {
        Side::ALL
            .into_iter()
            .find(|side| from + side.offset() == to)
    }
}

#[derive(Resource, Clone, Debug)]
pub struct Island {
    tiles: HashMap<IVec2, TileKind>,
}

impl Island {
    pub fn starting() -> Self {
        Self::from_map(STARTING_ISLAND)
    }

    /// Parses an ASCII map (see [`STARTING_ISLAND`]). Coordinates are relative to the `H` cell.
    pub fn from_map(rows: &[&str]) -> Self {
        let mut tiles = HashMap::new();
        let mut hearth = None;
        for (y, row) in rows.iter().enumerate() {
            for (x, ch) in row.chars().enumerate() {
                let kind = match ch {
                    '.' => TileKind::Meadow,
                    'T' => TileKind::Forest,
                    '=' => TileKind::Road,
                    'H' => TileKind::Hearth,
                    ' ' => continue,
                    other => panic!("unknown island map character {other:?}"),
                };
                let cell = IVec2::new(x as i32, y as i32);
                if kind == TileKind::Hearth {
                    assert!(hearth.is_none(), "island map has more than one hearth");
                    hearth = Some(cell);
                }
                tiles.insert(cell, kind);
            }
        }
        let hearth = hearth.expect("island map has no hearth");
        Self {
            tiles: tiles
                .into_iter()
                .map(|(cell, kind)| (cell - hearth, kind))
                .collect(),
        }
    }

    pub fn get(&self, cell: IVec2) -> Option<TileKind> {
        self.tiles.get(&cell).copied()
    }

    pub fn tiles(&self) -> impl Iterator<Item = (IVec2, TileKind)> + '_ {
        self.tiles.iter().map(|(&cell, &kind)| (cell, kind))
    }

    /// Puts a tile on `cell` (replacing whatever was there).
    pub fn place(&mut self, cell: IVec2, kind: TileKind) {
        self.tiles.insert(cell, kind);
    }

    /// The tiles next to `cell`, with the side of `cell` they're on.
    pub fn neighbors(&self, cell: IVec2) -> impl Iterator<Item = (Side, TileKind)> + '_ {
        Side::ALL
            .into_iter()
            .filter_map(move |side| Some((side, self.get(cell + side.offset())?)))
    }

    /// The empty cells touching the island, sorted row by row: where new tiles can go.
    pub fn edge_cells(&self) -> Vec<IVec2> {
        let mut cells: Vec<IVec2> = self
            .tiles
            .keys()
            .flat_map(|&cell| Side::ALL.map(|side| cell + side.offset()))
            .filter(|cell| !self.tiles.contains_key(cell))
            .collect();
        cells.sort_by_key(|cell| (cell.y, cell.x));
        cells.dedup();
        cells
    }

    /// Number of steps from `cell` to the nearest cell without a tile (1 for edge tiles).
    /// Used to shape the island's underside: deeper toward the middle.
    pub fn edge_distance(&self, cell: IVec2) -> u32 {
        let mut seen = vec![cell];
        let mut queue = VecDeque::from([(cell, 0)]);
        while let Some((current, distance)) = queue.pop_front() {
            if !self.tiles.contains_key(&current) {
                return distance;
            }
            for side in Side::ALL {
                let next = current + side.offset();
                if !seen.contains(&next) {
                    seen.push(next);
                    queue.push_back((next, distance + 1));
                }
            }
        }
        unreachable!("the island is finite, so the search always reaches an empty cell")
    }

    /// The enemy path: road cells from the open end to the hearth, ending with the hearth cell.
    /// Empty if no road touches the hearth.
    pub fn road_path(&self) -> Vec<IVec2> {
        let Some(hearth) = self.tiles().find(|&(_, kind)| kind == TileKind::Hearth) else {
            return Vec::new();
        };
        let mut path = vec![hearth.0];
        loop {
            let current = *path.last().unwrap();
            let next = Side::ALL
                .into_iter()
                .map(|side| current + side.offset())
                .find(|next| self.get(*next) == Some(TileKind::Road) && !path.contains(next));
            match next {
                Some(next) => path.push(next),
                None => break,
            }
        }
        if path.len() == 1 {
            return Vec::new();
        }
        path.reverse();
        path
    }

    /// The road cell furthest from the hearth, where slimes come in.
    pub fn road_end(&self) -> Option<IVec2> {
        self.road_path().first().copied()
    }

    /// The side of the road's open end where enemies come in from off the island.
    /// Prefers continuing the road straight; otherwise any side without a tile.
    pub fn road_entry(&self) -> Option<(IVec2, Side)> {
        let path = self.road_path();
        let (&end, &next) = (path.first()?, path.get(1)?);
        let inward = Side::between(end, next)?;
        [inward.opposite()]
            .into_iter()
            .chain(Side::ALL)
            .find(|side| self.get(end + side.offset()).is_none())
            .map(|side| (end, side))
    }

    /// The sides of `cell` that a road runs out of (toward the next road cell, the hearth,
    /// or off the island at the open end).
    pub fn road_sides(&self, cell: IVec2) -> Vec<Side> {
        let path = self.road_path();
        let mut sides = Vec::new();
        for pair in path.windows(2) {
            if pair[0] == cell {
                sides.extend(Side::between(pair[0], pair[1]));
            }
            if pair[1] == cell {
                sides.extend(Side::between(pair[1], pair[0]));
            }
        }
        if let Some((end, side)) = self.road_entry()
            && end == cell
        {
            sides.push(side);
        }
        sides
    }

    /// Half the width of the smallest centered square that contains the whole island, in world units.
    pub fn world_radius(&self) -> f32 {
        let max = self
            .tiles
            .keys()
            .map(|cell| cell.x.abs().max(cell.y.abs()))
            .max()
            .unwrap_or(0);
        (max as f32 + 0.5) * TILE_SIZE
    }
}

/// World position of the center of a cell's top surface.
pub fn cell_to_world(cell: IVec2) -> Vec3 {
    Vec3::new(cell.x as f32, 0.0, cell.y as f32) * TILE_SIZE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starting_island_has_hearth_at_origin() {
        let island = Island::starting();
        assert_eq!(island.get(IVec2::ZERO), Some(TileKind::Hearth));
        assert_eq!(island.tiles().count(), 25);
    }

    #[test]
    fn road_path_runs_from_edge_to_hearth() {
        let island = Island::from_map(&[
            ".=.", //
            ".=.", //
            ".H.", //
        ]);
        assert_eq!(
            island.road_path(),
            vec![IVec2::new(0, -2), IVec2::new(0, -1), IVec2::ZERO]
        );
        assert_eq!(island.road_entry(), Some((IVec2::new(0, -2), Side::North)));
    }

    #[test]
    fn road_path_follows_bends() {
        let island = Island::starting();
        assert_eq!(
            island.road_path(),
            vec![
                IVec2::new(1, -2),
                IVec2::new(1, -1),
                IVec2::new(1, 0),
                IVec2::ZERO
            ]
        );
    }

    #[test]
    fn road_sides_connect_neighbors_and_open_end() {
        let island = Island::starting();
        let sorted = |mut sides: Vec<Side>| {
            sides.sort_by_key(|side| *side as u8);
            sides
        };
        assert_eq!(
            sorted(island.road_sides(IVec2::new(1, -2))),
            vec![Side::North, Side::South]
        );
        assert_eq!(
            sorted(island.road_sides(IVec2::new(1, 0))),
            vec![Side::North, Side::West]
        );
        assert_eq!(island.road_sides(IVec2::ZERO), vec![Side::East]);
        assert!(island.road_sides(IVec2::new(-1, 0)).is_empty());
    }

    #[test]
    fn no_road_means_no_path() {
        let island = Island::from_map(&[".H."]);
        assert!(island.road_path().is_empty());
        assert_eq!(island.road_entry(), None);
    }

    #[test]
    fn edge_distance_grows_toward_the_middle() {
        let island = Island::starting();
        assert_eq!(island.edge_distance(IVec2::new(-2, -2)), 1);
        assert_eq!(island.edge_distance(IVec2::new(-1, -1)), 2);
        assert_eq!(island.edge_distance(IVec2::ZERO), 3);
    }

    #[test]
    fn edge_cells_ring_the_island() {
        let island = Island::from_map(&[".H"]);
        assert_eq!(
            island.edge_cells(),
            vec![
                IVec2::new(-1, -1),
                IVec2::new(0, -1),
                IVec2::new(-2, 0),
                IVec2::new(1, 0),
                IVec2::new(-1, 1),
                IVec2::new(0, 1),
            ]
        );
        assert_eq!(Island::starting().edge_cells().len(), 20);
    }

    #[test]
    fn placing_extends_the_road() {
        let mut island = Island::starting();
        assert_eq!(island.road_end(), Some(IVec2::new(1, -2)));
        island.place(IVec2::new(1, -3), TileKind::Road);
        assert_eq!(island.road_end(), Some(IVec2::new(1, -3)));
        assert_eq!(island.road_entry(), Some((IVec2::new(1, -3), Side::North)));
        assert_eq!(island.road_path().len(), 5);
    }

    #[test]
    fn world_radius_covers_all_tiles() {
        assert_eq!(Island::starting().world_radius(), 2.5 * TILE_SIZE);
    }
}
