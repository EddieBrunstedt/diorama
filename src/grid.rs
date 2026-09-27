//! The map as plain data: cells with terrain and an optional structure, grouped into chunks.
//!
//! Cells are addressed by `IVec2` grid coordinates, which can be negative so the map can grow
//! in any direction. Cell `(x, y)` covers the world square from `(x, 0, y)` to `(x + 1, 0, y + 1)`:
//! grid `y` runs along world `+Z`, toward the camera.

use crate::tuning::grid::CHUNK_SIZE;
use bevy::{platform::collections::HashMap, prelude::*};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Terrain {
    #[default]
    Ground,
    /// Impassable, and nothing can be built on it.
    Rock,
    /// Where the horde comes out. The horde walks over it, but nothing can be built on it.
    Nest,
}

impl Terrain {
    pub fn buildable(self) -> bool {
        match self {
            Terrain::Ground => true,
            Terrain::Rock | Terrain::Nest => false,
        }
    }

    /// Whether ground enemies can walk over it (structures on it aside).
    pub fn walkable(self) -> bool {
        match self {
            Terrain::Ground | Terrain::Nest => true,
            Terrain::Rock => false,
        }
    }
}

/// Something built on a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Structure {
    Wall,
    /// Part of the Command Center: what the horde is after. It's there from the start and can't
    /// be removed.
    CommandCenter,
}

impl Structure {
    /// Whether the player can remove it.
    pub fn removable(self) -> bool {
        match self {
            Structure::Wall => true,
            Structure::CommandCenter => false,
        }
    }
}

/// A square of cells that something bigger than one cell stands on, e.g. the Command Center.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Footprint {
    /// The lowest cell.
    pub min: IVec2,
    /// Cells along each side.
    pub size: i32,
}

impl Footprint {
    pub fn cells(self) -> impl Iterator<Item = IVec2> {
        (0..self.size * self.size).map(move |i| self.min + IVec2::new(i % self.size, i / self.size))
    }

    /// The area it covers on the ground (world x and z).
    pub fn area(self) -> Rect {
        let min = self.min.as_vec2();
        Rect::from_corners(min, min + Vec2::splat(self.size as f32))
    }

    /// Its middle on the ground.
    pub fn center(self) -> Vec3 {
        let center = self.area().center();
        Vec3::new(center.x, 0.0, center.y)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub terrain: Terrain,
    pub structure: Option<Structure>,
}

/// A square of `CHUNK_SIZE`×`CHUNK_SIZE` cells, the unit the map grows by.
#[derive(Clone, Debug, PartialEq)]
pub struct Chunk {
    /// Row by row, indexed by local coordinates.
    cells: Vec<Cell>,
}

impl Chunk {
    pub fn new(terrain: Terrain) -> Self {
        let cell = Cell {
            terrain,
            structure: None,
        };
        Self {
            cells: vec![cell; (CHUNK_SIZE * CHUNK_SIZE) as usize],
        }
    }

    fn index(local: IVec2) -> usize {
        debug_assert!(
            local.cmpge(IVec2::ZERO).all() && local.cmplt(IVec2::splat(CHUNK_SIZE)).all()
        );
        (local.y * CHUNK_SIZE + local.x) as usize
    }

    pub fn get(&self, local: IVec2) -> &Cell {
        &self.cells[Self::index(local)]
    }

    fn get_mut(&mut self, local: IVec2) -> &mut Cell {
        &mut self.cells[Self::index(local)]
    }
}

/// Why a structure can't go on a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocked {
    OutsideMap,
    Terrain(Terrain),
    Occupied(Structure),
}

/// One cell's structure changing, e.g. a wall placed (`None` → `Some(Wall)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Change {
    pub cell: IVec2,
    pub before: Option<Structure>,
    pub after: Option<Structure>,
}

impl Change {
    /// The change that takes it back.
    pub fn inverse(self) -> Self {
        Self {
            cell: self.cell,
            before: self.after,
            after: self.before,
        }
    }
}

/// The whole map.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct Grid {
    chunks: HashMap<IVec2, Chunk>,
}

