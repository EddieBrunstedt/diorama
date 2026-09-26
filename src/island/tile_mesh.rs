//! Voxel models for island tiles: a colorful top surface over a chunk of floating rock.
//!
//! Everything here is in voxel units, relative to the center of the tile's top surface.
//! A tile spans -5..5 on x and z; the grass surface is at y = 0 and the rock hangs below it.

use super::grid::{Island, Side, TileKind};
use super::trees::{TreeSpot, tree_spots};
use crate::palette;
use crate::rng::Rng;
use crate::voxel::{VOXELS_PER_TILE, VoxelMesh, shade, vary};
use bevy::prelude::*;

const N: usize = VOXELS_PER_TILE as usize;
const HALF: f32 = VOXELS_PER_TILE / 2.0;

/// Bottom of the surface layer (grass, road, cobbles). Soil starts below it.
const SURFACE_BOTTOM: f32 = -1.2;
/// Bottom of the dirt layer. Stone starts below it.
const DIRT_BOTTOM: f32 = -3.2;
/// How far the road surface sits below the grass.
const ROAD_TOP: f32 = -0.35;
/// Road width in voxels, centered on the tile.
const ROAD_FROM: usize = 3;
const ROAD_TO: usize = 7;
/// Rock depth under an edge tile, and how much deeper each ring toward the middle gets.
const EDGE_DEPTH: f32 = 8.0;
const DEPTH_PER_RING: f32 = 9.0;

const SURFACE_SALT: u64 = 1;

/// What a single surface voxel is made of.
#[derive(Clone, Copy, PartialEq)]
enum Surface {
    Grass,
    Road,
    Cobble,
}

pub fn tile_mesh(island: &Island, cell: IVec2) -> VoxelMesh {
    let kind = island
        .get(cell)
        .expect("tile_mesh called for an empty cell");
    let mut rng = Rng::for_cell(cell, SURFACE_SALT);
    let mut mesh = VoxelMesh::default();

    let surfaces = surface_layout(kind, &island.road_sides(cell), &mut rng);
    let open_sides: Vec<Side> = Side::ALL
        .into_iter()
        .filter(|side| island.get(cell + side.offset()).is_none())
        .collect();

    add_surface(&mut mesh, kind, &surfaces, &mut rng);
    add_rock(&mut mesh, island.edge_distance(cell), &open_sides, &mut rng);
    add_grass_drips(&mut mesh, kind, &surfaces, &open_sides, &mut rng);
    match kind {
        TileKind::Meadow => add_meadow_details(&mut mesh, &surfaces, &mut rng),
        TileKind::Forest => add_forest_details(&mut mesh, &tree_spots(cell), &mut rng),
        TileKind::Road | TileKind::Hearth => {}
    }
    mesh
}

/// Local voxel coordinate of the low corner of surface voxel `(i, j)`.
fn corner(i: usize, j: usize) -> (f32, f32) {
    (i as f32 - HALF, j as f32 - HALF)
}

fn surface_layout(kind: TileKind, road_sides: &[Side], rng: &mut Rng) -> [[Surface; N]; N] {
    let mut layout = [[Surface::Grass; N]; N];
    let road = |i: usize| (ROAD_FROM..ROAD_TO).contains(&i);
    if !road_sides.is_empty() {
        for (i, column) in layout.iter_mut().enumerate() {
            for (j, surface) in column.iter_mut().enumerate() {
                let arm = road_sides.iter().any(|side| match side {
                    Side::North => road(i) && j < ROAD_TO,
                    Side::South => road(i) && j >= ROAD_FROM,
                    Side::West => road(j) && i < ROAD_TO,
                    Side::East => road(j) && i >= ROAD_FROM,
                });
                if (road(i) && road(j)) || arm {
                    *surface = Surface::Road;
                }
            }
        }
        // Wobble the road's edges a little so it looks trodden rather than drawn. Tile borders
        // stay untouched so road arms always line up with the neighboring tile.
        let before = layout;
        for i in 1..N - 1 {
            for j in 1..N - 1 {
                let next_to_road = [(i - 1, j), (i + 1, j), (i, j - 1), (i, j + 1)]
                    .iter()
                    .any(|&(a, b)| before[a][b] == Surface::Road);
                if before[i][j] == Surface::Grass && next_to_road && rng.chance(0.18) {
                    layout[i][j] = Surface::Road;
                }
            }
        }
    }
    if kind == TileKind::Hearth {
        for column in &mut layout[1..N - 1] {
            for surface in &mut column[1..N - 1] {
                *surface = Surface::Cobble;
            }
        }
    }
    layout
}

