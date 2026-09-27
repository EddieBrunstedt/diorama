//! The island: its grid of tiles (data) and their voxel visuals.

mod grid;
mod tile_mesh;
mod trees;

pub use grid::{Island, Side, TileKind, cell_to_world};
pub use tile_mesh::TileLook;

use crate::juice::{Burst, Particles, Shake, Tween, TweenTarget, Wobble};
use crate::palette;
use crate::phase::GameState;
use crate::rng::Rng;
use crate::voxel::VoxelMesh;
use crate::voxel::{VOXEL, VoxelMaterial};
use bevy::prelude::*;

/// Seconds before the first tile drops in when the game starts.
const INTRO_DELAY: f32 = 0.3;
/// Extra delay per ring away from the hearth, so the island assembles from the middle outward.
const INTRO_STAGGER: f32 = 0.14;
const TILE_DROP_HEIGHT: f32 = 2.5;
const TREE_POP_DELAY: f32 = 0.35;
/// A placed tile drops from this high.
const PLACE_DROP_HEIGHT: f32 = 1.3;
/// When a dropping tile first hits (a bounce-out ease lands at 1/2.75 of the way through).
const DROP_IMPACT: f32 = 0.75 / 2.75;
/// Neighbors knocked down by a tile landing next to them, and how long they take to spring back.
const NEIGHBOR_DIP: f32 = 0.07;
const NEIGHBOR_SETTLE_TIME: f32 = 0.5;
const LANDING_SHAKE: f32 = 0.4;

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
            .add_message::<PlaceTile>()
            .add_systems(OnEnter(GameState::Intro), spawn_island)
            .add_systems(OnExit(GameState::GameOver), clear_island)
            .add_systems(Update, (place_tiles, land_tiles, refresh_tiles).chain())
            .add_systems(Update, fell_trees);
    }
}

/// The visual for one grid cell. Trees and structures on the tile are its children.
#[derive(Component, Debug)]
pub struct Tile {
    pub cell: IVec2,
    pub kind: TileKind,
    /// What the model was built for; it's rebuilt when this changes.
    look: TileLook,
}

/// Asks to add a tile to the island. It drops in with a bounce.
#[derive(Message, Clone, Copy, Debug)]
pub struct PlaceTile {
    pub cell: IVec2,
    pub kind: TileKind,
}

/// A placed tile on its way down. Its neighbors adapt to it once it has landed.
#[derive(Component, Debug)]
struct Landing {
    left: f32,
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
    for (cell, _) in island.tiles() {
        let mut rng = Rng::for_cell(cell, INTRO_SALT);
        let ring = cell.x.abs().max(cell.y.abs()) as f32;
        let delay = INTRO_DELAY + ring * INTRO_STAGGER + rng.range(0.0, 0.08);
        let drop = Tween::drop_in(cell_to_world(cell), TILE_DROP_HEIGHT, delay);
        spawn_tile(&mut commands, &island, cell, drop, &material, &mut meshes);
    }
}

/// Spawns the tile on `cell` (already on the island) dropping in, with its trees popping up once
/// it has landed.
fn spawn_tile(
    commands: &mut Commands,
    island: &Island,
    cell: IVec2,
    drop: Tween,
    material: &VoxelMaterial,
    meshes: &mut Assets<Mesh>,
) -> Entity {
    let look = TileLook::of(island, cell);
    let kind = look.kind;
    let mut rng = Rng::for_cell(cell, INTRO_SALT);
    let tree_delay = drop.remaining() - TREE_POP_DELAY;
    let tile = commands
        .spawn((
            Mesh3d(meshes.add(tile_mesh::tile_mesh(cell, &look).build())),
            Tile { cell, kind, look },
            MeshMaterial3d(material.0.clone()),
            drop.start_transform(),
            Visibility::Hidden,
            drop,
        ))
        .id();

    if kind == TileKind::Forest {
        for (index, spot) in trees::tree_spots(cell).into_iter().enumerate() {
            let pop = Tween::pop_in(Vec3::splat(spot.scale), tree_delay + rng.range(0.0, 0.25));
            let (transform, model) = tree_model(cell, index, &spot);
            commands.spawn((
                Tree {
                    id: TreeId {
                        cell,
                        index: index as u8,
                    },
                    position: cell_to_world(cell).xz() + transform.translation.xz(),
                    scale: spot.scale,
                    rest: transform.rotation,
                },
                Mesh3d(meshes.add(model.build())),
                MeshMaterial3d(material.0.clone()),
                Transform {
                    scale: pop.start_transform().scale,
                    ..transform
                },
                Visibility::Hidden,
                pop,
                ChildOf(tile),
            ));
        }
    }
    tile
}

