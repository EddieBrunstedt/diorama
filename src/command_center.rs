//! The Command Center: the heart of the outpost and what the horde is after. It stands in the
//! middle of the map from the start, as a structure on its cells (see [`Landmarks`]). Its HP is
//! kept with the map ([`Grid::command_center`]), and it can be repaired in the build phase like
//! anything built.
//!
//! A swarmer that reaches it bursts on it and deals damage: the building flinches and flashes,
//! and gibs fly. When its HP runs out it blows up in slow motion and slumps into a smoking ruin:
//! the run is over ([`CommandCenterFell`]). A new run drops a fresh one in.

use crate::grid::{CommandCenterHp, Grid};
use crate::horde::HordeSystems;
use crate::juice::{
    Burst, HitStop, Particles, Shake, Spray, Squash, Tween, TweenDone, TweenTarget,
};
use crate::map::Landmarks;
use crate::palette;
use crate::shapes::{BoxMesh, shade};
use crate::tuning::command_center::MAX_HP;
use crate::tuning::juice::*;
use crate::waves::{Phase, WaveSystems};
use bevy::prelude::*;

/// How tall it stands (to the top of its antenna).
pub const HEIGHT: f32 = 1.5;
/// How bright the hit flash glows at its peak.
const FLASH_BRIGHTNESS: f32 = 2.5;
/// A ruin is wider and much lower than the building, and charred.
const RUIN_SCALE: Vec3 = Vec3::new(1.12, 0.32, 1.12);
const RUIN_SHADE: f32 = 0.35;
/// A new one drops in from this high, taking this long (seconds).
const DROP_HEIGHT: f32 = 12.0;
const DROP_SECONDS: f32 = 0.3;

pub struct CommandCenterPlugin;

impl Plugin for CommandCenterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BaseHp>()
            .add_message::<CommandCenterHit>()
            .add_message::<CommandCenterFell>()
            .add_message::<CommandCenterRepaired>()
            .add_systems(Startup, (spawn_command_center, stand_up))
            .add_systems(OnExit(Phase::GameOver), (drop_in_a_new_one, stand_up))
            .add_systems(
                Update,
                (take_hits, show_repairs, flash, smoke)
                    .chain()
                    .after(HordeSystems)
                    .before(WaveSystems),
            )
            .add_observer(land);
    }
}

/// What a run's Command Center starts with (`DIORAMA_BASE_HP` overrides it, see
/// [`crate::dev`]).
#[derive(Resource, Clone, Copy, Debug)]
pub struct BaseHp(pub u32);

impl Default for BaseHp {
    fn default() -> Self {
        Self(MAX_HP)
    }
}

/// A run starts with the Command Center whole.
fn stand_up(base: Res<BaseHp>, mut grid: ResMut<Grid>) {
    grid.command_center = CommandCenterHp {
        hp: base.0,
        max: base.0,
    };
}

/// Something reached the Command Center at `at` and hit it.
#[derive(Message, Clone, Copy, Debug)]
pub struct CommandCenterHit {
    pub at: Vec3,
    pub damage: u32,
}

/// The Command Center's HP ran out: the run is over.
#[derive(Message, Clone, Copy, Debug)]
pub struct CommandCenterFell;

/// The player repaired the Command Center (or undid a repair) from `from` HP to `to`.
#[derive(Message, Clone, Copy, Debug)]
pub struct CommandCenterRepaired {
    pub from: u32,
    pub to: u32,
}

#[derive(Component)]
struct CommandCenterView {
    /// Its own material, so it can flash without lighting up everything else.
    material: Handle<StandardMaterial>,
    /// Seconds of flash left, and how strong it started.
    flash: f32,
    strength: f32,
    /// Blown up: a smoking ruin. And whether its material is charred to match.
    ruined: bool,
    charred: bool,
}

fn spawn_command_center(
    mut commands: Commands,
    landmarks: Res<Landmarks>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let footprint = landmarks.command_center;
    let material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.8,
        reflectance: 0.25,
        ..default()
    });
    commands.spawn((
        CommandCenterView {
            material: material.clone(),
            flash: 0.0,
            strength: 0.0,
            ruined: false,
            charred: false,
        },
        Mesh3d(meshes.add(command_center_mesh(footprint.size as f32))),
        MeshMaterial3d(material),
        Transform::from_translation(footprint.center()),
        Squash::new(Vec3::ONE),
    ));
}