fn add_surface(mesh: &mut VoxelMesh, kind: TileKind, surfaces: &[[Surface; N]; N], rng: &mut Rng) {
    for (i, column) in surfaces.iter().enumerate() {
        for (j, surface) in column.iter().enumerate() {
            let (x, z) = corner(i, j);
            let (color, top) = match surface {
                Surface::Grass => {
                    let base = match kind {
                        TileKind::Forest if rng.chance(0.3) => palette::FOREST_MOSS,
                        TileKind::Forest => palette::FOREST_GRASS,
                        _ => palette::MEADOW_GRASS,
                    };
                    (vary(base, rng, 0.035, 6.0), 0.0)
                }
                Surface::Road => {
                    if rng.chance(0.06) {
                        (vary(palette::PEBBLE, rng, 0.05, 10.0), ROAD_TOP + 0.15)
                    } else {
                        (vary(palette::ROAD, rng, 0.03, 4.0), ROAD_TOP)
                    }
                }
                Surface::Cobble => (
                    vary(palette::COBBLE, rng, 0.06, 10.0),
                    rng.pick(&[0.0, 0.08, 0.16]),
                ),
            };
            mesh.cuboid(
                Vec3::new(x, SURFACE_BOTTOM, z),
                Vec3::new(x + 1.0, top, z + 1.0),
                color,
            );
        }
    }
}

/// Dirt, then layered stone that gets deeper toward the island's middle, so all tiles together
/// form one tapered floating rock.
fn add_rock(mesh: &mut VoxelMesh, edge_distance: u32, open_sides: &[Side], rng: &mut Rng) {
    let depth = EDGE_DEPTH + DEPTH_PER_RING * (edge_distance.max(1) - 1) as f32;
    let bottom = -(depth + rng.range(0.0, 3.0)).round();

    let dirt = vary(palette::DIRT, rng, 0.02, 4.0);
    mesh.cuboid_graded(
        Vec3::new(-HALF, DIRT_BOTTOM, -HALF),
        Vec3::new(HALF, SURFACE_BOTTOM, HALF),
        dirt,
        palette::DIRT_DARK,
    );

    // Stone strata. Faces on the island's outside get small random ledges and recesses.
    let deepest = -(EDGE_DEPTH + DEPTH_PER_RING * 2.0 + 3.0);
    let stone_at = |y: f32| {
        let t = (y / deepest).clamp(0.0, 1.0);
        palette::STONE.mix(&palette::STONE_DEEP, t)
    };
    let mut top = DIRT_BOTTOM;
    while top > bottom {
        let layer_bottom = (top - rng.range(2.0, 4.0).round()).max(bottom);
        let mut min = Vec3::new(-HALF, layer_bottom, -HALF);
        let mut max = Vec3::new(HALF, top, HALF);
        for side in open_sides {
            let ledge = rng.pick(&[-0.3, 0.0, 0.0, 0.2]);
            match side {
                Side::North => min.z -= ledge,
                Side::South => max.z += ledge,
                Side::West => min.x -= ledge,
                Side::East => max.x += ledge,
            }
        }
        let upper = vary(stone_at(top), rng, 0.06, 10.0);
        mesh.cuboid_graded(min, max, upper, shade(stone_at(layer_bottom), 0.9));
        top = layer_bottom;
    }

    // A couple of rocks hanging from the bottom for a less boxy silhouette.
    for _ in 0..rng.index(3) {
        let size = Vec2::new(rng.range(2.0, 5.0), rng.range(2.0, 5.0)).round();
        let x = rng.range(-HALF, HALF - size.x).round();
        let z = rng.range(-HALF, HALF - size.y).round();
        let height = rng.range(1.0, 3.0).round();
        let color = vary(palette::STONE_DEEP, rng, 0.03, 8.0);
        mesh.cuboid_graded(
            Vec3::new(x, bottom - height, z),
            Vec3::new(x + size.x, bottom, z + size.y),
            color,
            shade(color, 0.7),
        );
    }
}

