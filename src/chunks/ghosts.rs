//! The chunks on offer, drawn where they'd go: their ground dimmed and cool, a frame around each
//! (cyan for plain ground, red for a chunk with a nest), and the nest itself. They rise into view
//! when the offer opens; the one under the cursor lifts and lights up. Once one is claimed, they
//! all go (the claimed one drops onto the map as real ground, see [`crate::map`]).

use super::templates::Claim;
use super::{ChunkOffer, OfferHover};
use crate::grid::Grid;
use crate::horde::{nest_mesh, nest_seed};
use crate::map::{NEST_HEIGHT, chunk_mesh, mud_mesh};
use crate::palette;
use crate::shapes::BoxMesh;
use crate::tuning::grid::CHUNK_SIZE;
use crate::tuning::juice::{GLOW_BRIGHTNESS, OFFER_HOVER_LIFT, OFFER_PULSE_RATE};
use bevy::prelude::*;

/// The frame around an offered chunk: how wide and tall its bars are.
const FRAME_WIDTH: f32 = 0.2;
const FRAME_HEIGHT: f32 = 0.06;
/// An offered chunk rises into view from this deep.
const RISE_FROM: f32 = -1.2;
/// How quickly an offered chunk eases to its height (per second, exponential).
const LIFT_RATE: f32 = 14.0;

pub struct GhostsPlugin;

impl Plugin for GhostsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GhostMaterials>()
            .add_systems(Update, (show_ghosts, light_ghosts).chain());
    }
}

/// An offered chunk, drawn where it would go.
#[derive(Component)]
struct Ghost(Claim);

/// Part of an offered chunk's ground, which lights up when the cursor is on the chunk.
#[derive(Component)]
struct GhostGround {
    mud: bool,
}

#[derive(Resource)]
struct GhostMaterials {
    ground: Handle<StandardMaterial>,
    ground_hover: Handle<StandardMaterial>,
    mud: Handle<StandardMaterial>,
    mud_hover: Handle<StandardMaterial>,
    frame: Handle<StandardMaterial>,
    frame_nest: Handle<StandardMaterial>,
}

impl FromWorld for GhostMaterials {
    fn from_world(world: &mut World) -> Self {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let mut ground = |tint: Color, gloss: bool| {
            materials.add(StandardMaterial {
                base_color: tint,
                perceptual_roughness: if gloss { 0.35 } else { 0.9 },
                reflectance: if gloss { 0.45 } else { 0.2 },
                ..default()
            })
        };
        let (ground, ground_hover, mud, mud_hover) = (
            ground(palette::OFFER_TINT, false),
            ground(palette::OFFER_TINT_HOVER, false),
            ground(palette::OFFER_TINT, true),
            ground(palette::OFFER_TINT_HOVER, true),
        );
        let mut frame = |color: Color| {
            materials.add(StandardMaterial {
                base_color: color,
                unlit: true,
                ..default()
            })
        };
        Self {
            ground,
            ground_hover,
            mud,
            mud_hover,
            frame: frame(palette::OFFER_FRAME),
            frame_nest: frame(palette::OFFER_FRAME_NEST),
        }
    }
}

/// Keeps a ghost for each offered chunk while one has to be claimed, and none otherwise.
fn show_ghosts(
    mut commands: Commands,
    grid: Res<Grid>,
    offer: Res<ChunkOffer>,
    ghosts: Query<(Entity, &Ghost)>,
    materials: Res<GhostMaterials>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if !offer.is_changed() && !grid.is_changed() {
        return;
    }
    let wanted: Vec<Claim> = match &offer.0 {
        Some(offer) if offer.pending(&grid) => offer.choices.clone(),
        _ => Vec::new(),
    };
    let shown: Vec<Claim> = ghosts.iter().map(|(_, ghost)| ghost.0).collect();
    if shown == wanted {
        return;
    }
    for (entity, _) in &ghosts {
        commands.entity(entity).despawn();
    }
    for claim in wanted {
        spawn_ghost(&mut commands, &grid, claim, &materials, &mut meshes);
    }
}