impl Grid {
    /// The chunk a cell belongs to.
    pub fn chunk_of(cell: IVec2) -> IVec2 {
        cell.div_euclid(IVec2::splat(CHUNK_SIZE))
    }

    /// The cell's coordinates inside its chunk.
    pub fn local(cell: IVec2) -> IVec2 {
        cell.rem_euclid(IVec2::splat(CHUNK_SIZE))
    }

    /// A chunk's first (lowest) cell.
    pub fn chunk_origin(chunk: IVec2) -> IVec2 {
        chunk * CHUNK_SIZE
    }

    /// Adds a chunk, replacing any chunk already there.
    pub fn insert_chunk(&mut self, at: IVec2, chunk: Chunk) {
        self.chunks.insert(at, chunk);
    }

    pub fn chunks(&self) -> impl Iterator<Item = (IVec2, &Chunk)> {
        self.chunks.iter().map(|(&at, chunk)| (at, chunk))
    }

    pub fn cell(&self, cell: IVec2) -> Option<&Cell> {
        let chunk = self.chunks.get(&Self::chunk_of(cell))?;
        Some(chunk.get(Self::local(cell)))
    }

    fn cell_mut(&mut self, cell: IVec2) -> Option<&mut Cell> {
        let chunk = self.chunks.get_mut(&Self::chunk_of(cell))?;
        Some(chunk.get_mut(Self::local(cell)))
    }

    pub fn structure(&self, cell: IVec2) -> Option<Structure> {
        self.cell(cell).and_then(|cell| cell.structure)
    }

    /// Sets a cell's terrain. Returns false if the cell is outside the map.
    pub fn set_terrain(&mut self, cell: IVec2, terrain: Terrain) -> bool {
        self.cell_mut(cell)
            .map(|cell| cell.terrain = terrain)
            .is_some()
    }

    /// Whether a structure can be built on `cell`.
    pub fn check_build(&self, cell: IVec2) -> Result<(), Blocked> {
        let cell = self.cell(cell).ok_or(Blocked::OutsideMap)?;
        if let Some(structure) = cell.structure {
            return Err(Blocked::Occupied(structure));
        }
        if !cell.terrain.buildable() {
            return Err(Blocked::Terrain(cell.terrain));
        }
        Ok(())
    }

    /// Puts `structure` on `cell` (or clears it), whatever was there. Returns the change, or
    /// `None` if nothing changed (including when the cell is outside the map).
    pub fn set_structure(&mut self, cell: IVec2, structure: Option<Structure>) -> Option<Change> {
        let data = self.cell_mut(cell)?;
        if data.structure == structure {
            return None;
        }
        let change = Change {
            cell,
            before: data.structure,
            after: structure,
        };
        data.structure = structure;
        Some(change)
    }

    /// Applies a change, whatever the cell holds now. Returns it if it changed anything.
    pub fn apply(&mut self, change: Change) -> Option<Change> {
        self.set_structure(change.cell, change.after)
    }

    /// The smallest rectangle of cells covering the whole map (`max` inclusive).
    pub fn bounds(&self) -> Option<IRect> {
        self.chunks.keys().fold(None, |bounds, &chunk| {
            let min = Self::chunk_origin(chunk);
            let max = min + IVec2::splat(CHUNK_SIZE - 1);
            let rect = IRect::from_corners(min, max);
            Some(bounds.map_or(rect, |bounds: IRect| bounds.union(rect)))
        })
    }
}

/// The middle of a cell on the ground.
pub fn cell_center(cell: IVec2) -> Vec3 {
    Vec3::new(cell.x as f32 + 0.5, 0.0, cell.y as f32 + 0.5)
}

