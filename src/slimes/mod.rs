//! Slimes: they come out at the road's open end during a wave, hop down the road to the hearth
//! and leap into its fire. Arrows pop them into a shower of voxel cubes.
//!
//! The road, the hop, the waves and slime health are plain data in [`road`] and [`wave`]. The rest
//! is the ECS side: spawning, moving, reacting to hits, and damaging the hearth.

mod model;
pub mod road;
pub mod wave;

pub use wave::{Health, WaveRun, WaveSpec};

use crate::hearth::{HearthHealth, HearthHit};
use crate::island::{Island, TileKind, cell_to_world};
use crate::juice::{Burst, Flight, FlightTarget, Landed, Particles, Squash, Tween};
use crate::palette;
use crate::phase::{GameState, Waves};
use crate::rng::Rng;
use crate::tuning::{SLIME_DAMAGE, SLIME_HP_FIRST_WAVE, SLIME_SPEED};
use crate::voxel::{VOXEL, VoxelMaterial, shade};
use bevy::prelude::*;
use road::{HOP_SECONDS, RoadPath, air_speed, hop};

/// Height of a slime's middle above the ground, at scale 1.
pub const MIDDLE_HEIGHT: f32 = model::MIDDLE * VOXEL;
/// The road surface sits a little below the grass.
const ROAD_Y: f32 = -0.03;
/// A wave-1 slime's size; tougher slimes are bigger.
const BASE_SIZE: f32 = 0.95;
const SIZE_PER_HP: f32 = 0.08;
const MAX_SIZE: f32 = 1.5;
/// Slimes wander this far to either side of the road's middle.
const MAX_OFFSET: f32 = 0.06;
const TURN_RATE: f32 = 10.0;
/// How quickly a slime catches up with where it should be (per second, exponential). Smooths
/// the jump in sideways offset at the road's bends.
const FOLLOW_RATE: f32 = 25.0;
const FLASH_TIME: f32 = 0.08;
/// Where on the hearth a leaping slime lands, above the tile's middle.
const HEARTH_TARGET_HEIGHT: f32 = 0.25;
const LEAP_HEIGHT: f32 = 0.45;
const LEAP_TIME: f32 = 0.42;

pub struct SlimesPlugin;

impl Plugin for SlimesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VoxelMaterial>()
            .init_resource::<SlimeAssets>()
            .init_resource::<Road>()
            .init_resource::<WaveRun>()
            .add_message::<ArrowHit>()
            .add_systems(OnEnter(GameState::Wave), start_wave_run)
            .add_systems(OnExit(GameState::GameOver), clear_slimes)
            .add_systems(
                Update,
                (
                    update_road,
                    spawn_slimes.run_if(in_state(GameState::Wave)),
                    hop_slimes,
                    take_hits,
                    fade_flashes,
                )
                    .chain(),
            )
            .add_observer(into_hearth);
    }
}

/// A slime on its way to the hearth.
#[derive(Component, Debug)]
pub struct Slime {
    pub health: Health,
    /// How far along the road it is.
    pub distance: f32,
    /// Sideways from the road's middle, so they don't march in a perfect line.
    offset: f32,
    /// Hops so far; the fraction is where in the current hop.
    hop: f32,
    /// The child that squashes and flashes.
    body: Entity,
    /// Seconds left of the white flash after a hit.
    flash: f32,
}

/// A slime leaping into the hearth. It does its damage when it lands.
#[derive(Component, Debug)]
struct Leaping;

/// The slimes' road, recomputed when the island changes.
#[derive(Resource, Default)]
pub struct Road {
    pub path: Option<RoadPath>,
    /// The hearth's middle (world x/z).
    pub hearth: Vec2,
}

/// An arrow hit a slime.
#[derive(Message, Clone, Copy, Debug)]
pub struct ArrowHit {
    pub slime: Entity,
    pub damage: u32,
}

#[derive(Resource)]
struct SlimeAssets {
    mesh: Handle<Mesh>,
    normal: Handle<StandardMaterial>,
    flash: Handle<StandardMaterial>,
}

impl FromWorld for SlimeAssets {
    fn from_world(world: &mut World) -> Self {
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(model::slime_mesh().build());
        let normal = world.resource::<VoxelMaterial>().0.clone();
        let flash = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: Color::WHITE,
                emissive: LinearRgba::rgb(1.0, 0.95, 1.0) * 0.9,
                perceptual_roughness: 0.9,
                ..default()
            });
        Self {
            mesh,
            normal,
            flash,
        }
    }
}