fn spawn_ghost(
    commands: &mut Commands,
    grid: &Grid,
    claim: Claim,
    materials: &GhostMaterials,
    meshes: &mut Assets<Mesh>,
) {
    // Drawn as it would be on the map, so its edges match the ground next door.
    let mut claimed = grid.clone();
    claim.add_to(&mut claimed);
    let origin = Grid::chunk_origin(claim.slot).as_vec2();
    let ground = meshes.add(chunk_mesh(&claimed, claim.slot));
    let mud = mud_mesh(&claimed, claim.slot).map(|mesh| meshes.add(mesh));
    let frame = meshes.add(frame_mesh());
    let nest = claim.nest();
    let frame_material = if nest.is_some() {
        materials.frame_nest.clone()
    } else {
        materials.frame.clone()
    };
    commands
        .spawn((
            Ghost(claim),
            Transform::from_xyz(origin.x, RISE_FROM, origin.y),
            Visibility::default(),
        ))
        .with_children(|ghost| {
            ghost.spawn((
                GhostGround { mud: false },
                Mesh3d(ground),
                MeshMaterial3d(materials.ground.clone()),
            ));
            if let Some(mud) = mud {
                ghost.spawn((
                    GhostGround { mud: true },
                    Mesh3d(mud),
                    MeshMaterial3d(materials.mud.clone()),
                ));
            }
            ghost.spawn((Mesh3d(frame), MeshMaterial3d(frame_material)));
            if let Some(nest) = nest {
                let at = nest.center().xz() - origin;
                ghost.spawn((
                    GhostGround { mud: false },
                    Mesh3d(meshes.add(nest_mesh(nest.size as f32, nest_seed(nest)))),
                    MeshMaterial3d(materials.ground.clone()),
                    Transform::from_xyz(at.x, NEST_HEIGHT, at.y),
                ));
            }
        });
}

/// Bars along a chunk's edges (chunk-local), just inside it.
fn frame_mesh() -> Mesh {
    let size = CHUNK_SIZE as f32;
    let (w, h) = (FRAME_WIDTH, FRAME_HEIGHT);
    let mut mesh = BoxMesh::default();
    for (min, max) in [
        (Vec2::ZERO, Vec2::new(size, w)),
        (Vec2::new(0.0, size - w), Vec2::splat(size)),
        (Vec2::new(0.0, w), Vec2::new(w, size - w)),
        (Vec2::new(size - w, w), Vec2::new(size, size - w)),
    ] {
        mesh.cuboid(
            Vec3::new(min.x, 0.0, min.y),
            Vec3::new(max.x, h, max.y),
            Color::WHITE,
        );
    }
    mesh.build()
}

/// The ghost under the cursor lifts and lights up; the frames pulse.
fn light_ghosts(
    time: Res<Time<Real>>,
    hover: Res<OfferHover>,
    materials: Res<GhostMaterials>,
    mut assets: ResMut<Assets<StandardMaterial>>,
    mut ghosts: Query<(&Ghost, &mut Transform, &Children)>,
    mut grounds: Query<(&GhostGround, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    if ghosts.is_empty() {
        return;
    }
    let dt = time.delta_secs();
    let pulse = 0.5 + 0.5 * (time.elapsed_secs() * OFFER_PULSE_RATE * std::f32::consts::TAU).sin();
    for (handle, color) in [
        (&materials.frame, palette::OFFER_FRAME),
        (&materials.frame_nest, palette::OFFER_FRAME_NEST),
    ] {
        if let Some(mut material) = assets.get_mut(handle) {
            let glow = 0.6 + (GLOW_BRIGHTNESS - 0.6) * pulse;
            material.base_color = (color.to_linear() * glow).into();
        }
    }
    for (ghost, mut transform, children) in &mut ghosts {
        let hovered = hover.0 == Some(ghost.0);
        let lift = if hovered { OFFER_HOVER_LIFT } else { 0.0 };
        let ease = 1.0 - (-LIFT_RATE * dt).exp();
        transform.translation.y = transform.translation.y.lerp(lift, ease);
        for &child in children {
            let Ok((ground, mut material)) = grounds.get_mut(child) else {
                continue;
            };
            let wanted = match (ground.mud, hovered) {
                (false, false) => &materials.ground,
                (false, true) => &materials.ground_hover,
                (true, false) => &materials.mud,
                (true, true) => &materials.mud_hover,
            };
            if material.0 != *wanted {
                material.0 = wanted.clone();
            }
        }
    }
}
