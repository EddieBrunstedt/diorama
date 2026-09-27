//! Brutes: big, slow and armored, too wide for a 1-cell gap. They follow their own flow field
//! (see [`crate::flow`]), and where swarmers would squeeze through a gap, a brute smashes the wall
//! beside it. Each smash hits every wall it's pressing against, up to the two it needs out of the
//! way. They shove swarmers aside, and swarmers don't push back.
//!
//! They come out of the nests one at a time, leading the packs of a wave.

use super::crowd::{CrowdIndex, push_out, separation, steer, touches, walls_in_the_way};
use super::{Health, HordeRng, MAX_STEP, SwarmersEmerged, Toughness, facing};
use crate::command_center::CommandCenterHit;
use crate::flow::{Class, FlowFields};
use crate::juice::{Shake, Squash};
use crate::map::Landmarks;
use crate::palette;
use crate::shapes::{BoxMesh, ShapeMaterial, shade};
use crate::tuning::horde::*;
use crate::tuning::juice::{
    BRUTE_ROCK, BRUTE_STRIDE, EMERGE_DEPTH, EMERGE_SECONDS, SMASH_DIP, SMASH_REAR,
    SMASH_REAR_SECONDS, SMASH_SECONDS, SMASH_SHAKE, SMASH_SHAKE_MAX, SMASH_SQUASH,
};
use crate::wear::StructureHit;
use bevy::prelude::*;

/// How much a brute's speed varies from one to the next (a fraction).
const SPEED_VARIATION: f32 = 0.05;

pub struct BrutePlugin;

impl Plugin for BrutePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BruteQueue>()
            .init_resource::<BruteAssets>()
            .init_resource::<BruteCrowd>();
    }
}

/// Brutes released but not out of a nest yet.
#[derive(Resource, Default, Debug)]
pub struct BruteQueue {
    pub waiting: u32,
    /// Seconds until the next one comes out.
    next: f32,
}

impl BruteQueue {
    /// Lets `count` more brutes out, one every [`BRUTE_SPAWN_INTERVAL`].
    pub fn release(&mut self, count: u32) {
        self.waiting += count;
    }

