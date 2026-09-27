//! Showing where the picked tile can go: corner brackets on every free spot it fits, and a ghost of
//! the tile hovering over the spot under the cursor, glowing where it fits and red where it
//! doesn't. The ghost is the real model the tile would get there (road arms, trees and all).

use super::{Placement, TileOffer};
use crate::island::{Island, TileKind, cell_to_world, preview_models};
use crate::juice::{Squash, Wobble};
use crate::palette;
use crate::voxel::{TILE_SIZE, VoxelMesh};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

/// How high the ghost hovers over its spot.
pub const HOVER_HEIGHT: f32 = 0.3;
const BOB_HEIGHT: f32 = 0.03;
const BOB_SPEED: f32 = 3.0;
/// How quickly the ghost glides to a new spot (per second, exponential).
const GLIDE_RATE: f32 = 24.0;
/// A slot's outline: half its size and its thickness, in voxels.
const SLOT_HALF: f32 = 3.9;
const SLOT_THICKNESS: f32 = 0.55;
const SLOT_POP_TIME: f32 = 0.3;
/// Extra delay per slot, so they pop up one after another.
const SLOT_STAGGER: f32 = 0.03;

/// The player clicked a spot where the tile doesn't fit.
#[derive(Message, Clone, Copy, Debug)]
pub struct Rejected;

#[derive(Resource)]
pub struct GhostAssets {
    fits: Handle<StandardMaterial>,
    blocked: Handle<StandardMaterial>,
    slot: Handle<Mesh>,
    slot_material: Handle<StandardMaterial>,
}

