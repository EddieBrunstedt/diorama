//! Chunk templates as plain data: hand-drawn 16×16 maps, in the starting map's format (see
//! [`crate::map::terrain_of`]), turned to face the map they're added to and maybe mirrored.
//!
//! Every template is drawn as if the map were below it: its bottom row is the edge that meets the
//! map (always open ground, so the new chunk joins the map), and a nest, if it has one, sits near
//! the top, far from the map.

use crate::grid::{Chunk, Footprint, Grid, Terrain};
use crate::map::{find_nests, terrain_of};
use crate::tuning::grid::CHUNK_SIZE;
use bevy::prelude::*;

/// A hand-drawn chunk.
#[derive(Debug)]
pub struct Template {
    pub name: &'static str,
    /// Row by row from the far edge down to the edge that meets the map.
    pub rows: [&'static str; CHUNK_SIZE as usize],
}

impl Template {
    /// The terrain drawn at `cell` (template coordinates).
    fn terrain(&self, cell: IVec2) -> Terrain {
        let char = self.rows[cell.y as usize].as_bytes()[cell.x as usize] as char;
        terrain_of(char).expect("templates are checked by the tests")
    }

    pub fn has_nest(&self) -> bool {
        self.rows.iter().any(|row| row.contains('N'))
    }
}

/// The templates chunks are made from. Four without a nest, four with one.
pub const TEMPLATES: [Template; 8] = [
    Template {
        name: "Clearing",
        rows: [
            "................",
            "................",
            "....##..........",
            "....##......#...",
            "...........##...",
            "................",
            "................",
            "..........~~....",
            ".#........~~....",
            ".##.............",
            "................",
            ".......##.......",
            ".......#........",
            "................",
            "................",
            "................",
        ],
    },
    Template {
        name: "Bog",
        rows: [
            "................",
            "...~~~~.........",
            "..~~~~~~~~......",
            ".~~~~~~~~~~~....",
            ".~~~~..~~~~~~...",
            "..~~....~~~~~~..",
            "..~~~...~~~~~~..",
            "...~~~~~~~~~~...",
            "....~~~~~~~~....",
            ".....~~~~~......",
            "................",
            ".......#........",
            "................",
            "..~~~...........",
            "..~~~~..........",
            "................",
        ],
    },
    Template {
        name: "Pass",
        rows: [
            "................",
            "................",
            "..#.........#...",
            "................",
            "######..########",
            "#######..#######",
            ".######..######.",
            "..####....####..",
            "................",
            "................",
            "...#............",
            "................",
            "..........##....",
            "..........##....",
            "................",
            "................",
        ],
    },
    Template {
        name: "Boulders",
        rows: [
            "................",
            "..##......##....",
            "..##......##....",
            "......##.......#",
            "......##......##",
            ".##.............",
            ".##.......##....",
            "..........##....",
            "....##..........",
            "....##.....##...",
            "...........##...",
            ".##.............",
            ".##.....##......",
            "........##......",
            "................",
            "................",
        ],
    },
    Template {
        name: "Open nest",
        rows: [
            "................",
            ".......NN.......",
            ".......NN.......",
            "................",
            "...#........#...",
            "................",
            "................",
            ".....~~.........",
            ".....~~.....##..",
            "............##..",
            "................",
            "..##............",
            "..##............",
            "................",
            "................",
            "................",
        ],
    },
    Template {
        name: "Moat",
        rows: [
            "................",
            ".....~~~~~~.....",
            "....~~~~~~~~....",
            "....~~~NN~~~....",
            "....~~~NN~~~....",
            "....~~~~~~~~....",
            ".....~~~~~~.....",
            "................",
            "................",
            "..#..........#..",
            ".###........###.",
            "................",
            "................",
            "................",
            "................",
            "................",
        ],
    },
    Template {
        name: "Gully",
        rows: [
            ".....######.....",
            "....###NN###....",
            "....##.NN..##...",
            "....##......##..",
            "....##......##..",
            ".....##....##...",
            ".....##....##...",
            "......#....#....",
            "................",
            "................",
            ".#............#.",
            ".##..........##.",
            "................",
            "................",
            "................",
            "................",
        ],
    },
    Template {
        name: "Two mouths",
        rows: [
            "................",
            "......NN........",
            "......NN........",
            "................",
            "................",
            "###.#########...",
            "###.#########...",
            "................",
            "................",
            ".....~~.........",
            ".....~~~........",
            "................",
            ".........##.....",
            ".........##.....",
            "................",
            "................",
        ],
    },
];

/// A side of a chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    /// Toward -y (up the screen).
    North,
    /// Toward +y (down the screen).
    South,
    East,
    West,
}

impl Side {
    pub const ALL: [Side; 4] = [Side::North, Side::South, Side::East, Side::West];

    /// The step to the chunk on this side.
    pub fn step(self) -> IVec2 {
        match self {
            Side::North => IVec2::NEG_Y,
            Side::South => IVec2::Y,
            Side::East => IVec2::X,
            Side::West => IVec2::NEG_X,
        }
    }
}

