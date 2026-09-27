//! The catapult: the keeper's one direct attack. He builds it like a tower, and during the wave a
//! "man" order puts him in it. While he's in, holding the left mouse button charges it (the arm
//! creaks back) and letting go throws a boulder at the aimed spot, which lands with a splat, a
//! shake and a moment of hit-stop. Then he winds it back and loads a new boulder before the next
//! shot. Right-click or Esc gets him out.
//!
//! Charging, reloading, aiming, flight and splash are plain data in [`shot`]; the models are in
//! [`model`]. Getting in and out is the keeper's side (see `keeper::manning`): he marks the
//! catapult [`Manned`] once he's in.

pub mod model;
pub mod shot;

use crate::island::Island;
use crate::island::cell_to_world;
use crate::juice::{Burst, HitStop, Particles, Shake, Squash, Tween};
use crate::keeper::{Aim, PlanSystems};
use crate::lamps::{GlowMaterials, flash, lamp};
use crate::palette;
use crate::phase::GameState;
use crate::slimes::{HitBy, Road, Slime, SlimeHit};
use crate::ui::Popup;
use crate::voxel::{TILE_SIZE, VOXEL, VoxelMaterial};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use shot::{Loader, Shot, boulder_position, clamp_aim, flight, shot, splash};

pub const LEAVE_KEY: KeyCode = KeyCode::Escape;
/// A finished catapult drops in from this high, after the logs have landed.
const DROP_HEIGHT: f32 = 1.2;
const DROP_DELAY: f32 = 0.3;
/// How quickly the catapult turns toward where it's aimed (per second, exponential).
const TURN_RATE: f32 = 7.0;
/// Turning while nobody's in it: slowly, to keep an eye on the road.
const IDLE_TURN_RATE: f32 = 1.5;
/// How far back charging pulls the arm (radians past resting).
const ARM_PULL: f32 = 0.3;
/// The arm's spring, and the kick it gets when let go.
const ARM_STIFFNESS: f32 = 240.0;
const ARM_DAMPING: f32 = 13.0;
const ARM_KICK: f32 = 16.0;
/// The boulder leaves the bucket this far (radians) before the arm hits the crossbar.
const LAUNCH_BEFORE_STOP: f32 = 0.3;
/// A burning boulder's light at full dusk, and how far it reaches.
const BOULDER_LUMENS: f32 = 12_000.0;
const BOULDER_LIGHT_RANGE: f32 = 1.8;
/// The flash where a boulder lands (a full charge's), whatever the mood.
const IMPACT_LUMENS: f32 = 60_000.0;
const IMPACT_FLASH_RANGE: f32 = 2.5;
const IMPACT_FLASH_TIME: f32 = 0.35;
const IMPACT_FLASH_HEIGHT: f32 = 0.3;
/// Screen shake and hit-stop (real seconds) for a shot landing: a base, plus more for a full
/// charge and for every slime popped.
const IMPACT_SHAKE: f32 = 0.22;
const FULL_CHARGE_SHAKE: f32 = 0.3;
const HIT_STOP: f32 = 0.05;
const HIT_STOP_PER_POP: f32 = 0.035;
/// Boulders thrown past the island's edge fall this far below it before they're gone.
const LOST_DEPTH: f32 = -4.0;
/// How long the dirt churned up by a boulder stays on the ground, and how many clods.
const CRATER_TIME: f32 = 2.5;
const CRATER_CLODS: usize = 9;
/// Dots in the aiming ring and in the arc preview.
const RING_DOTS: usize = 20;
const ARC_DOTS: usize = 10;
const DOT_SIZE: f32 = 0.065;

pub struct CatapultPlugin;

impl Plugin for CatapultPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VoxelMaterial>()
            .init_resource::<CatapultAssets>()
            .init_resource::<CatapultAim>()
            .add_message::<LeaveCatapult>()
            .add_systems(OnExit(GameState::GameOver), clear_catapults)
            .add_systems(
                Update,
                (
                    work_catapult,
                    swing_arms,
                    fly_boulders,
                    fade_craters,
                    land_catapults,
                    show_reticle,
                )
                    .chain()
                    .after(PlanSystems),
            );
    }
}

