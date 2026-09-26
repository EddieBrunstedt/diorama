//! The island: its grid of tiles (data) and their voxel visuals.

mod grid;
mod tile_mesh;
mod trees;

pub use grid::{Island, TileKind};

use crate::juice::Tween;
use crate::rng::Rng;
use crate::voxel::{VOXEL, VoxelMaterial};
use bevy::prelude::*;
use grid::cell_to_world;

/// Seconds before the first tile drops in when the game starts.
const INTRO_DELAY: f32 = 0.3;
/// Extra delay per ring away from the hearth, so the island assembles from the middle outward.
const INTRO_STAGGER: f32 = 0.14;
const TILE_DROP_HEIGHT: f32 = 2.5;
const TREE_POP_DELAY: f32 = 0.35;

const INTRO_SALT: u64 = 4;

pub struct IslandPlugin;

impl Plugin for IslandPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Island::starting())
            .init_resource::<VoxelMaterial>()
            .add_systems(Startup, spawn_island);
    }
}

/// The visual for one grid cell. Trees and structures on the tile are its children.
#[derive(Component, Debug)]
pub struct Tile {
    #[expect(dead_code, reason = "used for targeting from milestone 2 on")]
    pub cell: IVec2,
    pub kind: TileKind,
}

/// A choppable tree standing on a forest tile.
#[derive(Component, Debug)]
pub struct Tree;

fn spawn_island(
    mut commands: Commands,
    island: Res<Island>,
    material: Res<VoxelMaterial>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (cell, kind) in island.tiles() {
        let mut rng = Rng::for_cell(cell, INTRO_SALT);
        let ring = cell.x.abs().max(cell.y.abs()) as f32;
        let delay = INTRO_DELAY + ring * INTRO_STAGGER + rng.range(0.0, 0.08);
        let drop = Tween::drop_in(cell_to_world(cell), TILE_DROP_HEIGHT, delay);
        let tree_delay = drop.remaining() - TREE_POP_DELAY;

        let tile = commands
            .spawn((
                Tile { cell, kind },
                Mesh3d(meshes.add(tile_mesh::tile_mesh(&island, cell).build())),
                MeshMaterial3d(material.0.clone()),
                drop.start_transform(),
                Visibility::Hidden,
                drop,
            ))
            .id();

        if kind == TileKind::Forest {
            for (index, spot) in trees::tree_spots(cell).into_iter().enumerate() {
                let scale = Vec3::splat(spot.scale);
                let pop = Tween::pop_in(scale, tree_delay + rng.range(0.0, 0.25));
                commands.spawn((
                    Tree,
                    Mesh3d(meshes.add(trees::tree_mesh(cell, index, spot.style).build())),
                    MeshMaterial3d(material.0.clone()),
                    Transform {
                        translation: Vec3::new(spot.position.x, 0.0, spot.position.y) * VOXEL,
                        rotation: Quat::from_rotation_y(
                            spot.quarter_turns as f32 * std::f32::consts::FRAC_PI_2,
                        ),
                        ..pop.start_transform()
                    },
                    Visibility::Hidden,
                    pop,
                    ChildOf(tile),
                ));
            }
        }
    }
}