/// How a template is laid down: which side of the chunk its bottom edge (the one that meets the
/// map) ends up on, and whether it's mirrored along that edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Orientation {
    pub facing: Side,
    pub mirror: bool,
}

impl Orientation {
    /// The template cell drawn at `local` (a cell of the chunk). Always a turn, then maybe a
    /// mirror, so the template is never skewed.
    pub fn template_cell(self, local: IVec2) -> IVec2 {
        let n = CHUNK_SIZE - 1;
        let (x, y) = (local.x, local.y);
        // Turn the template so its bottom row lies along the facing side.
        let turned = match self.facing {
            Side::South => IVec2::new(x, y),
            Side::North => IVec2::new(n - x, n - y),
            Side::East => IVec2::new(n - y, x),
            Side::West => IVec2::new(y, n - x),
        };
        if self.mirror {
            IVec2::new(n - turned.x, turned.y)
        } else {
            turned
        }
    }
}

/// A chunk to add to the map: which template, where, and turned which way. Everything about it
/// follows from these, so it's cheap to keep in the undo history.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Claim {
    /// Where the chunk goes (chunk coordinates).
    pub slot: IVec2,
    /// An index into [`TEMPLATES`].
    pub template: usize,
    pub orientation: Orientation,
}

impl Claim {
    pub fn template(self) -> &'static Template {
        &TEMPLATES[self.template]
    }

    /// The chunk as it goes onto the map.
    pub fn chunk(self) -> Chunk {
        let mut chunk = Chunk::new(Terrain::Ground);
        for local in local_cells() {
            let terrain = self
                .template()
                .terrain(self.orientation.template_cell(local));
            chunk.set_terrain(local, terrain);
        }
        chunk
    }

    /// The nest it holds, if it has one (map cells).
    pub fn nest(self) -> Option<Footprint> {
        if !self.template().has_nest() {
            return None;
        }
        let origin = Grid::chunk_origin(self.slot);
        let chunk = self.chunk();
        let mut cells: Vec<IVec2> = local_cells()
            .filter(|&local| chunk.get(local).terrain == Terrain::Nest)
            .map(|local| origin + local)
            .collect();
        cells.sort_by_key(|cell| (cell.y, cell.x));
        find_nests(&cells).first().copied()
    }

    /// Adds the chunk (and its nest) to the map. False if there's a chunk there already.
    pub fn add_to(self, grid: &mut Grid) -> bool {
        if grid.chunk(self.slot).is_some() {
            return false;
        }
        grid.insert_chunk(self.slot, self.chunk());
        grid.nests.extend(self.nest());
        true
    }

    /// Whether [`Claim::take_off`] would work: the chunk is on the map, with nothing built on it.
    pub fn can_take_off(self, grid: &Grid) -> bool {
        grid.chunk(self.slot).is_some_and(Chunk::is_bare)
    }

    /// Takes the chunk (and its nest) off the map again. False if it isn't there, or something
    /// stands on it.
    pub fn take_off(self, grid: &mut Grid) -> bool {
        if !self.can_take_off(grid) {
            return false;
        }
        grid.remove_chunk(self.slot);
        if let Some(nest) = self.nest() {
            grid.nests.retain(|&other| other != nest);
        }
        true
    }
}

