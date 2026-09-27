//! The horde: swarmers and brutes pour out of the nests and follow their flow fields to the
//! Command Center, crowding like a horde instead of stacking up. Where their way leads through a
//! wall, they stop at it and attack it until it breaks. One that reaches the Command Center dies
//! on it and deals damage.
//!
//! Space releases a batch of swarmers and B a few brutes ([`brute`]), until milestone 5 brings
//! waves. Thousands of swarmers can be on the map at once: they share one mesh and one material
//! so they're drawn in a few batches, move in parallel, and find their neighbors through a
//! spatial hash ([`crowd`], plain data).

mod brute;
pub mod crowd;
mod nest;

use crate::command_center::CommandCenterHit;
use crate::flow::{Class, FlowFields, FlowSystems};
use crate::map::Landmarks;
use crate::palette;
use crate::rng::{Rng, RunSeed};
use crate::shapes::{BoxMesh, ShapeMaterial, shade};
use crate::tuning::horde::*;
use crate::tuning::juice::{
    BITE_LUNGE, BITE_SECONDS, EMERGE_DEPTH, EMERGE_SECONDS, SCUTTLE_HOP, SCUTTLE_STRIDE,
};
use crate::walls::WallHit;
use bevy::{prelude::*, utils::Parallel};
pub use brute::{Brute, BruteBatch, BruteQueue};
use crowd::{CrowdIndex, push_out, separation, shove, steer, touches, walls_in_the_way};

/// Longest step the crowd moves at once, so a hitch can't throw swarmers through walls.
const MAX_STEP: f32 = 1.0 / 20.0;

pub struct HordePlugin;

impl Plugin for HordePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BatchSize>()
            .init_resource::<SpawnQueue>()
            .init_resource::<SwarmerAssets>()
            .init_resource::<ShapeMaterial>()
            .init_resource::<Crowd>()
            .add_message::<SwarmersEmerged>()
            .add_plugins((nest::NestPlugin, brute::BrutePlugin))
            .add_systems(Startup, seed_horde)
            .add_systems(
                Update,
                (
                    release_batch,
                    spawn_swarmers,
                    brute::release_brutes,
                    brute::spawn_brutes,
                    // Brutes first: swarmers get out of their way.
                    brute::move_brutes,
                    move_swarmers,
                    reach_command_center,
                    brute::reach_command_center,
                )
                    .chain()
                    .in_set(HordeSystems)
                    .after(FlowSystems),
            );
    }
}

/// Moves the horde. Whatever reacts to swarmers runs after it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HordeSystems;

/// How many swarmers Space releases (`DIORAMA_BATCH` overrides it, see [`crate::dev`]).
#[derive(Resource, Clone, Copy, Debug)]
pub struct BatchSize(pub u32);

impl Default for BatchSize {
    fn default() -> Self {
        Self(BATCH)
    }
}

/// Swarmers released but not out of a nest yet.
#[derive(Resource, Default, Debug)]
pub struct SpawnQueue {
    pub waiting: u32,
    /// Swarmers per second while the queue empties.
    rate: f32,
    /// Fractions of a swarmer owed from earlier frames.
    owed: f32,
}

/// Swarmers came out of a nest this frame.
#[derive(Message, Clone, Copy, Debug)]
pub struct SwarmersEmerged {
    pub nest: usize,
}

#[derive(Component, Clone, Debug)]
pub struct Swarmer {
    velocity: Vec2,
    /// Top speed: every swarmer is a little different.
    speed: f32,
    /// Which way to go when another swarmer sits exactly on top of this one.
    fallback: Vec2,
    /// Walk cycle, advanced by the distance walked.
    stride: f32,
    /// Seconds since it came out of the nest.
    age: f32,
    /// Seconds until it can bite again, and what's left of its bite's lunge.
    bite_cooldown: f32,
    lunge: f32,
}

#[derive(Resource)]
struct HordeRng(Rng);

fn seed_horde(mut commands: Commands, seed: Res<RunSeed>) {
    commands.insert_resource(HordeRng(Rng::new(seed.0 ^ 0x5A4D)));
}

#[derive(Resource)]
struct SwarmerAssets {
    mesh: Handle<Mesh>,
}

impl FromWorld for SwarmerAssets {
    fn from_world(world: &mut World) -> Self {
        let mesh = world.resource_mut::<Assets<Mesh>>().add(swarmer_mesh());
        Self { mesh }
    }
}

