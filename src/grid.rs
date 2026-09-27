//! The map as plain data: cells with terrain and an optional structure, grouped into chunks.
//!
//! Cells are addressed by `IVec2` grid coordinates, which can be negative so the map can grow
//! in any direction. Cell `(x, y)` covers the world square from `(x, 0, y)` to `(x + 1, 0, y + 1)`:
//! grid `y` runs along world `+Z`, toward the camera.

use crate::tuning::command_center::MAX_HP as COMMAND_CENTER_HP;
use crate::tuning::grid::CHUNK_SIZE;
use crate::tuning::structures::{
    ARTILLERY_COMMAND_HP, AUTOCANNON_HP, FLAMER_HP, MORTAR_HP, WALL_HP,
};
use bevy::{platform::collections::HashMap, prelude::*};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Terrain {
    #[default]
    Ground,
    /// Impassable, and nothing can be built on it.
    Rock,
    /// Where the horde comes out. The horde walks over it, but nothing can be built on it.
    Nest,
    /// Slows the horde down (see [`crate::flow::Class::speed_on`]), and nothing can be built on
    /// it: a gap full of mud can't be walled shut.
    Mud,
}

impl Terrain {
    pub fn buildable(self) -> bool {
        match self {
            Terrain::Ground => true,
            Terrain::Rock | Terrain::Nest | Terrain::Mud => false,
        }
    }

    /// Whether ground enemies can walk over it (structures on it aside).
    pub fn walkable(self) -> bool {
        match self {
            Terrain::Ground | Terrain::Nest | Terrain::Mud => true,
            Terrain::Rock => false,
        }
    }
}

/// What the player can build. Every kind stands on one cell and is part of the maze: the horde
/// walks around it, or breaks through it (see [`crate::flow`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Kind {
    #[default]
    Wall,
    /// Single target, fast.
    Autocannon,
    /// Short range, burns everything in a cone.
    Flamer,
    /// Long range, slow, splash.
    Mortar,
    /// Doesn't shoot by itself: it gives the mortar barrage ability, one charge per copy (see
    /// [`crate::abilities`]).
    ArtilleryCommand,
}

impl Kind {
    /// Everything that can be built, in the order the build panel shows them.
    pub const ALL: [Kind; 5] = [
        Kind::Wall,
        Kind::Autocannon,
        Kind::Flamer,
        Kind::Mortar,
        Kind::ArtilleryCommand,
    ];

    /// The most HP it can have, which it's built with.
    pub const fn max_hp(self) -> u32 {
        match self {
            Kind::Wall => WALL_HP,
            Kind::Autocannon => AUTOCANNON_HP,
            Kind::Flamer => FLAMER_HP,
            Kind::Mortar => MORTAR_HP,
            Kind::ArtilleryCommand => ARTILLERY_COMMAND_HP,
        }
    }

    /// Everything but walls: the towers, and the Artillery Command, which is built and drawn
    /// like one (a base and a turret that turns), though it doesn't shoot by itself.
    pub fn is_tower(self) -> bool {
        self != Kind::Wall
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Wall => "Wall",
            Kind::Autocannon => "Autocannon",
            Kind::Flamer => "Flamer",
            Kind::Mortar => "Mortar",
            Kind::ArtilleryCommand => "Artillery Command",
        }
    }
}

/// Something standing on a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Structure {
    /// Something the player built. The horde can break it: `hp` is what it has left. Its HP is
    /// part of the structure, so undoing the removal of a damaged wall brings it back as damaged
    /// as it was. `committed` is whether it was built before the last wave started, which
    /// makes it sell at a loss (see [`crate::economy`]).
    Built {
        kind: Kind,
        hp: u32,
        committed: bool,
    },
    /// Part of the Command Center: what the horde is after. It's there from the start and can't
    /// be removed. Its HP is kept for the whole building, in [`Grid::command_center`].
    CommandCenter,
}

impl Structure {
    /// A newly built wall, at full HP.
    #[cfg(test)]
    pub const WALL: Structure = Structure::new(Kind::Wall);

    /// A newly built `kind`, at full HP and not committed yet.
    pub const fn new(kind: Kind) -> Self {
        Structure::Built {
            kind,
            hp: kind.max_hp(),
            committed: false,
        }
    }

    /// What was built, if the player built it.
    pub fn kind(self) -> Option<Kind> {
        match self {
            Structure::Built { kind, .. } => Some(kind),
            Structure::CommandCenter => None,
        }
    }