/// How big a slime with this many hit points is.
fn slime_size(hp: u32) -> f32 {
    (BASE_SIZE * (1.0 + SIZE_PER_HP * hp.saturating_sub(SLIME_HP_FIRST_WAVE) as f32)).min(MAX_SIZE)
}

fn facing(direction: Vec2) -> Quat {
    Quat::from_rotation_y(direction.x.atan2(direction.y))
}

fn update_road(island: Res<Island>, mut road: ResMut<Road>) {
    if !island.is_changed() {
        return;
    }
    road.path = RoadPath::from_island(&island);
    road.hearth = island
        .tiles()
        .find(|&(_, kind)| kind == TileKind::Hearth)
        .map_or(Vec2::ZERO, |(cell, _)| cell_to_world(cell).xz());
}

fn start_wave_run(waves: Res<Waves>, road: Res<Road>, mut run: ResMut<WaveRun>) {
    let mut spec = WaveSpec::for_wave(waves.current());
    if road.path.is_none() {
        // Nowhere to come from.
        spec.count = 0;
    }
    *run = WaveRun::new(spec);
}

fn spawn_slimes(
    mut commands: Commands,
    time: Res<Time>,
    mut run: ResMut<WaveRun>,
    road: Res<Road>,
    assets: Res<SlimeAssets>,
    mut particles: Particles,
    mut rng: Local<Option<Rng>>,
) {
    let due = run.tick(time.delta_secs());
    let Some(path) = &road.path else {
        return;
    };
    let rng = rng.get_or_insert_with(|| Rng::new(0x5_11AE));
    for _ in 0..due {
        let (point, direction) = path.sample(0.0);
        let at = Vec3::new(point.x, ROAD_Y, point.y);
        let pop = Tween::pop_in(Vec3::splat(slime_size(run.spec.hp)), 0.0);
        let body = commands
            .spawn((
                Mesh3d(assets.mesh.clone()),
                MeshMaterial3d(assets.normal.clone()),
                Transform::default(),
                Squash::new(Vec3::ONE),
            ))
            .id();
        commands
            .spawn((
                Slime {
                    health: Health::new(run.spec.hp),
                    distance: 0.0,
                    offset: rng.range(-MAX_OFFSET, MAX_OFFSET),
                    hop: rng.range(0.0, 0.3),
                    body,
                    flash: 0.0,
                },
                Transform {
                    translation: at,
                    rotation: facing(direction),
                    ..pop.start_transform()
                },
                Visibility::Hidden,
                pop,
            ))
            .add_child(body);
        // A puff as it comes out of the fog.
        particles.burst(
            &mut commands,
            at + Vec3::Y * 0.1,
            Burst {
                colors: &[palette::CLOUD, palette::CLOUD_SHADE, palette::SLIME],
                count: 8,
                size: 0.07,
                spread: 0.8,
                lift: 1.0,
                lifetime: 0.6,
            },
        );
    }
}

/// Hops slimes down the road, squashing as they land and stretching as they take off. At the
/// hearth they leap in. After game over they hop in place, celebrating.
fn hop_slimes(
    mut commands: Commands,
    time: Res<Time>,
    state: Res<State<GameState>>,
    road: Res<Road>,
    mut slimes: Query<(Entity, &mut Slime, &mut Transform)>,
    mut squashes: Query<&mut Squash>,
) {
    let Some(path) = &road.path else {
        return;
    };
    let advancing = *state.get() == GameState::Wave;
    let dt = time.delta_secs();
    for (entity, mut slime, mut transform) in &mut slimes {
        let before = slime.hop;
        slime.hop += dt / HOP_SECONDS;
        let now = hop(slime.hop);
        if advancing && now.airborne {
            slime.distance += air_speed(SLIME_SPEED) * dt;
        }
        if let Ok(mut squash) = squashes.get_mut(slime.body) {
            if before.floor() != slime.hop.floor() {
                squash.pulse(-0.3);
            } else if !hop(before).airborne && now.airborne {
                squash.pulse(0.28);
            }
        }

        let (point, direction) = path.sample(slime.distance);
        let side = Vec2::new(-direction.y, direction.x) * slime.offset;
        let goal = point + side;
        let follow = (FOLLOW_RATE * dt).min(1.0);
        let xz = transform.translation.xz().lerp(goal, follow);
        transform.translation = Vec3::new(xz.x, ROAD_Y + now.height, xz.y);
        transform.rotation = transform
            .rotation
            .slerp(facing(direction), (TURN_RATE * dt).min(1.0));

        if advancing && slime.distance >= path.length() {
            let hearth = Vec3::new(road.hearth.x, HEARTH_TARGET_HEIGHT, road.hearth.y);
            commands.entity(entity).remove::<Slime>().insert((
                Leaping,
                Flight::new(
                    transform.translation,
                    FlightTarget::Point(hearth),
                    LEAP_HEIGHT,
                    LEAP_TIME,
                ),
            ));
            if let Ok(mut squash) = squashes.get_mut(slime.body) {
                squash.pulse(0.45);
            }
        }
    }
}