/// A little six-legged bug, facing -Z, about a cell's half long and wide.
fn swarmer_mesh() -> Mesh {
    let mut mesh = BoxMesh::default();
    let (light, dark) = (palette::CHITIN, palette::CHITIN_DARK);
    let mut part = |min: [f32; 3], max: [f32; 3], top: Color, bottom: Color| {
        mesh.cuboid_graded(Vec3::from(min), Vec3::from(max), top, bottom);
    };
    // Abdomen, thorax, head.
    part([-0.12, 0.05, 0.0], [0.12, 0.19, 0.22], light, dark);
    part(
        [-0.09, 0.06, -0.12],
        [0.09, 0.16, 0.02],
        shade(light, 0.85),
        dark,
    );
    part(
        [-0.07, 0.05, -0.22],
        [0.07, 0.13, -0.11],
        shade(light, 0.7),
        dark,
    );
    // Eyes and mandibles.
    part(
        [-0.06, 0.1, -0.23],
        [-0.02, 0.14, -0.19],
        palette::ACID,
        palette::ACID,
    );
    part(
        [0.02, 0.1, -0.23],
        [0.06, 0.14, -0.19],
        palette::ACID,
        palette::ACID,
    );
    part([-0.06, 0.05, -0.28], [-0.035, 0.08, -0.2], dark, dark);
    part([0.035, 0.05, -0.28], [0.06, 0.08, -0.2], dark, dark);
    // Legs, sticking out from the thorax so the silhouette reads as a bug from above.
    for z in [-0.1, -0.04, 0.02] {
        part([-0.21, 0.0, z], [-0.08, 0.07, z + 0.025], dark, dark);
        part([0.08, 0.0, z], [0.21, 0.07, z + 0.025], dark, dark);
    }
    mesh.build()
}

/// The spatial hash, kept between frames so its memory is reused.
#[derive(Resource)]
struct Crowd {
    index: CrowdIndex<Entity>,
    positions: Vec<(Vec2, Entity)>,
}

impl Default for Crowd {
    fn default() -> Self {
        Self {
            index: CrowdIndex::new(Rect::default(), 1.0),
            positions: Vec::new(),
        }
    }
}

fn release_batch(
    keys: Res<ButtonInput<KeyCode>>,
    batch: Res<BatchSize>,
    landmarks: Res<Landmarks>,
    mut queue: ResMut<SpawnQueue>,
) {
    if !keys.just_pressed(KeyCode::Space) || landmarks.nests.is_empty() {
        return;
    }
    queue.waiting += batch.0;
    let steady = SPAWN_RATE * landmarks.nests.len() as f32;
    queue.rate = steady.max(queue.waiting as f32 / BATCH_MAX_SECONDS);
}

fn spawn_swarmers(
    mut commands: Commands,
    time: Res<Time>,
    landmarks: Res<Landmarks>,
    assets: Res<SwarmerAssets>,
    material: Res<ShapeMaterial>,
    mut queue: ResMut<SpawnQueue>,
    mut rng: ResMut<HordeRng>,
    mut emerged: MessageWriter<SwarmersEmerged>,
) {
    if queue.waiting == 0 {
        queue.owed = 0.0;
        return;
    }
    queue.owed += queue.rate * time.delta_secs();
    let count = (queue.owed as u32).min(queue.waiting);
    queue.owed -= count as f32;
    queue.waiting -= count;
    let rng = &mut rng.0;
    let mut from = vec![false; landmarks.nests.len()];
    let batch: Vec<_> = (0..count)
        .map(|_| {
            let nest = rng.index(landmarks.nests.len());
            from[nest] = true;
            let area = landmarks.nests[nest].area();
            let inset = SWARMER_RADIUS;
            let at = Vec2::new(
                rng.range(area.min.x + inset, area.max.x - inset),
                rng.range(area.min.y + inset, area.max.y - inset),
            );
            let fallback = Vec2::from_angle(rng.range(0.0, std::f32::consts::TAU));
            (
                Swarmer {
                    velocity: Vec2::ZERO,
                    speed: SWARMER_SPEED * (1.0 + rng.range(-SPEED_VARIATION, SPEED_VARIATION)),
                    fallback,
                    stride: rng.range(0.0, std::f32::consts::TAU),
                    age: 0.0,
                    // So a pile doesn't bite in lockstep.
                    bite_cooldown: rng.range(0.0, SWARMER_BITE_INTERVAL),
                    lunge: 0.0,
                },
                Mesh3d(assets.mesh.clone()),
                MeshMaterial3d(material.0.clone()),
                Transform::from_xyz(at.x, -EMERGE_DEPTH, at.y)
                    .with_rotation(facing(fallback))
                    .with_scale(Vec3::ZERO),
            )
        })
        .collect();
    commands.spawn_batch(batch);
    for (nest, &any) in from.iter().enumerate() {
        if any {
            emerged.write(SwarmersEmerged { nest });
        }
    }
}

/// The rotation that faces a model looking down -Z toward `direction` on the ground.
fn facing(direction: Vec2) -> Quat {
    Quat::from_rotation_y((-direction.x).atan2(-direction.y))
}