    /// Whether the player can remove it.
    pub fn removable(self) -> bool {
        match self {
            Structure::Built { .. } => true,
            Structure::CommandCenter => false,
        }
    }

    /// HP left and the most it can have, if the horde can break it.
    pub fn health(self) -> Option<(u32, u32)> {
        match self {
            Structure::Built { kind, hp, .. } => Some((hp, kind.max_hp())),
            Structure::CommandCenter => None,
        }
    }

    /// Whether it's missing HP that a repair could put back.
    pub fn damaged(self) -> bool {
        self.health().is_some_and(|(hp, max)| hp < max)
    }

    /// The same structure with `hp` left (capped at its most).
    pub fn with_hp(self, hp: u32) -> Self {
        match self {
            Structure::Built {
                kind, committed, ..
            } => Structure::Built {
                kind,
                hp: hp.min(kind.max_hp()),
                committed,
            },
            other => other,
        }
    }
}

/// What a hit did to a structure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Damage {
    /// It took the hit and still stands, with this much HP out of `max`.
    Hurt { hp: u32, max: u32 },
    /// It broke: the change that cleared the cell.
    Broke(Change),
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

/// One cell's structure changing, e.g. a wall placed (`None` → `Some(WALL)`) or repaired (a
/// `Built` one → the same with more HP).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Change {
    pub cell: IVec2,
    pub before: Option<Structure>,
    pub after: Option<Structure>,
}

impl Change {
    /// Whether it puts HP back on something that stays standing, rather than building or
    /// removing it.
    pub fn is_repair(self) -> bool {
        match (self.before, self.after) {
            (Some(before), Some(after)) => {
                before.kind().is_some()
                    && before.kind() == after.kind()
                    && after.health() > before.health()
            }
            _ => false,
        }
    }

    /// The change that takes it back.
    pub fn inverse(self) -> Self {
        Self {
            cell: self.cell,
            before: self.after,
            after: self.before,
        }
    }
}

/// The Command Center's HP: what it has left, and the most it can have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandCenterHp {
    pub hp: u32,
    pub max: u32,
}

impl Default for CommandCenterHp {
    fn default() -> Self {
        Self {
            hp: COMMAND_CENTER_HP,
            max: COMMAND_CENTER_HP,
        }
    }
}

impl CommandCenterHp {
    /// Takes `damage`. Returns true if that used up the last of its HP.
    pub fn damage(&mut self, damage: u32) -> bool {
        let before = self.hp;
        self.hp = self.hp.saturating_sub(damage);
        before > 0 && self.hp == 0
    }

    pub fn damaged(self) -> bool {
        self.hp < self.max
    }

    pub fn fallen(self) -> bool {
        self.hp == 0
    }
}