/// Grass spilling over the island's outer edges.
fn add_grass_drips(
    mesh: &mut VoxelMesh,
    kind: TileKind,
    surfaces: &[[Surface; N]; N],
    open_sides: &[Side],
    rng: &mut Rng,
) {
    const OUT: f32 = 0.15;
    let grass = match kind {
        TileKind::Forest => palette::FOREST_GRASS,
        _ => palette::MEADOW_GRASS,
    };
    for side in open_sides {
        for k in 0..N {
            let (i, j) = match side {
                Side::North => (k, 0),
                Side::South => (k, N - 1),
                Side::West => (0, k),
                Side::East => (N - 1, k),
            };
            if surfaces[i][j] != Surface::Grass || !rng.chance(0.45) {
                continue;
            }
            let (x, z) = corner(i, j);
            let length = rng.range(0.5, 2.2);
            let (min_xz, max_xz) = match side {
                Side::North => (Vec2::new(x, -HALF - OUT), Vec2::new(x + 1.0, -HALF + 0.5)),
                Side::South => (Vec2::new(x, HALF - 0.5), Vec2::new(x + 1.0, HALF + OUT)),
                Side::West => (Vec2::new(-HALF - OUT, z), Vec2::new(-HALF + 0.5, z + 1.0)),
                Side::East => (Vec2::new(HALF - 0.5, z), Vec2::new(HALF + OUT, z + 1.0)),
            };
            let color = vary(shade(grass, 0.9), rng, 0.03, 6.0);
            mesh.cuboid(
                Vec3::new(min_xz.x, SURFACE_BOTTOM - length, min_xz.y),
                Vec3::new(max_xz.x, -0.3, max_xz.y),
                color,
            );
        }
    }
}

/// Picks a random grass voxel and returns its low corner, if one is found.
fn random_grass_spot(surfaces: &[[Surface; N]; N], rng: &mut Rng) -> Option<(f32, f32)> {
    (0..8).find_map(|_| {
        let (i, j) = (rng.index(N), rng.index(N));
        (surfaces[i][j] == Surface::Grass).then(|| corner(i, j))
    })
}

fn add_meadow_details(mesh: &mut VoxelMesh, surfaces: &[[Surface; N]; N], rng: &mut Rng) {
    for _ in 0..4 + rng.index(5) {
        let Some((x, z)) = random_grass_spot(surfaces, rng) else {
            continue;
        };
        // A tuft is two thin blades of slightly different height.
        let color = vary(palette::MEADOW_TUFT, rng, 0.03, 6.0);
        for (dx, dz) in [(0.15, 0.2), (0.5, 0.5)] {
            let height = rng.range(0.4, 0.8);
            mesh.cuboid(
                Vec3::new(x + dx, 0.0, z + dz),
                Vec3::new(x + dx + 0.3, height, z + dz + 0.3),
                color,
            );
        }
    }
    for _ in 0..1 + rng.index(4) {
        let Some((x, z)) = random_grass_spot(surfaces, rng) else {
            continue;
        };
        let (cx, cz) = (x + 0.5, z + 0.5);
        let stem = shade(palette::MEADOW_TUFT, 0.8);
        mesh.cuboid(
            Vec3::new(cx - 0.08, 0.0, cz - 0.08),
            Vec3::new(cx + 0.08, 0.4, cz + 0.08),
            stem,
        );
        let petals = vary(rng.pick(&palette::FLOWERS), rng, 0.03, 6.0);
        mesh.cuboid(
            Vec3::new(cx - 0.25, 0.4, cz - 0.25),
            Vec3::new(cx + 0.25, 0.8, cz + 0.25),
            petals,
        );
    }
}

fn add_forest_details(mesh: &mut VoxelMesh, trees: &[TreeSpot], rng: &mut Rng) {
    for _ in 0..rng.index(3) {
        let (cx, cz) = (rng.range(-4.0, 4.0), rng.range(-4.0, 4.0));
        let clear_of_trees = trees
            .iter()
            .all(|tree| tree.position.distance(Vec2::new(cx, cz)) > 1.6);
        if !clear_of_trees {
            continue;
        }
        let cap = vary(palette::MUSHROOM_CAP, rng, 0.04, 6.0);
        mesh.cuboid(
            Vec3::new(cx - 0.18, 0.0, cz - 0.18),
            Vec3::new(cx + 0.18, 0.5, cz + 0.18),
            palette::MUSHROOM_STEM,
        );
        mesh.cuboid(
            Vec3::new(cx - 0.45, 0.5, cz - 0.45),
            Vec3::new(cx + 0.45, 0.85, cz + 0.45),
            cap,
        );
    }
}