/// A squat bunker `size` cells across, centered on the origin: a concrete plinth with a hazard
/// edge, a steel block with lit windows, and an antenna with a light on top.
fn command_center_mesh(size: f32) -> Mesh {
    let mut mesh = BoxMesh::default();
    let half = size / 2.0 - 0.05;
    let cube = |mesh: &mut BoxMesh, half: Vec2, low: f32, high: f32, color: Color| {
        mesh.cuboid(
            Vec3::new(-half.x, low, -half.y),
            Vec3::new(half.x, high, half.y),
            color,
        );
    };
    cube(&mut mesh, Vec2::splat(half), 0.0, 0.2, palette::CONCRETE);
    cube(&mut mesh, Vec2::splat(half), 0.2, 0.26, palette::HAZARD);
    // Corner pylons.
    for (x, z) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let at = Vec2::new(x, z) * (half - 0.14);
        mesh.cuboid(
            Vec3::new(at.x - 0.14, 0.26, at.y - 0.14),
            Vec3::new(at.x + 0.14, 0.55, at.y + 0.14),
            shade(palette::CONCRETE, 0.9),
        );
    }
    let block = half * 0.74;
    cube(&mut mesh, Vec2::splat(block), 0.26, 0.95, palette::STEEL);
    // A band of lit windows around the block.
    let windows = block + 0.015;
    cube(
        &mut mesh,
        Vec2::new(windows, block * 0.7),
        0.62,
        0.72,
        palette::BASE_LIGHT,
    );
    cube(
        &mut mesh,
        Vec2::new(block * 0.7, windows),
        0.62,
        0.72,
        palette::BASE_LIGHT,
    );
    let top = half * 0.46;
    cube(
        &mut mesh,
        Vec2::splat(top),
        0.95,
        1.2,
        shade(palette::STEEL, 1.25),
    );
    // Antenna and its light.
    let mast = Vec2::new(0.22, -0.18);
    mesh.cuboid(
        Vec3::new(mast.x - 0.03, 1.2, mast.y - 0.03),
        Vec3::new(mast.x + 0.03, HEIGHT - 0.08, mast.y + 0.03),
        palette::STEEL,
    );
    mesh.cuboid_graded(
        Vec3::new(mast.x - 0.06, HEIGHT - 0.08, mast.y - 0.06),
        Vec3::new(mast.x + 0.06, HEIGHT, mast.y + 0.06),
        palette::BASE_LIGHT,
        palette::BASE_LIGHT,
    );
    mesh.build()
}

fn take_hits(
    mut commands: Commands,
    mut hits: MessageReader<CommandCenterHit>,
    mut grid: ResMut<Grid>,
    mut view: Single<(&mut CommandCenterView, &mut Squash, &Transform)>,
    mut particles: Particles,
    mut shake: ResMut<Shake>,
    mut hit_stop: ResMut<HitStop>,
    mut fell: MessageWriter<CommandCenterFell>,
) {
    let mut count = 0;
    let mut fallen = false;
    for hit in hits.read() {
        fallen |= grid.command_center.damage(hit.damage);
        // A few bursts a frame is plenty to read, even when a whole pile arrives at once.
        if count < MAX_GIB_BURSTS {
            particles.burst(
                &mut commands,
                hit.at + Vec3::Y * 0.15,
                0.05,
                Burst {
                    colors: &palette::GIBS,
                    count: GIB_COUNT,
                    size: GIB_SIZE,
                    spread: GIB_SPREAD,
                    lift: GIB_LIFT,
                    lifetime: GIB_LIFETIME,
                },
            );
        }
        count += 1;
    }
    if count == 0 {
        return;
    }
    let (view, squash, transform) = &mut *view;
    squash.pulse(BASE_HIT_SQUASH);
    shake.add_capped(BASE_HIT_SHAKE, BASE_HIT_SHAKE_MAX);
    if fallen {
        fell.write(CommandCenterFell);
        blow_up(&mut commands, &mut particles, transform.translation);
        shake.add(BASE_FALL_SHAKE);
        hit_stop.slow_motion(BASE_FALL_SLOW_MOTION, BASE_FALL_SLOW_SPEED);
        view.flash = BASE_FALL_FLASH_SECONDS;
        view.strength = 3.0;
        view.ruined = true;
        // It slumps into a charred heap.
        squash.rest = RUIN_SCALE;
        squash.pulse(-0.6);
    } else if view.flash <= BASE_HIT_FLASH_SECONDS {
        view.flash = BASE_HIT_FLASH_SECONDS;
        view.strength = 1.0;
    }
}

/// The Command Center blowing up: chunks of concrete and steel, a fireball, and dust.
fn blow_up(commands: &mut Commands, particles: &mut Particles, at: Vec3) {
    particles.burst(
        commands,
        at + Vec3::Y * 0.6,
        0.8,
        Burst {
            colors: &palette::RUBBLE,
            count: BASE_FALL_DEBRIS,
            size: RUBBLE_SIZE * 1.6,
            spread: RUBBLE_SPREAD * 2.5,
            lift: RUBBLE_LIFT * 2.0,
            lifetime: RUBBLE_LIFETIME * 2.5,
        },
    );
    particles.spray(
        commands,
        at + Vec3::Y * 0.8,
        Spray {
            colors: &palette::FLAME,
            count: BASE_FALL_DEBRIS,
            size: 0.3,
            direction: Vec3::Y,
            cone: 1.2,
            speed: (2.0, 7.0),
            gravity: 0.3,
            grow: 2.0,
            lifetime: 0.8,
            glow: true,
        },
    );
    particles.burst(
        commands,
        at + Vec3::Y * 0.1,
        1.0,
        Burst {
            colors: &palette::DUST,
            count: BASE_FALL_DEBRIS,
            size: DUST_SIZE * 1.5,
            spread: DUST_SPREAD * 2.0,
            lift: DUST_LIFT,
            lifetime: DUST_LIFETIME * 2.0,
        },
    );
}

