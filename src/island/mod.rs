//! The island: its grid of tiles (data) and their voxel visuals.

mod grid;
mod tile_mesh;
mod trees;

pub use grid::{Island, TileKind, cell_to_world};

use crate::juice::{Burst, Particles, Tween, Wobble};
use crate::palette;
use crate::phase::GameState;
use crate::rng::Rng;
use crate::voxel::{VOXEL, VoxelMaterial};
use bevy::prelude::*;

/// Seconds before the first tile drops in when the game starts.
const INTRO_DELAY: f32 = 0.3;
/// Extra delay per ring away from the hearth, so the island assembles from the middle outward.
const INTRO_STAGGER: f32 = 0.14;
const TILE_DROP_HEIGHT: f32 = 2.5;
const TREE_POP_DELAY: f32 = 0.35;

const INTRO_SALT: u64 = 4;

/// Seconds a chopped tree takes to topple over.
const FALL_TIME: f32 = 0.55;
/// Seconds it lies on the ground before breaking into chunks.
const LIE_TIME: f32 = 0.3;
const SHRINK_TIME: f32 = 0.2;

pub struct IslandPlugin;

impl Plugin for IslandPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Island::starting())
            .init_resource::<VoxelMaterial>()
            .add_systems(OnEnter(GameState::Intro), spawn_island)
            .add_systems(OnExit(GameState::GameOver), clear_island)
            .add_systems(Update, fell_trees);
    }
}

/// The visual for one grid cell. Trees and structures on the tile are its children.
#[derive(Component, Debug)]
pub struct Tile {
    #[expect(dead_code, reason = "for placing tiles in milestone 5")]
    pub cell: IVec2,
    pub kind: TileKind,
}

/// Identifies a tree: the forest cell it stands on and which of that cell's trees it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TreeId {
    pub cell: IVec2,
    pub index: u8,
}

/// A choppable tree standing on a forest tile.
#[derive(Component, Debug)]
pub struct Tree {
    pub id: TreeId,
    /// World x/z of the trunk.
    pub position: Vec2,
    pub scale: f32,
    /// The tree's upright rotation.
    pub rest: Quat,
}

/// A tree that has been chopped: it topples away from the keeper, then breaks into chunks.
/// Insert it to fell a tree; the tree stops being a [`Tree`] right away.
#[derive(Component, Debug)]
pub struct Felled {
    /// World x/z direction to fall toward.
    pub toward: Vec2,
    rest: Quat,
    scale: f32,
    age: f32,
    landed: bool,
}

impl Felled {
    pub fn new(tree: &Tree, toward: Vec2) -> Self {
        Self {
            toward: toward.normalize_or(Vec2::X),
            rest: tree.rest,
            scale: tree.scale,
            age: 0.0,
            landed: false,
        }
    }
}

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
                let rest =
                    Quat::from_rotation_y(spot.quarter_turns as f32 * std::f32::consts::FRAC_PI_2);
                let local = spot.position * VOXEL;
                commands.spawn((
                    Tree {
                        id: TreeId {
                            cell,
                            index: index as u8,
                        },
                        position: cell_to_world(cell).xz() + local,
                        scale: spot.scale,
                        rest,
                    },
                    Mesh3d(meshes.add(trees::tree_mesh(cell, index, spot.style).build())),
                    MeshMaterial3d(material.0.clone()),
                    Transform {
                        translation: Vec3::new(local.x, 0.0, local.y),
                        rotation: rest,
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

/// Clears the old run's island (trees and all) and goes back to the starting island.
fn clear_island(
    mut commands: Commands,
    tiles: Query<Entity, With<Tile>>,
    mut island: ResMut<Island>,
) {
    for tile in &tiles {
        commands.entity(tile).despawn();
    }
    *island = Island::starting();
}

/// Topples felled trees, leaves a stump, and breaks the trunk into chunks once it has landed.
fn fell_trees(
    mut commands: Commands,
    time: Res<Time>,
    mut felled: Query<(
        Entity,
        &mut Felled,
        &mut Transform,
        &GlobalTransform,
        &ChildOf,
        Has<Wobble>,
    )>,
    material: Res<VoxelMaterial>,
    mut particles: Particles,
) {
    let dt = time.delta_secs();
    for (entity, mut fell, mut transform, global, child_of, wobbling) in &mut felled {
        // A hover or hit wobble can still land in the frame the tree is felled.
        if wobbling {
            commands.entity(entity).remove::<Wobble>();
        }
        if fell.is_added() {
            commands.entity(entity).remove::<(Tree, Tween)>();
            let pop = Tween::pop_in(Vec3::splat(fell.scale), 0.0);
            commands.spawn((
                Mesh3d(particles.meshes().add(trees::stump_mesh().build())),
                MeshMaterial3d(material.0.clone()),
                Transform {
                    translation: transform.translation,
                    rotation: fell.rest,
                    ..pop.start_transform()
                },
                Visibility::Hidden,
                pop,
                ChildOf(child_of.parent()),
            ));
            transform.scale = Vec3::splat(fell.scale);
        }
        fell.age += dt;
        let axis = Vec3::Y.cross(Vec3::new(fell.toward.x, 0.0, fell.toward.y));
        let fall = (fell.age / FALL_TIME).min(1.0);
        // Accelerates like it's really falling, with a small bounce once it hits the ground.
        let bounce = if fell.landed {
            let since = fell.age - FALL_TIME;
            0.12 * (-since * 9.0).exp() * (since * 22.0).sin().abs()
        } else {
            0.0
        };
        let angle = (fall * fall * std::f32::consts::FRAC_PI_2 * 0.96) - bounce;
        transform.rotation = Quat::from_axis_angle(axis, angle) * fell.rest;

        let crown = global.translation()
            + Vec3::new(fell.toward.x, 0.0, fell.toward.y) * 0.4 * fell.scale
            + Vec3::Y * 0.08;
        if fall >= 1.0 && !fell.landed {
            fell.landed = true;
            particles.burst(
                &mut commands,
                crown,
                Burst {
                    colors: &[palette::LEAVES, palette::PINE, palette::FOREST_GRASS],
                    count: 10,
                    size: 0.05,
                    spread: 1.1,
                    lift: 1.6,
                    lifetime: 0.7,
                },
            );
        }
        let shrink = (fell.age - FALL_TIME - LIE_TIME) / SHRINK_TIME;
        if shrink >= 1.0 {
            particles.burst(
                &mut commands,
                crown,
                Burst {
                    colors: &[palette::BARK, palette::LOG_END, palette::LEAVES],
                    count: 14,
                    size: 0.06,
                    spread: 1.3,
                    lift: 2.2,
                    lifetime: 0.9,
                },
            );
            commands.entity(entity).despawn();
        } else if shrink > 0.0 {
            transform.scale = Vec3::splat(fell.scale * (1.0 - shrink * shrink).max(0.01));
        }
    }
}