/// Arrows flash and squash slimes, and pop them into cubes when their hit points run out.
fn take_hits(
    mut commands: Commands,
    mut hits: MessageReader<ArrowHit>,
    mut slimes: Query<(&mut Slime, &GlobalTransform)>,
    mut bodies: Query<(&mut Squash, &mut MeshMaterial3d<StandardMaterial>)>,
    assets: Res<SlimeAssets>,
    mut run: ResMut<WaveRun>,
    mut particles: Particles,
) {
    for hit in hits.read() {
        let Ok((mut slime, global)) = slimes.get_mut(hit.slime) else {
            continue;
        };
        if slime.health.hp == 0 {
            // Already popped by an earlier hit this frame.
            continue;
        }
        let middle = global.transform_point(Vec3::Y * MIDDLE_HEIGHT);
        if slime.health.hit(hit.damage) {
            commands.entity(hit.slime).despawn();
            run.slime_gone();
            particles.burst(
                &mut commands,
                middle,
                Burst {
                    colors: &[
                        palette::SLIME,
                        shade(palette::SLIME, 0.8),
                        palette::SLIME.mix(&Color::WHITE, 0.4),
                    ],
                    count: 18,
                    size: 0.075,
                    spread: 1.6,
                    lift: 2.6,
                    lifetime: 0.9,
                },
            );
            continue;
        }
        slime.flash = FLASH_TIME;
        if let Ok((mut squash, mut material)) = bodies.get_mut(slime.body) {
            squash.pulse(-0.35);
            material.0 = assets.flash.clone();
        }
        particles.burst(
            &mut commands,
            middle,
            Burst {
                colors: &[palette::SLIME, palette::SLIME.mix(&Color::WHITE, 0.4)],
                count: 4,
                size: 0.04,
                spread: 1.0,
                lift: 1.5,
                lifetime: 0.5,
            },
        );
    }
}

fn fade_flashes(
    time: Res<Time>,
    mut slimes: Query<&mut Slime>,
    mut materials: Query<&mut MeshMaterial3d<StandardMaterial>>,
    assets: Res<SlimeAssets>,
) {
    for mut slime in &mut slimes {
        if slime.flash <= 0.0 {
            continue;
        }
        slime.flash -= time.delta_secs();
        if slime.flash <= 0.0
            && let Ok(mut material) = materials.get_mut(slime.body)
        {
            material.0 = assets.normal.clone();
        }
    }
}

/// A leaping slime lands in the hearth: it hurts the hearth and splats.
fn into_hearth(
    landed: On<Landed>,
    mut commands: Commands,
    leaping: Query<&GlobalTransform, With<Leaping>>,
    mut health: ResMut<HearthHealth>,
    mut run: ResMut<WaveRun>,
    mut hits: MessageWriter<HearthHit>,
    mut particles: Particles,
) {
    let Ok(global) = leaping.get(landed.entity) else {
        return;
    };
    health.damage(SLIME_DAMAGE);
    run.slime_gone();
    hits.write(HearthHit {
        at: global.translation(),
    });
    particles.burst(
        &mut commands,
        global.translation(),
        Burst {
            colors: &[palette::SLIME, shade(palette::SLIME, 0.8)],
            count: 12,
            size: 0.06,
            spread: 1.4,
            lift: 2.0,
            lifetime: 0.7,
        },
    );
}

fn clear_slimes(
    mut commands: Commands,
    slimes: Query<Entity, Or<(With<Slime>, With<Leaping>)>>,
    mut run: ResMut<WaveRun>,
) {
    for slime in &slimes {
        commands.entity(slime).despawn();
    }
    *run = WaveRun::default();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tougher_slimes_are_bigger_up_to_a_point() {
        assert_eq!(slime_size(SLIME_HP_FIRST_WAVE), BASE_SIZE);
        assert!(slime_size(SLIME_HP_FIRST_WAVE + 1) > BASE_SIZE);
        assert_eq!(slime_size(100), MAX_SIZE);
    }
}