/// The catapult on the island (there's only room for one).
#[derive(Component, Debug)]
pub struct Catapult {
    pub cell: IVec2,
    pub loader: Loader,
    /// The way it faces (yaw, radians).
    yaw: f32,
    body: Entity,
    arm: Entity,
    /// The boulder waiting in the bucket, and whether it's there.
    loaded: Entity,
    bucket_full: bool,
    /// The arm's angle and how fast it's swinging.
    arm_angle: f32,
    arm_speed: f32,
    /// A shot that's been let go but hasn't left the bucket yet: it goes when the arm swings up.
    pending: Option<(Shot, Vec2)>,
}

/// The keeper is in the catapult and working it.
#[derive(Component, Debug)]
pub struct Manned;

/// Asks the keeper to get out of the catapult.
#[derive(Message, Clone, Copy, Debug)]
pub struct LeaveCatapult;

/// How the catapult is being aimed, for the UI.
#[derive(Resource, Default, Debug)]
pub struct CatapultAim {
    /// The keeper is in the catapult.
    pub manned: bool,
    /// Where a shot would land (world x/z), while aiming at the island.
    pub target: Option<Vec2>,
    /// The aim was out of reach and got pulled in.
    pub clamped: bool,
    /// A shot would land past the island's edge.
    pub past_edge: bool,
    pub loader: Loader,
}

/// A boulder in flight.
#[derive(Component, Debug)]
struct Boulder {
    from: Vec3,
    to: Vec3,
    height: f32,
    age: f32,
    duration: f32,
    shot: Shot,
    spin: Vec3,
    /// Thrown past the island's edge: it drops away into the clouds.
    lost: bool,
}

/// A catapult that's still dropping into place.
#[derive(Component)]
struct Landing;

/// A clod of dirt where a boulder landed, fading away.
#[derive(Component)]
struct Crater {
    age: f32,
    size: Vec3,
}

/// One dot of the aiming ring (`ring`) or the arc preview.
#[derive(Component)]
struct AimDot {
    ring: bool,
}

#[derive(Resource)]
pub struct CatapultAssets {
    base: Handle<Mesh>,
    arm: Handle<Mesh>,
    boulder: Handle<Mesh>,
    voxel: Handle<StandardMaterial>,
    /// Boulders smoulder at dusk.
    ember: Handle<StandardMaterial>,
}

impl FromWorld for CatapultAssets {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        let base = meshes.add(model::base_mesh().build());
        let arm = meshes.add(model::arm_mesh().build());
        let boulder = meshes.add(model::boulder_mesh().build());
        let voxel = world.resource::<VoxelMaterial>().0.clone();
        let ember = world.get_resource_or_init::<GlowMaterials>().ember.clone();
        Self {
            base,
            arm,
            boulder,
            voxel,
            ember,
        }
    }
}

/// Replaces a finished building site with the catapult, which drops into place loaded.
pub fn finish_catapult(
    commands: &mut Commands,
    assets: &CatapultAssets,
    site: Entity,
    cell: IVec2,
) {
    commands.entity(site).despawn();
    let drop = Tween::drop_in(cell_to_world(cell), DROP_HEIGHT, DROP_DELAY);
    let mesh =
        |handle: &Handle<Mesh>| (Mesh3d(handle.clone()), MeshMaterial3d(assets.voxel.clone()));
    let start = drop.start_transform().with_scale(Vec3::splat(model::SCALE));
    let root = commands
        .spawn((start, Visibility::Hidden, drop, Landing))
        .id();
    let body = commands
        .spawn((
            mesh(&assets.base),
            Transform::default(),
            Squash::new(Vec3::ONE),
            ChildOf(root),
        ))
        .id();
    let arm = commands
        .spawn((
            mesh(&assets.arm),
            Transform::from_translation(model::PIVOT * VOXEL)
                .with_rotation(Quat::from_rotation_x(model::ARM_REST)),
            ChildOf(body),
        ))
        .id();
    let loaded = commands
        .spawn((
            Mesh3d(assets.boulder.clone()),
            MeshMaterial3d(assets.ember.clone()),
            Transform::from_translation(model::BUCKET * VOXEL),
            ChildOf(arm),
        ))
        .id();
    commands.entity(root).insert(Catapult {
        cell,
        loader: Loader::default(),
        yaw: 0.0,
        body,
        arm,
        loaded,
        bucket_full: true,
        arm_angle: model::ARM_REST,
        arm_speed: 0.0,
        pending: None,
    });
}

/// Where the keeper stands on the catapult (world space).
pub fn seat(catapult: &GlobalTransform) -> Vec3 {
    catapult.transform_point(model::SEAT * VOXEL)
}