impl FromWorld for GhostAssets {
    fn from_world(world: &mut World) -> Self {
        let slot = world
            .resource_mut::<Assets<Mesh>>()
            .add(slot_mesh().build());
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        Self {
            // The tile's own colors, lit up a little so it reads as "not placed yet".
            fits: materials.add(StandardMaterial {
                base_color: Color::WHITE,
                emissive: palette::UI_PAPER.to_linear() * 0.18,
                perceptual_roughness: 0.9,
                reflectance: 0.2,
                ..default()
            }),
            blocked: materials.add(StandardMaterial {
                base_color: palette::BLUEPRINT_PROBLEM,
                emissive: palette::PROBLEM.to_linear() * 0.35,
                perceptual_roughness: 1.0,
                reflectance: 0.0,
                ..default()
            }),
            slot,
            slot_material: materials.add(StandardMaterial {
                base_color: palette::UI_PAPER.with_alpha(0.8),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
        }
    }
}

#[derive(Component)]
pub(super) struct Ghost {
    /// The spot and tile the ghost's models were made for.
    shown: Option<(IVec2, TileKind)>,
    fits: bool,
}

/// One of the ghost's models (the tile, or one of its trees).
#[derive(Component)]
pub(super) struct GhostPart;

/// Brackets around a free spot where the picked tile fits.
#[derive(Component)]
pub(super) struct Slot {
    age: f32,
}

pub(super) fn spawn_ghost(mut commands: Commands) {
    commands.spawn((
        Ghost {
            shown: None,
            fits: true,
        },
        Transform::default(),
        Visibility::Hidden,
        Squash::new(Vec3::ONE),
    ));
}

pub(super) fn move_ghost(
    mut commands: Commands,
    time: Res<Time>,
    island: Res<Island>,
    offer: Res<TileOffer>,
    placement: Res<Placement>,
    assets: Res<GhostAssets>,
    ghost: Single<(
        Entity,
        &mut Ghost,
        &mut Transform,
        &mut Visibility,
        &mut Squash,
        Option<&Wobble>,
    )>,
    mut parts: Query<&mut MeshMaterial3d<StandardMaterial>, With<GhostPart>>,
    mut rejected: MessageReader<Rejected>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let (entity, mut ghost, mut transform, mut visibility, mut squash, wobble) = ghost.into_inner();
    let Some(spot) = placement.hover.filter(|_| offer.placing().is_some()) else {
        if ghost.shown.is_some() {
            commands.entity(entity).despawn_children();
            ghost.shown = None;
        }
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    let fits = spot.fits.is_ok();
    let material = if fits { &assets.fits } else { &assets.blocked };
    let bob = BOB_HEIGHT * (time.elapsed_secs() * BOB_SPEED).sin();
    let goal = cell_to_world(spot.cell) + Vec3::Y * (HOVER_HEIGHT + bob);

    if ghost.shown != Some((spot.cell, spot.kind)) {
        let appearing = ghost.shown.is_none();
        commands.entity(entity).despawn_children();
        for (part, model) in preview_models(&island, spot.cell, spot.kind) {
            commands.spawn((
                GhostPart,
                Mesh3d(meshes.add(model.build())),
                MeshMaterial3d(material.clone()),
                part,
                NotShadowCaster,
                ChildOf(entity),
            ));
        }
        ghost.shown = Some((spot.cell, spot.kind));
        ghost.fits = fits;
        if appearing {
            transform.translation = goal;
            squash.pulse(0.3);
        } else {
            squash.pulse(0.15);
        }
        if !fits {
            commands
                .entity(entity)
                .insert(Wobble::new(wobble, &transform, Vec3::Z, 0.05));
        }
    } else if ghost.fits != fits {
        ghost.fits = fits;
        for mut part in &mut parts {
            part.0 = material.clone();
        }
    }
    visibility.set_if_neq(Visibility::Inherited);
    let glide = 1.0 - (-GLIDE_RATE * time.delta_secs()).exp();
    transform.translation = transform.translation.lerp(goal, glide);

    if rejected.read().count() > 0 {
        squash.pulse(-0.25);
        commands
            .entity(entity)
            .insert(Wobble::new(wobble, &transform, Vec3::Z, 0.14));
    }
}

/// Brackets on every free spot where the picked tile fits, popping up one after another.
pub(super) fn show_slots(
    mut commands: Commands,
    time: Res<Time>,
    island: Res<Island>,
    offer: Res<TileOffer>,
    assets: Res<GhostAssets>,
    mut slots: Query<(&mut Slot, &mut Transform, &mut Visibility)>,
    mut pool: Local<Vec<Entity>>,
    mut cells: Local<Vec<IVec2>>,
    mut shown_for: Local<Option<TileKind>>,
) {
    let placing = offer.placing();
    if placing != *shown_for || island.is_changed() {
        *cells = placing.map_or_else(Vec::new, |kind| super::rules::valid_cells(&island, kind));
        // A new pick pops the brackets up again.
        for &entity in pool.iter() {
            if let Ok((mut slot, _, _)) = slots.get_mut(entity) {
                slot.age = 0.0;
            }
        }
        *shown_for = placing;
    }
    while pool.len() < cells.len() {
        let slot = commands
            .spawn((
                Slot { age: 0.0 },
                Mesh3d(assets.slot.clone()),
                MeshMaterial3d(assets.slot_material.clone()),
                Transform::from_scale(Vec3::splat(0.01)),
                Visibility::Hidden,
                NotShadowCaster,
            ))
            .id();
        pool.push(slot);
    }
    let t = time.elapsed_secs();
    for (k, &entity) in pool.iter().enumerate() {
        let Ok((mut slot, mut transform, mut visibility)) = slots.get_mut(entity) else {
            continue;
        };
        let Some(&cell) = cells.get(k) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        slot.age += time.delta_secs();
        visibility.set_if_neq(Visibility::Inherited);
        let pop = EaseFunction::BackOut
            .sample_clamped((slot.age - k as f32 * SLOT_STAGGER).max(0.0) / SLOT_POP_TIME);
        let breathe = 1.0 + 0.04 * (t * 4.0 + k as f32 * 0.7).sin();
        transform.translation =
            Vec3::new(cell.x as f32, 0.0, cell.y as f32) * TILE_SIZE + Vec3::Y * 0.01;
        transform.scale = Vec3::splat((pop * breathe).max(0.01));
    }
}

/// A square outline marking a free spot, a little inside the tile's edges so neighboring spots
/// stay apart.
fn slot_mesh() -> VoxelMesh {
    let mut mesh = VoxelMesh::default();
    let (outer, inner) = (SLOT_HALF, SLOT_HALF - SLOT_THICKNESS);
    let height = 0.3;
    for (min, max) in [
        (Vec2::new(-outer, -outer), Vec2::new(outer, -inner)),
        (Vec2::new(-outer, inner), Vec2::new(outer, outer)),
        (Vec2::new(-outer, -inner), Vec2::new(-inner, inner)),
        (Vec2::new(inner, -inner), Vec2::new(outer, inner)),
    ] {
        mesh.cuboid(
            Vec3::new(min.x, 0.0, min.y),
            Vec3::new(max.x, height, max.y),
            Color::WHITE,
        );
    }
    mesh
}