/// A repair pops the building and throws off sparks.
fn show_repairs(
    mut commands: Commands,
    mut repairs: MessageReader<CommandCenterRepaired>,
    mut view: Single<(&mut Squash, &Transform), With<CommandCenterView>>,
    mut particles: Particles,
) {
    let mut repaired = false;
    for repair in repairs.read() {
        repaired |= repair.to > repair.from;
    }
    if !repaired {
        return;
    }
    let (squash, transform) = &mut *view;
    squash.pulse(REPAIR_POP);
    particles.spray(
        &mut commands,
        transform.translation + Vec3::Y * HEIGHT * 0.7,
        Spray {
            colors: &palette::SPARKS,
            count: 16,
            size: 0.07,
            direction: Vec3::Y,
            cone: 1.1,
            speed: (1.5, 3.5),
            gravity: 0.6,
            grow: 1.0,
            lifetime: 0.45,
            glow: true,
        },
    );
}

/// Glows red for a moment after a hit, and looks charred while it's a ruin.
fn flash(
    time: Res<Time>,
    mut view: Single<&mut CommandCenterView>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if view.charred != view.ruined
        && let Some(mut material) = materials.get_mut(&view.material)
    {
        material.base_color = if view.ruined {
            shade(Color::WHITE, RUIN_SHADE)
        } else {
            Color::WHITE
        };
        view.charred = view.ruined;
    }
    if view.flash <= 0.0 {
        return;
    }
    view.flash = (view.flash - time.delta_secs()).max(0.0);
    let duration = if view.strength > 1.0 {
        BASE_FALL_FLASH_SECONDS
    } else {
        BASE_HIT_FLASH_SECONDS
    };
    let glow = view.flash / duration * view.strength * FLASH_BRIGHTNESS;
    if let Some(mut material) = materials.get_mut(&view.material) {
        material.emissive = (palette::BASE_HIT.to_linear() * glow).with_alpha(1.0);
    }
}

/// A ruin smokes.
fn smoke(
    mut commands: Commands,
    time: Res<Time>,
    view: Single<(&CommandCenterView, &Transform)>,
    mut particles: Particles,
    mut since: Local<f32>,
) {
    let (view, transform) = *view;
    if !view.ruined {
        return;
    }
    *since += time.delta_secs();
    if *since < RUIN_SMOKE_INTERVAL {
        return;
    }
    *since = 0.0;
    particles.spray(
        &mut commands,
        transform.translation + Vec3::Y * 0.4,
        Spray {
            colors: &palette::SMOKE,
            count: 3,
            size: 0.3,
            direction: Vec3::Y,
            cone: 0.5,
            speed: (0.5, 1.2),
            gravity: -0.05,
            grow: 3.0,
            lifetime: 1.6,
            glow: false,
        },
    );
}

/// A new run: a fresh Command Center drops in where the ruin was.
fn drop_in_a_new_one(
    mut commands: Commands,
    landmarks: Res<Landmarks>,
    mut view: Single<(Entity, &mut CommandCenterView, &mut Squash)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (entity, view, squash) = &mut *view;
    view.ruined = false;
    view.flash = 0.0;
    squash.rest = Vec3::ONE;
    if let Some(mut material) = materials.get_mut(&view.material) {
        material.emissive = LinearRgba::BLACK;
    }
    let at = landmarks.command_center.center();
    commands.entity(*entity).insert(Tween::new(
        TweenTarget::Translation {
            from: at + Vec3::Y * DROP_HEIGHT,
            to: at,
        },
        EaseFunction::QuadraticIn,
        0.0,
        DROP_SECONDS,
    ));
}

/// It lands: a heavy slam, a ring of dust and a big bump.
fn land(
    done: On<TweenDone>,
    mut commands: Commands,
    mut views: Query<(&Transform, &mut Squash), With<CommandCenterView>>,
    mut particles: Particles,
    mut shake: ResMut<Shake>,
) {
    let Ok((transform, mut squash)) = views.get_mut(done.entity) else {
        return;
    };
    squash.pulse(LAND_SQUASH);
    shake.add(TOWER_LAND_SHAKE * 1.5);
    particles.burst(
        &mut commands,
        transform.translation + Vec3::Y * 0.05,
        1.0,
        Burst {
            colors: &palette::DUST,
            count: TOWER_DUST_COUNT * 2,
            size: DUST_SIZE * 1.3,
            spread: DUST_SPREAD * 1.5,
            lift: DUST_LIFT,
            lifetime: DUST_LIFETIME,
        },
    );
}