/// Whether a spot (world x/z) is on one of the island's tiles.
fn on_island(island: &Island, at: Vec2) -> bool {
    island.get((at / TILE_SIZE).round().as_ivec2()).is_some()
}

/// Aiming, charging and letting go, while the keeper is in the catapult.
fn work_catapult(
    time: Res<Time>,
    state: Res<State<GameState>>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    aim: Res<Aim>,
    island: Res<Island>,
    mut catapults: Query<(&mut Catapult, &Transform, Has<Manned>)>,
    mut aiming: ResMut<CatapultAim>,
    mut leave: MessageWriter<LeaveCatapult>,
    mut popups: MessageWriter<Popup>,
) {
    let Ok((mut catapult, transform, manned)) = catapults.single_mut() else {
        *aiming = CatapultAim::default();
        return;
    };
    catapult.loader.tick(time.delta_secs());
    // After game over he may still stand in it, but it's no use any more.
    let manned = manned && *state.get() == GameState::Wave;
    let from = transform.translation.xz();
    let target = aim.ground.map(|ground| clamp_aim(from, ground.xz()));
    *aiming = CatapultAim {
        manned,
        target: target.filter(|_| manned).map(|(at, _)| at),
        clamped: manned && target.is_some_and(|(_, clamped)| clamped),
        past_edge: manned && target.is_some_and(|(at, _)| !on_island(&island, at)),
        loader: catapult.loader,
    };
    if !manned {
        catapult.loader.cancel();
        return;
    }
    if mouse.just_pressed(MouseButton::Right) || keys.just_pressed(LEAVE_KEY) {
        catapult.loader.cancel();
        leave.write(LeaveCatapult);
        return;
    }
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let head = transform.translation + Vec3::Y * 0.8;
    if mouse.just_pressed(MouseButton::Left) && !shift && target.is_some() {
        let charging = catapult.loader.press();
        if !charging && matches!(catapult.loader, Loader::Reloading { .. }) {
            popups.write(Popup::new(head, "Reloading...", palette::UI_INK_SOFT));
        }
    }
    if mouse.just_released(MouseButton::Left) && matches!(catapult.loader, Loader::Charging { .. })
    {
        match (catapult.loader.release(), target) {
            (Some(charge), Some((at, _))) => {
                catapult.pending = Some((shot(charge), at));
                catapult.arm_speed = ARM_KICK;
            }
            (Some(_), None) => {
                // Let go over the UI: nowhere to throw. Keep the boulder.
                catapult.loader = Loader::Ready;
            }
            (None, _) => {
                popups.write(Popup::new(head, "Hold to charge", palette::UI_INK_SOFT));
            }
        }
    }
    aiming.loader = catapult.loader;
}