fn move_swarmers(
    time: Res<Time>,
    fields: Res<FlowFields>,
    brutes: Res<brute::BruteCrowd>,
    mut crowd: ResMut<Crowd>,
    mut swarmers: Query<(Entity, &mut Swarmer, &mut Transform)>,
    mut bites: Local<Parallel<Vec<WallHit>>>,
    mut hits: MessageWriter<WallHit>,
) {
    let dt = time.delta_secs().min(MAX_STEP);
    if dt <= 0.0 {
        return;
    }
    // Everyone moves against where the others were at the start of the frame, so the order
    // swarmers are moved in doesn't matter.
    let spacing = SWARMER_RADIUS * 2.0;
    let field = &fields.swarmer;
    let crowd = &mut *crowd;
    if !crowd.index.covers(field.area(), spacing) {
        crowd.index = CrowdIndex::new(field.area(), spacing);
    }
    crowd.positions.clear();
    crowd.positions.extend(
        swarmers
            .iter()
            .map(|(entity, _, transform)| (transform.translation.xz(), entity)),
    );
    crowd.index.rebuild(&crowd.positions);
    let index = &crowd.index;
    let brute_spacing = SWARMER_RADIUS + BRUTE_RADIUS;
    let alignment = ATTACK_ANGLE.to_radians().cos();

    swarmers
        .par_iter_mut()
        .for_each(|(entity, mut swarmer, mut transform)| {
            let at = transform.translation.xz();
            let heading = field.direction_at(at);
            let desired = heading * swarmer.speed;
            let velocity = steer(swarmer.velocity, desired, STEERING_RATE, dt);
            let mut push = separation(
                index,
                at,
                entity,
                spacing,
                SEPARATION_STIFFNESS,
                MAX_NEIGHBORS,
                swarmer.fallback,
            );
            // Brutes don't budge for swarmers: they shove them out of the way.
            for &(brute, _) in brutes.index.near(at, brute_spacing) {
                push += shove(at, brute, brute_spacing, swarmer.fallback);
            }
            // However hard the crowd shoves, a swarmer moves less than half a cell a frame, so
            // `push_out` always sends it back out the side it came in through.
            let push = push.clamp_length_max(SWARMER_RADIUS);
            let mut next = push_out(at + velocity * dt + push, SWARMER_RADIUS, |cell| {
                fields.cell_is_clear(cell)
            });
            // Never end up inside a wall or off the map, whatever happens.
            let here = next.floor().as_ivec2();
            if !fields.cell_is_clear(here) {
                next = at;
            }

            // Bite the wall in the way, if its way leads through one.
            let here = next.floor().as_ivec2();
            let through = |cell| {
                fields.cell_is_breakable(cell) && field.distance(cell) < field.distance(here)
            };
            let wall = walls_in_the_way(
                next,
                SWARMER_RADIUS,
                ATTACK_REACH,
                heading,
                alignment,
                through,
            )
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(cell, _)| cell);
            swarmer.bite_cooldown = (swarmer.bite_cooldown - dt).max(0.0);
            swarmer.lunge = (swarmer.lunge - dt).max(0.0);
            if let Some(cell) = wall
                && swarmer.bite_cooldown <= 0.0
            {
                swarmer.bite_cooldown = SWARMER_BITE_INTERVAL;
                swarmer.lunge = BITE_SECONDS;
                bites.borrow_local_mut().push(WallHit {
                    cell,
                    damage: SWARMER_BITE,
                    by: Class::Swarmer,
                    from: next,
                });
            }

            // Scuttle: a quick hop per step, and rise out of the nest when new.
            let swarmer = &mut *swarmer;
            swarmer.velocity = velocity;
            swarmer.stride += next.distance(at) * SCUTTLE_STRIDE;
            swarmer.age += dt;
            let emerged = (swarmer.age / EMERGE_SECONDS).min(1.0);
            let hop = swarmer.stride.sin().abs() * SCUTTLE_HOP;
            transform.translation = Vec3::new(next.x, hop - EMERGE_DEPTH * (1.0 - emerged), next.y);
            transform.scale = Vec3::splat(emerged);
            let look = match wall {
                Some(cell) => Some(facing(cell.as_vec2() + 0.5 - next)),
                None => (velocity.length_squared() > 0.01).then(|| facing(velocity)),
            };
            if let Some(look) = look {
                // A bite dips the head in and back out.
                let dip = (swarmer.lunge / BITE_SECONDS * std::f32::consts::PI).sin();
                transform.rotation = look * Quat::from_rotation_x(-BITE_LUNGE * dip);
            }
        });
    hits.write_batch(bites.drain());
}

fn reach_command_center(
    mut commands: Commands,
    landmarks: Res<Landmarks>,
    swarmers: Query<(Entity, &Transform), With<Swarmer>>,
    mut hits: MessageWriter<CommandCenterHit>,
) {
    let area = landmarks.command_center.area();
    for (entity, transform) in &swarmers {
        let at = transform.translation.xz();
        if touches(at, SWARMER_RADIUS, area) {
            commands.entity(entity).despawn();
            hits.write(CommandCenterHit {
                at: transform.translation,
                damage: SWARMER_DAMAGE,
            });
        }
    }
}