/// Every cell of a chunk, in local coordinates, row by row.
fn local_cells() -> impl Iterator<Item = IVec2> {
    (0..CHUNK_SIZE * CHUNK_SIZE).map(|i| IVec2::new(i % CHUNK_SIZE, i / CHUNK_SIZE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::Structure;
    use crate::tuning::grid::NEST_SIZE;

    #[test]
    fn templates_are_well_drawn() {
        for template in &TEMPLATES {
            let name = template.name;
            for row in &template.rows {
                assert_eq!(row.len(), CHUNK_SIZE as usize, "{name}: {row:?}");
                for char in row.chars() {
                    assert!(terrain_of(char).is_some(), "{name}: {char:?}");
                }
            }
            // The edge that meets the map is open, so the chunk always joins it.
            assert!(
                template.rows[CHUNK_SIZE as usize - 1]
                    .chars()
                    .all(|c| c == '.'),
                "{name}"
            );
            // A nest is one whole nest, in the far half.
            let nests: usize = template
                .rows
                .iter()
                .map(|row| row.matches('N').count())
                .sum();
            if template.has_nest() {
                assert_eq!(nests as i32, NEST_SIZE * NEST_SIZE, "{name}");
                let row = template.rows.iter().position(|row| row.contains('N'));
                assert!(row.unwrap() < CHUNK_SIZE as usize / 2, "{name}");
            }
        }
        let with_nests = TEMPLATES.iter().filter(|t| t.has_nest()).count();
        assert!(with_nests > 0 && with_nests < TEMPLATES.len());
    }

    fn orientations() -> impl Iterator<Item = Orientation> {
        Side::ALL.into_iter().flat_map(|facing| {
            [false, true]
                .into_iter()
                .map(move |mirror| Orientation { facing, mirror })
        })
    }

    #[test]
    fn every_orientation_lays_the_whole_template_down() {
        for orientation in orientations() {
            let mut seen: Vec<IVec2> = local_cells()
                .map(|local| orientation.template_cell(local))
                .collect();
            seen.sort_by_key(|cell| (cell.y, cell.x));
            seen.dedup();
            assert_eq!(
                seen.len(),
                (CHUNK_SIZE * CHUNK_SIZE) as usize,
                "{orientation:?}"
            );
            assert!(
                seen.iter().all(|cell| cell.cmpge(IVec2::ZERO).all()
                    && cell.cmplt(IVec2::splat(CHUNK_SIZE)).all())
            );
        }
    }

    #[test]
    fn the_bottom_edge_faces_the_map() {
        let n = CHUNK_SIZE - 1;
        for orientation in orientations() {
            // The cells along the facing side are the template's bottom row.
            let edge: Vec<IVec2> = (0..CHUNK_SIZE)
                .map(|i| match orientation.facing {
                    Side::South => IVec2::new(i, n),
                    Side::North => IVec2::new(i, 0),
                    Side::East => IVec2::new(n, i),
                    Side::West => IVec2::new(0, i),
                })
                .collect();
            for local in edge {
                assert_eq!(orientation.template_cell(local).y, n, "{orientation:?}");
            }
        }
    }

    #[test]
    fn a_turn_keeps_the_template_unmirrored() {
        // An L drawn in the template's top left: its foot points right. Turned (without a
        // mirror), a right-handed corner stays right-handed.
        let n = CHUNK_SIZE - 1;
        for facing in Side::ALL {
            let orientation = Orientation {
                facing,
                mirror: false,
            };
            let at = |local: IVec2| orientation.template_cell(local);
            // Cross product of the template's x and y axes as they land on the map.
            let origin = local_cells().find(|&l| at(l) == IVec2::ZERO).unwrap();
            let x = local_cells().find(|&l| at(l) == IVec2::X).unwrap() - origin;
            let y = local_cells().find(|&l| at(l) == IVec2::Y).unwrap() - origin;
            assert_eq!(x.perp_dot(y), 1, "{facing:?}");
            let mirrored = Orientation {
                facing,
                mirror: true,
            };
            assert_eq!(
                mirrored.template_cell(IVec2::ZERO).x,
                n - orientation.template_cell(IVec2::ZERO).x
            );
        }
    }

    #[test]
    fn claims_bring_their_nest() {
        let open_nest = TEMPLATES
            .iter()
            .position(|t| t.name == "Open nest")
            .unwrap();
        for orientation in orientations() {
            let claim = Claim {
                slot: IVec2::new(2, -1),
                template: open_nest,
                orientation,
            };
            let nest = claim.nest().expect("a nest");
            let chunk = claim.chunk();
            let origin = Grid::chunk_origin(claim.slot);
            for cell in nest.cells() {
                assert_eq!(chunk.get(cell - origin).terrain, Terrain::Nest);
            }
            // Far from the side that meets the map.
            let local = nest.center().xz() - origin.as_vec2();
            let toward_map = orientation.facing.step().as_vec2();
            let middle = Vec2::splat(CHUNK_SIZE as f32 / 2.0);
            assert!((local - middle).dot(toward_map) < -3.0, "{orientation:?}");
        }
        let clearing = Claim {
            slot: IVec2::ZERO,
            template: 0,
            orientation: Orientation {
                facing: Side::South,
                mirror: false,
            },
        };
        assert_eq!(clearing.nest(), None);
    }

    #[test]
    fn claiming_and_taking_back() {
        let mut grid = Grid::default();
        grid.insert_chunk(IVec2::ZERO, Chunk::new(Terrain::Ground));
        let claim = Claim {
            slot: IVec2::new(0, -1),
            template: TEMPLATES.iter().position(|t| t.name == "Moat").unwrap(),
            orientation: Orientation {
                facing: Side::South,
                mirror: false,
            },
        };
        assert!(claim.add_to(&mut grid));
        assert!(!claim.add_to(&mut grid), "already there");
        assert_eq!(grid.nests, vec![claim.nest().unwrap()]);
        assert_eq!(
            grid.cell(IVec2::new(0, -1)).unwrap().terrain,
            Terrain::Ground
        );

        // Something built on it keeps it on the map.
        let cell = IVec2::new(1, -2);
        grid.set_structure(cell, Some(Structure::WALL));
        assert!(!claim.take_off(&mut grid));
        grid.set_structure(cell, None);
        assert!(claim.take_off(&mut grid));
        assert!(grid.chunk(claim.slot).is_none());
        assert!(grid.nests.is_empty());
        assert!(!claim.take_off(&mut grid), "already gone");
    }
}