/// Turns the catapult toward its aim, swings the arm (creaking back while charging, snapping up
/// to throw, winding back down to reload), and throws the boulder at the top of the swing.
fn swing_arms(
    mut commands: Commands,
    time: Res<Time>,
    aim: Res<CatapultAim>,
    road: Res<Road>,
    island: Res<Island>,
    assets: Res<CatapultAssets>,
    mut catapults: Query<(&mut Catapult, &mut Transform, Has<Manned>), Without<Tween>>,
    mut arms: Query<&mut Transform, (Without<Catapult>, Without<Boulder>)>,
    mut squashes: Query<&mut Squash>,
    mut shake: ResMut<Shake>,
    mut particles: Particles,
) {
    let dt = time.delta_secs().min(1.0 / 30.0);
    let t = time.elapsed_secs();
    for (mut catapult, mut transform, manned) in &mut catapults {
        let from = transform.translation.xz();
        // Face the aim while manned; otherwise keep an eye on where the slimes come from.
        let looking_at = if manned {
            aim.target.or_else(|| catapult.pending.map(|(_, at)| at))
        } else {
            road.path.as_ref().map(|path| path.sample(0.0).0)
        };
        if let Some(at) = looking_at.filter(|at| at.distance(from) > 0.1) {
            let wanted = (at.x - from.x).atan2(at.y - from.y);
            let turn = (wanted - catapult.yaw + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            let rate = if manned { TURN_RATE } else { IDLE_TURN_RATE };
            catapult.yaw += turn * (1.0 - (-rate * time.delta_secs()).exp());
        }
        transform.rotation = Quat::from_rotation_y(catapult.yaw);

        // The arm follows a spring toward where the loader wants it.
        let goal = match catapult.loader {
            Loader::Ready => model::ARM_REST,
            Loader::Charging { .. } => {
                let charge = catapult.loader.charge();
                let strain = if charge >= 1.0 {
                    0.03 * (t * 40.0).sin()
                } else {
                    0.0
                };
                model::ARM_REST - ARM_PULL * charge + strain
            }
            Loader::Reloading { .. } => {
                let wound = EaseFunction::SineInOut.sample_clamped(catapult.loader.loaded());
                let crank = 0.04 * (t * 16.0).sin() * (1.0 - wound);
                model::ARM_STOP.lerp(model::ARM_REST, wound) + crank
            }
        };
        let pull = ARM_STIFFNESS * (goal - catapult.arm_angle) - ARM_DAMPING * catapult.arm_speed;
        catapult.arm_speed += pull * dt;
        catapult.arm_angle += catapult.arm_speed * dt;

        // The boulder leaves just before the arm slams into the crossbar.
        if let Some((shot, at)) = catapult.pending
            && catapult.arm_angle >= model::ARM_STOP - LAUNCH_BEFORE_STOP
        {
            catapult.pending = None;
            let local = model::PIVOT + Quat::from_rotation_x(catapult.arm_angle) * model::BUCKET;
            let start = transform.transform_point(local * VOXEL);
            let lost = !on_island(&island, at);
            let to = Vec3::new(at.x, if lost { LOST_DEPTH } else { 0.0 }, at.y);
            let (duration, height) = flight(start.xz().distance(at));
            commands.spawn((
                Boulder {
                    from: start,
                    to,
                    height,
                    age: 0.0,
                    duration,
                    shot,
                    spin: Vec3::new(9.0, 3.0, 5.0),
                    lost,
                },
                Mesh3d(assets.boulder.clone()),
                MeshMaterial3d(assets.ember.clone()),
                Transform::from_translation(start),
                // Boulders fly burning at dusk, lighting up the ground beneath them.
                children![lamp(BOULDER_LUMENS, BOULDER_LIGHT_RANGE, start.x * 9.0)],
            ));
            commands.entity(catapult.loaded).insert(Visibility::Hidden);
            catapult.bucket_full = false;
        }
        if catapult.arm_angle > model::ARM_STOP {
            // Thud against the crossbar: the whole thing bucks.
            let speed = catapult.arm_speed;
            catapult.arm_angle = model::ARM_STOP;
            catapult.arm_speed = -speed.abs() * 0.3;
            if speed > 3.0 {
                if let Ok(mut squash) = squashes.get_mut(catapult.body) {
                    squash.pulse(0.22);
                }
                shake.add(0.18);
                particles.burst(
                    &mut commands,
                    transform.translation + Vec3::Y * 0.05,
                    Burst {
                        colors: &[palette::DIRT, palette::ROAD, palette::MEADOW_GRASS],
                        count: 10,
                        size: 0.045,
                        spread: 1.3,
                        lift: 0.9,
                        lifetime: 0.6,
                    },
                );
            }
        }
        if let Ok(mut arm) = arms.get_mut(catapult.arm) {
            arm.rotation = Quat::from_rotation_x(catapult.arm_angle);
        }

        // A new boulder pops into the bucket once it's wound back.
        let reloading = matches!(catapult.loader, Loader::Reloading { .. });
        if !reloading && !catapult.bucket_full && catapult.pending.is_none() {
            catapult.bucket_full = true;
            commands
                .entity(catapult.loaded)
                .insert(Tween::pop_in(Vec3::ONE, 0.0));
            if let Ok(mut squash) = squashes.get_mut(catapult.body) {
                squash.pulse(-0.1);
            }
        }
    }
}

/// Boulders arc to where they were aimed and land with a splat: every slime in the splash is
/// hit, the ground is dented, and the game stops for a heartbeat.
fn fly_boulders(
    mut commands: Commands,
    time: Res<Time>,
    mut boulders: Query<(Entity, &mut Boulder, &mut Transform)>,
    slimes: Query<(Entity, &Slime, &GlobalTransform)>,
    mut hits: MessageWriter<SlimeHit>,
    mut popups: MessageWriter<Popup>,
    mut shake: ResMut<Shake>,
    mut hit_stop: ResMut<HitStop>,
    mut particles: Particles,
) {
    for (entity, mut boulder, mut transform) in &mut boulders {
        boulder.age += time.delta_secs();
        let t = (boulder.age / boulder.duration).min(1.0);
        transform.translation = boulder_position(boulder.from, boulder.to, boulder.height, t);
        let spin = boulder.spin * time.delta_secs();
        transform.rotation =
            Quat::from_euler(EulerRot::XYZ, spin.x, spin.y, spin.z) * transform.rotation;
        if t < 1.0 {
            continue;
        }
        commands.entity(entity).despawn();
        if boulder.lost {
            continue;
        }
        let shot = boulder.shot;
        let center = boulder.to.xz();
        let in_splash = splash(
            center,
            shot.radius,
            slimes
                .iter()
                .map(|(entity, slime, global)| ((entity, slime), global.translation().xz())),
        );
        let popped = in_splash
            .iter()
            .filter(|(_, slime)| slime.health.hp <= shot.damage)
            .count();
        for (slime, _) in &in_splash {
            hits.write(SlimeHit {
                slime: *slime,
                damage: shot.damage,
                by: HitBy::Boulder,
            });
        }

        shake.add(IMPACT_SHAKE + if shot.full { FULL_CHARGE_SHAKE } else { 0.0 });
        if !in_splash.is_empty() || shot.full {
            hit_stop.add(HIT_STOP + HIT_STOP_PER_POP * popped.min(3) as f32);
        }
        let at = boulder.to;
        let strength = shot.radius / self::shot(1.0).radius;
        commands.spawn(flash(
            at + Vec3::Y * IMPACT_FLASH_HEIGHT,
            IMPACT_LUMENS * strength,
            IMPACT_FLASH_RANGE,
            IMPACT_FLASH_TIME,
        ));
        particles.burst(
            &mut commands,
            at + Vec3::Y * 0.05,
            Burst {
                colors: &[palette::DIRT, palette::ROAD, palette::MEADOW_GRASS],
                count: 16 + (shot.radius * 30.0) as usize,
                size: 0.055,
                spread: 1.2 + shot.radius * 4.0,
                lift: 1.6,
                lifetime: 0.8,
            },
        );
        particles.burst(
            &mut commands,
            at + Vec3::Y * 0.1,
            Burst {
                colors: &[palette::BOULDER, palette::PEBBLE, palette::STONE],
                count: 9,
                size: 0.06,
                spread: 1.4,
                lift: 2.8,
                lifetime: 0.9,
            },
        );
        // Clods of churned-up dirt where it hit, fading away.
        for k in 0..CRATER_CLODS {
            let angle = k as f32 * 2.4 + center.x * 5.0;
            let distance = shot.radius * 0.55 * (k as f32 / CRATER_CLODS as f32).sqrt();
            let offset = Vec3::new(angle.cos(), 0.0, angle.sin()) * distance;
            let side = 0.07 + 0.05 * ((k * 7) % 3) as f32;
            let size = Vec3::new(side, 0.02, side);
            let color = if k % 3 == 0 {
                palette::DIRT
            } else {
                palette::DIRT_DARK
            };
            commands.spawn((
                Crater { age: 0.0, size },
                particles.cube(color),
                Transform::from_translation(at.with_y(0.01) + offset)
                    .with_rotation(Quat::from_rotation_y(angle))
                    .with_scale(size),
                NotShadowCaster,
            ));
        }
        let text = match popped {
            0 => None,
            1 if shot.full => Some("Splat!".to_string()),
            1 => None,
            count => Some(format!("{count} slimes!")),
        };
        if let Some(text) = text {
            popups.write(Popup::new(at + Vec3::Y * 0.5, text, palette::FLAG));
        }
    }
}

fn fade_craters(
    mut commands: Commands,
    time: Res<Time>,
    mut craters: Query<(Entity, &mut Crater, &mut Transform)>,
) {
    for (entity, mut crater, mut transform) in &mut craters {
        crater.age += time.delta_secs();
        let life = crater.age / CRATER_TIME;
        if life >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let shrink = ((1.0 - life) / 0.3).min(1.0);
        transform.scale = crater.size * Vec3::new(shrink, 1.0, shrink);
    }
}

/// A dust puff and a bounce when a new catapult lands.
fn land_catapults(
    mut commands: Commands,
    landing: Query<(Entity, &Catapult, &Transform), (With<Landing>, Without<Tween>)>,
    mut squashes: Query<&mut Squash>,
    mut particles: Particles,
) {
    for (entity, catapult, transform) in &landing {
        commands.entity(entity).remove::<Landing>();
        if let Ok(mut squash) = squashes.get_mut(catapult.body) {
            squash.pulse(-0.3);
        }
        particles.burst(
            &mut commands,
            transform.translation + Vec3::Y * 0.05,
            Burst {
                colors: &[palette::DIRT, palette::ROAD, palette::MEADOW_GRASS],
                count: 18,
                size: 0.05,
                spread: 1.6,
                lift: 1.2,
                lifetime: 0.7,
            },
        );
    }
}

/// While the keeper is in the catapult: a ring of dots where the shot would land, as wide as
/// its splash, lighting up as it charges, and while charging, the arc the boulder would fly.
fn show_reticle(
    mut commands: Commands,
    time: Res<Time>,
    aim: Res<CatapultAim>,
    catapults: Query<&Transform, With<Catapult>>,
    mut dots: Query<(&AimDot, &mut Transform, &mut Visibility, &mut Mesh3d), Without<Catapult>>,
    mut pool: Local<Vec<Entity>>,
    mut particles: Particles,
) {
    if pool.is_empty() {
        for k in 0..RING_DOTS + ARC_DOTS {
            let dot = commands
                .spawn((
                    AimDot {
                        ring: k < RING_DOTS,
                    },
                    particles.marker(palette::ORDER_CATAPULT),
                    Transform::from_scale(Vec3::splat(DOT_SIZE)),
                    Visibility::Hidden,
                    NotShadowCaster,
                ))
                .id();
            pool.push(dot);
        }
    }
    let target = aim.target.filter(|_| aim.manned);
    let catapult = catapults.single().ok();
    let charge = aim.loader.charge();
    let charging = matches!(aim.loader, Loader::Charging { .. });
    let ready = !matches!(aim.loader, Loader::Reloading { .. });
    // The ring shows the splash a full charge would have, lit up as far as it's charged.
    let full = shot(1.0).radius;
    let now = shot(charge).radius;
    let lit = palette::FLAG;
    let dim = if ready {
        palette::ROUTE
    } else {
        palette::STONE
    };
    let (lit_mesh, dim_mesh) = (particles.cube(lit).0.0, particles.cube(dim).0.0);
    let t = time.elapsed_secs();
    let mut ring_index = 0;
    let mut arc_index = 0;
    for &entity in pool.iter() {
        let Ok((dot, mut transform, mut visibility, mut mesh)) = dots.get_mut(entity) else {
            continue;
        };
        let Some((target, catapult)) = target.zip(catapult) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let center = Vec3::new(target.x, 0.03, target.y);
        if dot.ring {
            let k = ring_index as f32 / RING_DOTS as f32;
            ring_index += 1;
            let angle = k * std::f32::consts::TAU + t * 0.8;
            let on = charging && k < charge;
            let radius = if charging { now.max(full * 0.4) } else { full };
            transform.translation = center + Vec3::new(angle.cos(), 0.0, angle.sin()) * radius;
            let pulse = if charge >= 1.0 {
                1.0 + 0.35 * (t * 14.0).sin().abs()
            } else {
                1.0
            };
            let size = if on {
                DOT_SIZE * 1.25 * pulse
            } else {
                DOT_SIZE
            };
            transform.scale = Vec3::new(size, size * 0.6, size);
            let want = if on { &lit_mesh } else { &dim_mesh };
            if mesh.0 != *want {
                mesh.0 = want.clone();
            }
            visibility.set_if_neq(Visibility::Inherited);
        } else {
            let k = (arc_index as f32 + 1.0) / (ARC_DOTS as f32 + 1.0);
            arc_index += 1;
            if !charging {
                visibility.set_if_neq(Visibility::Hidden);
                continue;
            }
            let local = model::PIVOT + Quat::from_rotation_x(model::ARM_STOP) * model::BUCKET;
            let start = catapult.transform_point(local * VOXEL);
            let (_, height) = flight(start.xz().distance(target));
            transform.translation = boulder_position(start, center, height, k);
            let size = DOT_SIZE * (0.5 + 0.5 * charge);
            transform.scale = Vec3::splat(size);
            let want = if k < charge { &lit_mesh } else { &dim_mesh };
            if mesh.0 != *want {
                mesh.0 = want.clone();
            }
            visibility.set_if_neq(Visibility::Inherited);
        }
    }
}

/// Clears the old run's catapult and anything it threw.
fn clear_catapults(
    mut commands: Commands,
    catapults: Query<Entity, Or<(With<Catapult>, With<Boulder>, With<Crater>)>>,
    mut aim: ResMut<CatapultAim>,
) {
    for entity in &catapults {
        commands.entity(entity).despawn();
    }
    *aim = CatapultAim::default();
}
