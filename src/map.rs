//! The map: the starting [`Grid`] and the ground it's drawn as, one mesh per chunk.

use crate::grid::{Chunk, Grid, Terrain};
use crate::palette;
use crate::rng::Rng;
use crate::shapes::{BoxMesh, ShapeMaterial, shade, vary};
use crate::tuning::grid::{CHUNK_SIZE, STARTING_CHUNKS};
use bevy::prelude::*;

/// Gap between neighboring cells' tiles, and the width of the seam along chunk borders.
const CELL_GAP: f32 = 0.06;
const CHUNK_SEAM: f32 = 0.12;
/// Thickness of the slab the map sits on, so its edge reads as an edge.
const SLAB_DEPTH: f32 = 0.5;
/// Rocks are blocks of varying height.
const ROCK_HEIGHT: (f32, f32) = (0.45, 0.8);
/// Salts for the per-cell random numbers.
const SALT_TILE: u64 = 1;
const SALT_ROCK: u64 = 2;

/// Rock outcrops on the starting map, as rectangles of cells (`min`, `max` inclusive). A stand-in
/// until the hand-authored map of milestone 7.
const STARTING_ROCKS: &[((i32, i32), (i32, i32))] = &[
    ((-11, -9), (-9, -8)),
    ((-10, -7), (-10, -6)),
    ((6, 4), (7, 6)),
    ((8, 6), (8, 6)),
    ((-4, 9), (-1, 9)),
    ((3, -12), (3, -10)),
];

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(starting_map())
            .init_resource::<ShapeMaterial>()
            .add_systems(Startup, spawn_ground);
    }
}

/// `STARTING_CHUNKS`×`STARTING_CHUNKS` chunks of bare ground around the origin, with a few rocks.
pub fn starting_map() -> Grid {
    let mut grid = Grid::default();
    let low = -STARTING_CHUNKS / 2;
    for y in low..low + STARTING_CHUNKS {
        for x in low..low + STARTING_CHUNKS {
            grid.insert_chunk(IVec2::new(x, y), Chunk::new(Terrain::Ground));
        }
    }
    for &((x0, y0), (x1, y1)) in STARTING_ROCKS {
        for y in y0..=y1 {
            for x in x0..=x1 {
                grid.set_terrain(IVec2::new(x, y), Terrain::Rock);
            }
        }
    }
    grid
}

/// How tall the terrain on a cell stands, for putting things on top of it.
pub fn terrain_height(cell: IVec2, terrain: Terrain) -> f32 {
    match terrain {
        Terrain::Ground => 0.0,
        Terrain::Rock => {
            let (low, high) = ROCK_HEIGHT;
            Rng::for_cell(cell, SALT_ROCK).range(low, high)
        }
    }
}

fn spawn_ground(
    mut commands: Commands,
    grid: Res<Grid>,
    material: Res<ShapeMaterial>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (at, _) in grid.chunks() {
        let origin = Grid::chunk_origin(at);
        commands.spawn((
            Mesh3d(meshes.add(chunk_mesh(&grid, at))),
            MeshMaterial3d(material.0.clone()),
            Transform::from_xyz(origin.x as f32, 0.0, origin.y as f32),
        ));
    }
}

/// A chunk's ground in chunk-local coordinates: a slab showing through as grid lines, a tile per
/// cell, and rock blocks.
fn chunk_mesh(grid: &Grid, at: IVec2) -> Mesh {
    let size = CHUNK_SIZE as f32;
    let origin = Grid::chunk_origin(at);
    let mut mesh = BoxMesh::default();
    mesh.cuboid_graded(
        Vec3::new(0.0, -SLAB_DEPTH, 0.0),
        Vec3::new(size, 0.0, size),
        palette::CHUNK_SEAM,
        shade(palette::GROUND_GAP, 0.5),
    );
    let seam = CHUNK_SEAM / 2.0;
    mesh.quad(
        Vec2::splat(seam),
        Vec2::splat(size - seam),
        0.001,
        palette::GROUND_GAP,
    );
    for y in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let local = IVec2::new(x, y);
            let cell = origin + local;
            let Some(data) = grid.cell(cell) else {
                continue;
            };
            // Cells on the chunk's border leave room for the seam.
            let gap = |edge: bool| if edge { seam } else { CELL_GAP / 2.0 };
            let min = local.as_vec2() + Vec2::new(gap(x == 0), gap(y == 0));
            let max = (local + IVec2::ONE).as_vec2()
                - Vec2::new(gap(x == CHUNK_SIZE - 1), gap(y == CHUNK_SIZE - 1));
            let mut rng = Rng::for_cell(cell, SALT_TILE);
            match data.terrain {
                Terrain::Ground => {
                    let color = vary(palette::GROUND, &mut rng, 0.012, 4.0);
                    mesh.quad(min, max, 0.002, color);
                }
                Terrain::Rock => {
                    // Rock fills the whole cell, so neighboring rocks merge into one outcrop,
                    // with a lump on top so it reads as natural rather than built.
                    let color = vary(palette::ROCK, &mut rng, 0.03, 6.0);
                    let height = terrain_height(cell, data.terrain);
                    let (low, high) = (local.as_vec2(), (local + IVec2::ONE).as_vec2());
                    mesh.cuboid(
                        Vec3::new(low.x, 0.0, low.y),
                        Vec3::new(high.x, height, high.y),
                        color,
                    );
                    let size = rng.range(0.35, 0.6);
                    let at =
                        low + Vec2::new(rng.range(0.1, 0.9 - size), rng.range(0.1, 0.9 - size));
                    mesh.cuboid(
                        Vec3::new(at.x, height, at.y),
                        Vec3::new(at.x + size, height + rng.range(0.1, 0.25), at.y + size),
                        shade(color, 1.15),
                    );
                }
            }
        }
    }
    mesh.build()
}