/// The cell under a point on (or above) the ground.
pub fn cell_at(point: Vec3) -> IVec2 {
    IVec2::new(point.x.floor() as i32, point.z.floor() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_by_two() -> Grid {
        let mut grid = Grid::default();
        for y in -1..=0 {
            for x in -1..=0 {
                grid.insert_chunk(IVec2::new(x, y), Chunk::new(Terrain::Ground));
            }
        }
        grid
    }

    #[test]
    fn negative_cells_find_their_chunk() {
        let n = CHUNK_SIZE;
        assert_eq!(Grid::chunk_of(IVec2::new(0, 0)), IVec2::new(0, 0));
        assert_eq!(Grid::chunk_of(IVec2::new(-1, n - 1)), IVec2::new(-1, 0));
        assert_eq!(Grid::chunk_of(IVec2::new(-n, -n - 1)), IVec2::new(-1, -2));
        assert_eq!(Grid::local(IVec2::new(-1, -n)), IVec2::new(n - 1, 0));
        assert_eq!(Grid::chunk_origin(IVec2::new(-1, 2)), IVec2::new(-n, 2 * n));
    }

    #[test]
    fn cells_across_chunks() {
        let mut grid = two_by_two();
        let n = CHUNK_SIZE;
        assert!(grid.cell(IVec2::new(-n, -n)).is_some());
        assert!(grid.cell(IVec2::new(n - 1, n - 1)).is_some());
        assert!(grid.cell(IVec2::new(n, 0)).is_none());
        assert!(grid.cell(IVec2::new(0, -n - 1)).is_none());
        assert!(grid.set_terrain(IVec2::new(-1, -1), Terrain::Rock));
        assert!(!grid.set_terrain(IVec2::new(100, 0), Terrain::Rock));
        assert_eq!(
            grid.cell(IVec2::new(-1, -1)).unwrap().terrain,
            Terrain::Rock
        );
        assert_eq!(
            grid.cell(IVec2::new(0, -1)).unwrap().terrain,
            Terrain::Ground
        );
    }

    #[test]
    fn building_rules() {
        let mut grid = two_by_two();
        let rock = IVec2::new(3, 3);
        grid.set_terrain(rock, Terrain::Rock);
        let cell = IVec2::new(-4, 2);
        assert_eq!(grid.check_build(cell), Ok(()));
        grid.set_structure(cell, Some(Structure::Wall));
        assert_eq!(
            grid.check_build(cell),
            Err(Blocked::Occupied(Structure::Wall))
        );
        assert_eq!(grid.check_build(rock), Err(Blocked::Terrain(Terrain::Rock)));
        assert_eq!(
            grid.check_build(IVec2::new(999, 0)),
            Err(Blocked::OutsideMap)
        );
    }

    #[test]
    fn changes_report_what_happened() {
        let mut grid = two_by_two();
        let cell = IVec2::new(5, -7);
        let placed = grid.set_structure(cell, Some(Structure::Wall)).unwrap();
        assert_eq!(placed.before, None);
        assert_eq!(placed.after, Some(Structure::Wall));
        assert_eq!(
            grid.set_structure(cell, Some(Structure::Wall)),
            None,
            "no-op"
        );
        assert_eq!(grid.apply(placed.inverse()), Some(placed.inverse()));
        assert_eq!(grid.structure(cell), None);
        assert_eq!(
            grid.set_structure(IVec2::new(999, 0), Some(Structure::Wall)),
            None
        );
    }

    #[test]
    fn footprints() {
        let footprint = Footprint {
            min: IVec2::new(-1, -1),
            size: 2,
        };
        let cells: Vec<IVec2> = footprint.cells().collect();
        assert_eq!(
            cells,
            [(-1, -1), (0, -1), (-1, 0), (0, 0)]
                .map(IVec2::from)
                .to_vec()
        );
        assert_eq!(footprint.center(), Vec3::ZERO);
        assert_eq!(footprint.area().size(), Vec2::splat(2.0));
    }

    #[test]
    fn world_to_cell_and_back() {
        for cell in [IVec2::new(0, 0), IVec2::new(-1, -1), IVec2::new(7, -12)] {
            assert_eq!(cell_at(cell_center(cell)), cell);
        }
        assert_eq!(cell_at(Vec3::new(-0.01, 0.0, 0.99)), IVec2::new(-1, 0));
    }

    #[test]
    fn bounds_cover_all_chunks() {
        let n = CHUNK_SIZE;
        assert_eq!(Grid::default().bounds(), None);
        let bounds = two_by_two().bounds().unwrap();
        assert_eq!(bounds.min, IVec2::splat(-n));
        assert_eq!(bounds.max, IVec2::splat(n - 1));
    }
}