/// A tree's model and where it stands on its tile.
fn tree_model(cell: IVec2, index: usize, spot: &trees::TreeSpot) -> (Transform, VoxelMesh) {
    let rest = Quat::from_rotation_y(spot.quarter_turns as f32 * std::f32::consts::FRAC_PI_2);
    let local = spot.position * VOXEL;
    let transform = Transform {
        translation: Vec3::new(local.x, 0.0, local.y),
        rotation: rest,
        scale: Vec3::splat(spot.scale),
    };
    (transform, trees::tree_mesh(cell, index, spot.style))
}

/// The models a tile of `kind` on the empty `cell` would have, for previewing it: the tile
/// itself, then its trees, each with its transform relative to the tile.
pub fn preview_models(island: &Island, cell: IVec2, kind: TileKind) -> Vec<(Transform, VoxelMesh)> {
    let look = TileLook::preview(island, cell, kind);
    let mut models = vec![(Transform::IDENTITY, tile_mesh::tile_mesh(cell, &look))];
    if kind == TileKind::Forest {
        models.extend(
            trees::tree_spots(cell)
                .iter()
                .enumerate()
                .map(|(index, spot)| tree_model(cell, index, spot)),
        );
    }
    models
}

/// Adds placed tiles to the island. They drop in; see [`land_tiles`].
fn place_tiles(
    mut commands: Commands,
    mut placed: MessageReader<PlaceTile>,
    mut island: ResMut<Island>,
    material: Res<VoxelMaterial>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for &PlaceTile { cell, kind } in placed.read() {
        island.place(cell, kind);
        let drop = Tween::drop_in(cell_to_world(cell), PLACE_DROP_HEIGHT, 0.0);
        let tile = spawn_tile(&mut commands, &island, cell, drop, &material, &mut meshes);
        commands.entity(tile).insert(Landing { left: DROP_IMPACT });
    }
}

/// A placed tile hits the island: dust flies, the camera shakes, and its neighbors get knocked
/// down a little and spring back.
fn land_tiles(
    mut commands: Commands,
    time: Res<Time>,
    mut landing: Query<(Entity, &Tile, &mut Landing)>,
    tiles: Query<(Entity, &Tile, &Transform), Without<Tween>>,
    mut shake: ResMut<Shake>,
    mut particles: Particles,
) {
    for (entity, tile, mut landing) in &mut landing {
        landing.left -= time.delta_secs();
        if landing.left > 0.0 {
            continue;
        }
        commands.entity(entity).remove::<Landing>();
        shake.add(LANDING_SHAKE);
        let at = cell_to_world(tile.cell);
        let grass = match tile.kind {
            TileKind::Forest => palette::FOREST_GRASS,
            TileKind::Road => palette::ROAD,
            _ => palette::MEADOW_GRASS,
        };
        for side in Side::ALL {
            let edge = at + Vec3::new(side.offset().x as f32, 0.0, side.offset().y as f32) * 0.5;
            particles.burst(
                &mut commands,
                edge + Vec3::Y * 0.05,
                Burst {
                    colors: &[palette::CLOUD, palette::CLOUD_SHADE, palette::DIRT, grass],
                    count: 6,
                    size: 0.06,
                    spread: 1.3,
                    lift: 1.3,
                    lifetime: 0.7,
                },
            );
        }
        for (neighbor, other, transform) in &tiles {
            if Side::between(tile.cell, other.cell).is_none() {
                continue;
            }
            let rest = cell_to_world(other.cell);
            let from = transform.translation - Vec3::Y * NEIGHBOR_DIP;
            commands.entity(neighbor).insert(Tween::new(
                TweenTarget::Translation { from, to: rest },
                EaseFunction::ElasticOut,
                0.0,
                NEIGHBOR_SETTLE_TIME,
            ));
        }
    }
}

/// Rebuilds the models of tiles whose surroundings changed (the rock deepens toward the new
/// middle, grass stops dripping over sides that are no longer the edge, the road's end turns
/// toward its new tile). Waits until placed tiles have landed.
fn refresh_tiles(
    island: Res<Island>,
    landing: Query<(), With<Landing>>,
    mut landed: RemovedComponents<Landing>,
    mut tiles: Query<(&mut Tile, &mut Mesh3d)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let just_landed = landed.read().count() > 0;
    if !(island.is_changed() || just_landed) || !landing.is_empty() {
        return;
    }
    for (mut tile, mut mesh) in &mut tiles {
        let look = TileLook::of(&island, tile.cell);
        if look != tile.look {
            mesh.0 = meshes.add(tile_mesh::tile_mesh(tile.cell, &look).build());
            tile.look = look;
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
