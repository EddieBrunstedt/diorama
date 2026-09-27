//! The horde: swarmers and brutes pour out of the nests and follow their flow fields to the
//! Command Center, crowding like a horde instead of stacking up. Where their way leads through a
//! wall, they stop at it and attack it until it breaks. One that reaches the Command Center dies
//! on it and deals damage.
//!
//! The wave loop ([`crate::waves`]) lets them out in packs through [`SpawnQueue`] and
//! [`BruteQueue`], and each wave's enemies are tougher ([`Toughness`]). Once the Command Center
//! has fallen, the horde swarms over its ruin instead of bursting on it.
//!
//! Thousands of swarmers can be on the map at once: they share one mesh and one material so
//! they're drawn in a few batches, move in parallel, and find their neighbors through a spatial
//! hash ([`crowd`], plain data). Towers find their targets through the same hash
//! ([`EnemyIndex`]).
//!
//! Every enemy has [`Health`]. Whatever hurts them runs in [`DamageSystems`], and the ones that
//! ran out burst into gibs and send an [`EnemyKilled`] ([`death`]).

mod brute;
pub mod crowd;
mod death;
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
use crate::waves::{CurrentWave, Phase};
use crate::wear::StructureHit;
use bevy::{ecs::system::SystemParam, prelude::*, utils::Parallel};
pub use brute::{Brute, BruteQueue};
use crowd::{CrowdIndex, push_out, separation, shove, steer, touches, walls_in_the_way};
pub use death::{DeathSystems, EnemyKilled};

/// Longest step the crowd moves at once, so a hitch can't throw swarmers through walls.
const MAX_STEP: f32 = 1.0 / 20.0;

pub struct HordePlugin;

impl Plugin for HordePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpawnQueue>()
            .init_resource::<Toughness>()
            .init_resource::<SwarmerAssets>()
            .init_resource::<ShapeMaterial>()
            .init_resource::<Crowd>()
            .add_message::<SwarmersEmerged>()
            .add_plugins((nest::NestPlugin, brute::BrutePlugin, death::DeathPlugin))
            .configure_sets(Update, DamageSystems.after(HordeSystems))
            .add_systems(Startup, seed_horde)
            .add_systems(OnEnter(Phase::Wave), toughen)
            .add_systems(OnEnter(Phase::GameOver), stop_spawning)
            .add_systems(OnExit(Phase::GameOver), (stop_spawning, clear_horde))
            .add_systems(
                Update,
                (
                    spawn_swarmers,
                    brute::spawn_brutes,
                    // Brutes first: swarmers get out of their way.
                    brute::move_brutes,
                    move_swarmers,
                    // Once it has fallen, the horde swarms over the ruin.
                    (reach_command_center, brute::reach_command_center)
                        .run_if(not(in_state(Phase::GameOver))),
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

/// Hurts the horde (towers, and later abilities). Runs after the horde moves; the dead are
/// cleared away after it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DamageSystems;

/// What an enemy has left. At zero or below it's dead, and cleared away at the end of the damage
/// step.
#[derive(Component, Clone, Copy, Debug)]
pub struct Health(pub f32);

impl Health {
    pub fn is_alive(self) -> bool {
        self.0 > 0.0
    }

    /// Takes `amount` off. True if that's the hit that killed it.
    pub fn hurt(&mut self, amount: f32) -> bool {
        let alive = self.is_alive();
        self.0 -= amount;
        alive && !self.is_alive()
    }
}

/// Where the horde is, for towers finding targets. Positions are from this frame's crowd step:
/// good for finding who's near, but read an enemy's `Transform` for where it is exactly.
#[derive(SystemParam)]
pub struct EnemyIndex<'w> {
    swarmers: Res<'w, Crowd>,
    brutes: Res<'w, brute::BruteCrowd>,
}

impl EnemyIndex<'_> {
    /// Every enemy in the buckets within `radius` of `at` (a superset of those that close), with
    /// its class.
    pub fn near(&self, at: Vec2, radius: f32) -> impl Iterator<Item = (Vec2, Entity, Class)> {
        let swarmers = self
            .swarmers
            .index
            .near(at, radius)
            .map(|&(point, entity)| (point, entity, Class::Swarmer));
        // Brutes are big: reach for their middles as far as their edge.
        let brutes = self
            .brutes
            .index
            .near(at, radius + BRUTE_RADIUS)
            .map(|&(point, entity)| (point, entity, Class::Brute));
        swarmers.chain(brutes)
    }
}

/// How much tougher than in wave 1 the enemies coming out now are: their HP is multiplied by it.
#[derive(Resource, Clone, Copy, Debug)]
pub struct Toughness(pub f32);

impl Default for Toughness {
    fn default() -> Self {
        Self(1.0)
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

impl SpawnQueue {
    /// Lets `count` more swarmers out, pouring out of the nests over `seconds` (along with any
    /// still waiting).
    pub fn release(&mut self, count: u32, seconds: f32) {
        self.waiting += count;
        self.rate = self.waiting as f32 / seconds.max(0.01);
    }

    /// Nothing more comes out.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
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

impl Swarmer {
    /// How fast and which way it's going, in cells per second.
    pub fn velocity(&self) -> Vec2 {
        self.velocity
    }
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
pub(crate) struct Crowd {
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

fn toughen(wave: Res<CurrentWave>, mut toughness: ResMut<Toughness>) {
    toughness.0 = wave.0.toughness;
}

fn stop_spawning(mut swarmers: ResMut<SpawnQueue>, mut brutes: ResMut<BruteQueue>) {
    swarmers.clear();
    brutes.clear();
}

/// A new run: the horde left from the last one is gone.
fn clear_horde(mut commands: Commands, enemies: Query<Entity, With<Health>>) {
    for entity in &enemies {
        commands.entity(entity).despawn();
    }
}

fn spawn_swarmers(
    mut commands: Commands,
    time: Res<Time>,
    landmarks: Res<Landmarks>,
    assets: Res<SwarmerAssets>,
    material: Res<ShapeMaterial>,
    toughness: Res<Toughness>,
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
                Health(SWARMER_HP * toughness.0),
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
    mut bites: Local<Parallel<Vec<StructureHit>>>,
    mut hits: MessageWriter<StructureHit>,
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
                bites.borrow_local_mut().push(StructureHit {
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