/// The whole map.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct Grid {
    chunks: HashMap<IVec2, Chunk>,
    /// The Command Center's HP, for the whole building (it stands on several cells). It's kept
    /// with the map so that repairing it is an edit like any other, and can be undone (see
    /// [`crate::build::history`]).
    pub command_center: CommandCenterHp,
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

    /// Hits whatever stands on `cell` for `amount` of damage. `None` if nothing breakable stands
    /// there (e.g. it already broke).
    pub fn damage(&mut self, cell: IVec2, amount: u32) -> Option<Damage> {
        let data = self.cell_mut(cell)?;
        let structure = data.structure?;
        let (hp, max) = structure.health()?;
        let left = hp.saturating_sub(amount);
        if left > 0 {
            data.structure = Some(structure.with_hp(left));
            return Some(Damage::Hurt { hp: left, max });
        }
        self.set_structure(cell, None).map(Damage::Broke)
    }

    /// Applies a change if the cell still holds what it starts from, so taking back an edit
    /// never undoes something that happened since (e.g. a wall the horde broke, or wore down).
    /// Returns it if it changed anything.
    pub fn apply(&mut self, change: Change) -> Option<Change> {
        if self.structure(change.cell) != change.before {
            return None;
        }
        self.set_structure(change.cell, change.after)
    }

    /// Marks everything built so far as committed: it now sells at a loss. Not a [`Change`], since
    /// nothing on the map moves, and not undoable.
    pub fn commit(&mut self) {
        for chunk in self.chunks.values_mut() {
            for cell in &mut chunk.cells {
                if let Some(Structure::Built { committed, .. }) = &mut cell.structure {
                    *committed = true;
                }
            }
        }
    }

    /// Clears away everything the player built (a new run). Returns the changes.
    pub fn clear_built(&mut self) -> Vec<Change> {
        let cells: Vec<IVec2> = self
            .chunks
            .iter()
            .flat_map(|(&at, chunk)| {
                let origin = Self::chunk_origin(at);
                chunk
                    .cells
                    .iter()
                    .enumerate()
                    .filter_map(move |(index, cell)| {
                        let index = index as i32;
                        let local = IVec2::new(index % CHUNK_SIZE, index / CHUNK_SIZE);
                        matches!(cell.structure, Some(Structure::Built { .. }))
                            .then_some(origin + local)
                    })
            })
            .collect();
        cells
            .into_iter()
            .filter_map(|cell| self.set_structure(cell, None))
            .collect()
    }

    /// How many `kind`s stand on the map.
    pub fn count(&self, kind: Kind) -> usize {
        self.chunks
            .values()
            .flat_map(|chunk| &chunk.cells)
            .filter(|cell| cell.structure.and_then(Structure::kind) == Some(kind))
            .count()
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
    fn clearing_the_built_leaves_the_command_center() {
        let mut grid = two_by_two();
        let base = IVec2::new(-1, -1);
        grid.set_structure(base, Some(Structure::CommandCenter));
        let built = [IVec2::new(-5, 3), IVec2::new(7, -9), IVec2::new(0, 0)];
        for cell in built {
            grid.set_structure(cell, Some(Structure::new(Kind::Mortar)));
        }
        let mut cleared: Vec<IVec2> = grid.clear_built().iter().map(|c| c.cell).collect();
        cleared.sort_by_key(|cell| (cell.x, cell.y));
        let mut expected = built.to_vec();
        expected.sort_by_key(|cell| (cell.x, cell.y));
        assert_eq!(cleared, expected);
        assert!(built.iter().all(|&cell| grid.structure(cell).is_none()));
        assert_eq!(grid.structure(base), Some(Structure::CommandCenter));
    }

    #[test]
    fn the_command_center_falls_once() {
        let mut hp = CommandCenterHp { hp: 3, max: 3 };
        assert!(!hp.damage(2));
        assert!(hp.damaged());
        assert!(hp.damage(2), "the hit that takes the last HP");
        assert!(hp.fallen());
        assert!(!hp.damage(1), "already down");
    }

    #[test]
    fn counting_what_stands() {
        let mut grid = two_by_two();
        assert_eq!(grid.count(Kind::ArtilleryCommand), 0);
        let cells = [IVec2::new(-12, 5), IVec2::new(9, -3)];
        for cell in cells {
            grid.set_structure(cell, Some(Structure::new(Kind::ArtilleryCommand)));
        }
        grid.set_structure(IVec2::new(0, 4), Some(Structure::WALL));
        assert_eq!(grid.count(Kind::ArtilleryCommand), 2);
        assert_eq!(grid.count(Kind::Wall), 1);
        grid.damage(cells[0], 10_000);
        assert_eq!(grid.count(Kind::ArtilleryCommand), 1, "broken");
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
        grid.set_structure(cell, Some(Structure::WALL));
        assert_eq!(
            grid.check_build(cell),
            Err(Blocked::Occupied(Structure::WALL))
        );
        assert_eq!(grid.check_build(rock), Err(Blocked::Terrain(Terrain::Rock)));
        let mud = IVec2::new(-6, -6);
        grid.set_terrain(mud, Terrain::Mud);
        assert_eq!(grid.check_build(mud), Err(Blocked::Terrain(Terrain::Mud)));
        assert!(Terrain::Mud.walkable());
        assert_eq!(
            grid.check_build(IVec2::new(999, 0)),
            Err(Blocked::OutsideMap)
        );
    }

    #[test]
    fn changes_report_what_happened() {
        let mut grid = two_by_two();
        let cell = IVec2::new(5, -7);
        let placed = grid.set_structure(cell, Some(Structure::WALL)).unwrap();
        assert_eq!(placed.before, None);
        assert_eq!(placed.after, Some(Structure::WALL));
        assert_eq!(
            grid.set_structure(cell, Some(Structure::WALL)),
            None,
            "no-op"
        );
        assert_eq!(grid.apply(placed.inverse()), Some(placed.inverse()));
        assert_eq!(grid.structure(cell), None);
        assert_eq!(
            grid.set_structure(IVec2::new(999, 0), Some(Structure::WALL)),
            None
        );
    }

    #[test]
    fn walls_wear_down_and_break() {
        let mut grid = two_by_two();
        let cell = IVec2::new(2, 2);
        assert_eq!(grid.damage(cell, 5), None, "nothing to hit");
        grid.set_structure(cell, Some(Structure::WALL));
        let max = WALL_HP;
        assert_eq!(
            grid.damage(cell, 30),
            Some(Damage::Hurt { hp: max - 30, max })
        );
        assert_eq!(
            grid.structure(cell),
            Some(Structure::WALL.with_hp(max - 30))
        );
        // The hit that takes the last HP clears the cell, and says what was there.
        let Some(Damage::Broke(change)) = grid.damage(cell, max) else {
            panic!("should break");
        };
        assert_eq!(change.before, Some(Structure::WALL.with_hp(max - 30)));
        assert_eq!(change.after, None);
        assert_eq!(grid.structure(cell), None);
        assert_eq!(grid.damage(cell, 1), None, "already broken");
        // The Command Center isn't broken through the grid.
        grid.set_structure(cell, Some(Structure::CommandCenter));
        assert_eq!(grid.damage(cell, 1), None);
    }

    #[test]
    fn towers_break_like_walls() {
        let mut grid = two_by_two();
        let cell = IVec2::new(1, 1);
        grid.set_structure(cell, Some(Structure::new(Kind::Mortar)));
        let max = Kind::Mortar.max_hp();
        assert_eq!(
            grid.damage(cell, 20),
            Some(Damage::Hurt { hp: max - 20, max })
        );
        assert!(grid.structure(cell).unwrap().damaged());
        assert!(matches!(grid.damage(cell, max), Some(Damage::Broke(_))));
        assert_eq!(grid.structure(cell), None);
    }

    #[test]
    fn repairs_and_commits() {
        let mut grid = two_by_two();
        let cell = IVec2::new(0, 3);
        grid.set_structure(cell, Some(Structure::WALL));
        grid.damage(cell, 60);
        let worn = grid.structure(cell).unwrap();
        let repair = grid.set_structure(cell, Some(worn.with_hp(999))).unwrap();
        assert!(repair.is_repair());
        assert_eq!(
            grid.structure(cell),
            Some(Structure::WALL),
            "capped at full HP"
        );
        assert!(
            !repair.inverse().is_repair(),
            "taking HP off isn't a repair"
        );
        let removed = Change {
            cell,
            before: Some(Structure::WALL),
            after: None,
        };
        assert!(!removed.is_repair());
        let swapped = Change {
            cell,
            before: Some(Structure::WALL.with_hp(10)),
            after: Some(Structure::new(Kind::Flamer)),
        };
        assert!(!swapped.is_repair(), "another kind");

        grid.commit();
        assert_eq!(
            grid.structure(cell),
            Some(Structure::Built {
                kind: Kind::Wall,
                hp: WALL_HP,
                committed: true
            })
        );
        grid.set_structure(IVec2::new(-1, -1), Some(Structure::CommandCenter));
        grid.commit();
        assert_eq!(
            grid.structure(IVec2::new(-1, -1)),
            Some(Structure::CommandCenter)
        );
    }

    #[test]
    fn changes_only_apply_to_what_they_started_from() {
        let mut grid = two_by_two();
        let cell = IVec2::new(4, 4);
        let placed = grid.set_structure(cell, Some(Structure::WALL)).unwrap();
        // The horde wears it down: taking the placement back no longer matches.
        grid.damage(cell, 10);
        assert_eq!(grid.apply(placed.inverse()), None);
        assert!(grid.structure(cell).is_some());
        // And once it broke, doing it again doesn't put it back over whatever is there.
        grid.damage(cell, 1000);
        grid.set_structure(cell, Some(Structure::new(Kind::Flamer)));
        assert_eq!(grid.apply(placed), None);
    }

    #[test]
    fn undoing_a_removal_brings_the_damage_back() {
        let mut grid = two_by_two();
        let cell = IVec2::new(-3, 4);
        grid.set_structure(cell, Some(Structure::WALL));
        grid.damage(cell, 40);
        let removed = grid.set_structure(cell, None).unwrap();
        grid.apply(removed.inverse());
        assert_eq!(
            grid.structure(cell),
            Some(Structure::WALL.with_hp(WALL_HP - 40))
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