    /// Nothing more comes out.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[derive(Component, Clone, Debug)]
pub struct Brute {
    velocity: Vec2,
    speed: f32,
    /// Which way to go when another brute sits exactly on top of this one.
    fallback: Vec2,
    /// Walk cycle, advanced by the distance walked.
    stride: f32,
    /// Seconds since it came out of the nest.
    age: f32,
    /// Seconds until it can smash again, and what's left of its last smash's slam.
    smash_cooldown: f32,
    slam: f32,
    /// Which way it faces on the ground.
    heading: Vec2,
}

impl Brute {
    /// How fast and which way it's going, in cells per second.
    pub fn velocity(&self) -> Vec2 {
        self.velocity
    }
}

/// Where the brutes are, for pushing each other apart and for swarmers to get out of their way.
#[derive(Resource)]
pub(crate) struct BruteCrowd {
    pub(super) index: CrowdIndex<Entity>,
    positions: Vec<(Vec2, Entity)>,
}

impl Default for BruteCrowd {
    fn default() -> Self {
        Self {
            index: CrowdIndex::new(Rect::default(), 1.0),
            positions: Vec::new(),
        }
    }
}

impl BruteCrowd {
    fn rebuild(&mut self, area: Rect, brutes: impl Iterator<Item = (Vec2, Entity)>) {
        let bucket = BRUTE_RADIUS * 2.0;
        if !self.index.covers(area, bucket) {
            self.index = CrowdIndex::new(area, bucket);
        }
        self.positions.clear();
        self.positions.extend(brutes);
        self.index.rebuild(&self.positions);
    }
}

#[derive(Resource)]
pub(super) struct BruteAssets {
    mesh: Handle<Mesh>,
}

impl FromWorld for BruteAssets {
    fn from_world(world: &mut World) -> Self {
        let mesh = world.resource_mut::<Assets<Mesh>>().add(brute_mesh());
        Self { mesh }
    }
}

/// A hulking armored beetle facing -Z: a low head with two bone scythes, a humped back of
/// overlapping plates, and six thick legs. About 1.4 cells wide.
fn brute_mesh() -> Mesh {
    let mut mesh = BoxMesh::default();
    let (shell, plate, bone) = (palette::BRUTE_SHELL, palette::BRUTE_PLATE, palette::BONE);
    let dark = palette::CHITIN_DARK;
    let mut part = |min: [f32; 3], max: [f32; 3], top: Color, bottom: Color| {
        mesh.cuboid_graded(Vec3::from(min), Vec3::from(max), top, bottom);
    };
    // Legs, thick and splayed.
    for z in [-0.45, 0.0, 0.42] {
        part([-0.74, 0.0, z - 0.07], [-0.4, 0.34, z + 0.07], shell, dark);
        part([0.4, 0.0, z - 0.07], [0.74, 0.34, z + 0.07], shell, dark);
    }
    // Body, then back plates rising toward the shoulders.
    part([-0.5, 0.14, -0.6], [0.5, 0.55, 0.72], shell, dark);
    part([-0.55, 0.45, 0.22], [0.55, 0.7, 0.74], plate, shell);
    part(
        [-0.6, 0.5, -0.22],
        [0.6, 0.8, 0.3],
        shade(plate, 1.1),
        shell,
    );
    part(
        [-0.58, 0.55, -0.62],
        [0.58, 0.9, -0.16],
        shade(plate, 1.2),
        shell,
    );
    // Bony ridge spikes along the back.
    for (z, top) in [(-0.4, 0.9), (0.03, 0.8), (0.46, 0.7)] {
        part(
            [-0.06, top, z - 0.07],
            [0.06, top + 0.14, z + 0.07],
            bone,
            bone,
        );
    }
    // Head, low in front, with glowing eyes.
    part(
        [-0.3, 0.14, -0.96],
        [0.3, 0.5, -0.56],
        shade(shell, 0.85),
        dark,
    );
    for x in [-0.2, 0.1] {
        part(
            [x, 0.34, -0.98],
            [x + 0.1, 0.42, -0.94],
            palette::ACID,
            palette::ACID,
        );
    }
    // Two scythes of bone sweeping forward from the shoulders.
    for side in [-1.0, 1.0] {
        let x = |a: f32, b: f32| if side < 0.0 { [-b, -a] } else { [a, b] };
        let [low, high] = x(0.34, 0.5);
        part(
            [low, 0.42, -0.78],
            [high, 0.62, -0.5],
            bone,
            shade(bone, 0.7),
        );
        let [low, high] = x(0.36, 0.46);
        part(
            [low, 0.26, -1.14],
            [high, 0.44, -0.76],
            bone,
            shade(bone, 0.7),
        );
    }
    mesh.build()
}

pub(super) fn spawn_brutes(
    mut commands: Commands,
    time: Res<Time>,
    landmarks: Res<Landmarks>,
    assets: Res<BruteAssets>,
    material: Res<ShapeMaterial>,
    toughness: Res<Toughness>,
    mut queue: ResMut<BruteQueue>,
    mut rng: ResMut<HordeRng>,
    mut emerged: MessageWriter<SwarmersEmerged>,
) {
    if queue.waiting == 0 {
        queue.next = 0.0;
        return;
    }
    queue.next -= time.delta_secs();
    if queue.next > 0.0 {
        return;
    }
    queue.next = BRUTE_SPAWN_INTERVAL;
    queue.waiting -= 1;
    let rng = &mut rng.0;
    let nest = rng.index(landmarks.nests.len());
    let at =
        landmarks.nests[nest].center().xz() + Vec2::new(rng.range(-0.2, 0.2), rng.range(-0.2, 0.2));
    let fallback = Vec2::from_angle(rng.range(0.0, std::f32::consts::TAU));
    commands.spawn((
        Brute {
            velocity: Vec2::ZERO,
            speed: BRUTE_SPEED * (1.0 + rng.range(-SPEED_VARIATION, SPEED_VARIATION)),
            fallback,
            stride: 0.0,
            age: 0.0,
            smash_cooldown: 0.0,
            slam: 0.0,
            heading: fallback,
        },
        Health(BRUTE_HP * toughness.0),
        Mesh3d(assets.mesh.clone()),
        MeshMaterial3d(material.0.clone()),
        Transform::from_xyz(at.x, -EMERGE_DEPTH, at.y)
            .with_rotation(facing(fallback))
            .with_scale(Vec3::ZERO),
        Squash::new(Vec3::ZERO),
    ));
    emerged.write(SwarmersEmerged { nest });
}

pub(super) fn move_brutes(
    time: Res<Time>,
    fields: Res<FlowFields>,
    mut crowd: ResMut<BruteCrowd>,
    mut brutes: Query<(Entity, &mut Brute, &mut Transform, &mut Squash)>,
    mut hits: MessageWriter<StructureHit>,
    mut shake: ResMut<Shake>,
) {
    let dt = time.delta_secs().min(MAX_STEP);
    if dt <= 0.0 {
        return;
    }
    let field = &fields.brute;
    let area = fields.swarmer.area();
    crowd.rebuild(
        area,
        brutes
            .iter()
            .map(|(entity, _, transform, _)| (transform.translation.xz(), entity)),
    );
    let alignment = ATTACK_ANGLE.to_radians().cos();
    for (entity, mut brute, mut transform, mut squash) in &mut brutes {
        let at = transform.translation.xz();
        let heading = field.direction_at(at);
        let velocity = steer(
            brute.velocity,
            heading * brute.speed,
            BRUTE_STEERING_RATE,
            dt,
        );
        let push = separation(
            &crowd.index,
            at,
            entity,
            BRUTE_RADIUS * 2.0,
            SEPARATION_STIFFNESS,
            MAX_NEIGHBORS,
            brute.fallback,
        )
        .clamp_length_max(BRUTE_RADIUS * 0.5);
        let mut next = push_out(at + velocity * dt + push, BRUTE_RADIUS, |cell| {
            fields.cell_is_clear(cell)
        });
        if !fields.cell_is_clear(next.floor().as_ivec2()) {
            next = at;
        }

        // Smash the walls in the way, if its way leads through them.
        let here = field.node_at(next);
        let through = |cell| {
            fields.cell_is_breakable(cell) && field.distance_through(cell) < field.distance(here)
        };
        let walls: Vec<IVec2> = walls_in_the_way(
            next,
            BRUTE_RADIUS,
            ATTACK_REACH,
            heading,
            alignment,
            through,
        )
        .map(|(cell, _)| cell)
        .collect();
        let brute = &mut *brute;
        brute.smash_cooldown = (brute.smash_cooldown - dt).max(0.0);
        brute.slam = (brute.slam - dt).max(0.0);
        if !walls.is_empty() && brute.smash_cooldown <= 0.0 {
            brute.smash_cooldown = BRUTE_SMASH_INTERVAL;
            brute.slam = SMASH_SECONDS;
            hits.write_batch(walls.iter().map(|&cell| StructureHit {
                cell,
                damage: BRUTE_SMASH,
                by: Class::Brute,
                from: next,
            }));
            squash.pulse(SMASH_SQUASH);
            shake.add_capped(SMASH_SHAKE, SMASH_SHAKE_MAX);
        }

        // Face the walls it's smashing, or where it's going.
        if !walls.is_empty() {
            let middle =
                walls.iter().map(|cell| cell.as_vec2() + 0.5).sum::<Vec2>() / walls.len() as f32;
            brute.heading = (middle - next).normalize_or(brute.heading);
        } else if velocity.length_squared() > 0.01 {
            brute.heading = velocity.normalize();
        }
        brute.velocity = velocity;
        brute.stride += next.distance(at);
        brute.age += dt;

        // It rises out of the nest, rocks as it walks, rears up before a smash and slams down.
        let emerged = (brute.age / EMERGE_SECONDS).min(1.0);
        squash.rest = Vec3::splat(emerged);
        let rock = (brute.stride / BRUTE_STRIDE * std::f32::consts::TAU).sin() * BRUTE_ROCK;
        let rear = if walls.is_empty() {
            0.0
        } else {
            (1.0 - brute.smash_cooldown / SMASH_REAR_SECONDS).max(0.0) * SMASH_REAR
        };
        let dip = brute.slam / SMASH_SECONDS * SMASH_DIP;
        transform.translation = Vec3::new(next.x, -EMERGE_DEPTH * (1.0 - emerged), next.y);
        transform.rotation =
            facing(brute.heading) * Quat::from_rotation_x(rear - dip) * Quat::from_rotation_z(rock);
    }
    // Swarmers get out of the way of where the brutes are now.
    crowd.rebuild(
        area,
        brutes
            .iter()
            .map(|(entity, _, transform, _)| (transform.translation.xz(), entity)),
    );
}

pub(super) fn reach_command_center(
    mut commands: Commands,
    landmarks: Res<Landmarks>,
    brutes: Query<(Entity, &Transform), With<Brute>>,
    mut hits: MessageWriter<CommandCenterHit>,
) {
    let area = landmarks.command_center.area();
    for (entity, transform) in &brutes {
        if touches(transform.translation.xz(), BRUTE_RADIUS, area) {
            commands.entity(entity).despawn();
            hits.write(CommandCenterHit {
                at: transform.translation,
                damage: BRUTE_DAMAGE,
            });
        }
    }
}
