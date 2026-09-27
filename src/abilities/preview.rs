//! What a click would hit, on the ground, while aiming:
//!
//! - The sniper shot: a reticle that snaps to the enemy the shot would hit and follows it, over
//!   its head, with a laser sight from the Command Center's antenna. Cyan on a target; red at the cursor when
//!   there's nothing near enough to hit. A click on nothing flashes it.
//! - The mortar barrage: the circle its shells will fall in, under the cursor, in amber (red off
//!   the map, where it can't be called in).

use super::{
    Ability, AbilityDenied, AbilitySystems, Aiming, EnemyLookup, barrage_allowed, look_up,
    sniper_mark, sniper_muzzle,
};
use crate::flow::Class;
use crate::grid::Grid;
use crate::horde::{EnemyIndex, Health};
use crate::map::Landmarks;
use crate::palette;
use crate::pointer::Pointer;
use crate::shapes::{BoxMesh, ring_mesh};
use crate::tuning::abilities::BARRAGE_RADIUS;
use crate::tuning::juice::DENIED_FLASH_SECONDS;
use bevy::{light::NotShadowCaster, prelude::*};

/// How high the barrage circle floats over the ground. The reticle floats over its target's
/// head (a swarmer's is this high, a brute's this many times its size), so the horde doesn't
/// hide it, and the laser sight meets it there.
const LIFT: f32 = 0.04;
const RETICLE_HEIGHT: f32 = 0.45;
const RETICLE_BRUTE_HEIGHT: f32 = 1.3;
const LASER_WIDTH: f32 = 0.05;
/// The reticle's size around its target (past the target's edge), and how fast it turns
/// (radians per second).
const RETICLE_MARGIN: f32 = 0.25;
const RETICLE_SPIN: f32 = 1.5;
/// The reticle with nothing to hit, and how much it swells when a click finds nothing.
const RETICLE_IDLE: f32 = 0.45;
const RETICLE_DENIED_POP: f32 = 0.5;
/// The barrage circle's line width, and how brightly the reticle and circle glow.
const LINE: f32 = 0.08;
const GLOW: f32 = 1.6;

pub struct PreviewPlugin;

impl Plugin for PreviewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_previews)
            .add_systems(Update, show_previews.after(AbilitySystems));
    }
}

#[derive(Component)]
struct Reticle {
    /// Seconds left of flashing red after a click on nothing.
    denied: f32,
}

#[derive(Component)]
struct Laser;

#[derive(Component)]
struct BarrageCircle;

#[derive(Component)]
struct BarrageFill;

/// Materials for the previews: on target (or allowed), and with nothing to hit (or not allowed).
#[derive(Resource)]
struct PreviewMaterials {
    reticle: (Handle<StandardMaterial>, Handle<StandardMaterial>),
    laser: (Handle<StandardMaterial>, Handle<StandardMaterial>),
    circle: (Handle<StandardMaterial>, Handle<StandardMaterial>),
    fill: (Handle<StandardMaterial>, Handle<StandardMaterial>),
}

/// A ring with four ticks pointing in at it, 1 across its middle, lying on the ground.
fn reticle_mesh() -> Mesh {
    let mut mesh = BoxMesh::default();
    mesh.ring(0.86, 1.0, 0.0, 40, Color::WHITE);
    for (x, z) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        let direction = Vec3::new(x, 0.0, z);
        let side = Vec3::new(z, 0.0, x).abs() * 0.05;
        let (a, b) = (direction * 0.62, direction * 1.3);
        mesh.cuboid(
            a.min(b) - side,
            a.max(b) + side + Vec3::Y * 0.02,
            Color::WHITE,
        );
    }
    mesh.build()
}

fn spawn_previews(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut glow = |color: Color| {
        materials.add(StandardMaterial {
            base_color: (color.to_linear() * GLOW).with_alpha(1.0).into(),
            unlit: true,
            ..default()
        })
    };
    let reticle = (glow(palette::AIM), glow(palette::AIM_NONE));
    let circle = (glow(palette::BARRAGE_AIM), glow(palette::AIM_NONE));
    let mut see_through = |color: Color| {
        materials.add(StandardMaterial {
            base_color: color,
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        })
    };
    let laser = (
        see_through(palette::LASER),
        see_through(palette::AIM_NONE.with_alpha(palette::LASER.alpha())),
    );
    let fill = (
        see_through(palette::BARRAGE_AIM.with_alpha(palette::AIM_FILL_ALPHA)),
        see_through(palette::AIM_NONE.with_alpha(palette::AIM_FILL_ALPHA)),
    );
    commands.spawn((
        Reticle { denied: 0.0 },
        Mesh3d(meshes.add(reticle_mesh())),
        MeshMaterial3d(reticle.0.clone()),
        Transform::default(),
        Visibility::Hidden,
        NotShadowCaster,
    ));
    commands.spawn((
        Laser,
        Mesh3d(meshes.add(Cuboid::from_length(1.0))),
        MeshMaterial3d(laser.0.clone()),
        Transform::default(),
        Visibility::Hidden,
        NotShadowCaster,
    ));
    commands
        .spawn((
            BarrageCircle,
            Mesh3d(meshes.add(ring_mesh(BARRAGE_RADIUS - LINE, BARRAGE_RADIUS, 96))),
            MeshMaterial3d(circle.0.clone()),
            Transform::default(),
            Visibility::Hidden,
            NotShadowCaster,
        ))
        .with_child((
            BarrageFill,
            Mesh3d(meshes.add(ring_mesh(0.0, BARRAGE_RADIUS - LINE, 64))),
            MeshMaterial3d(fill.0.clone()),
            Transform::default(),
            NotShadowCaster,
        ));
    commands.insert_resource(PreviewMaterials {
        reticle,
        laser,
        circle,
        fill,
    });
}

/// The material for `on` (a target, or allowed) or not.
fn pick(
    pair: &(Handle<StandardMaterial>, Handle<StandardMaterial>),
    on: bool,
) -> Handle<StandardMaterial> {
    if on { pair.0.clone() } else { pair.1.clone() }
}

fn set_material(material: &mut MeshMaterial3d<StandardMaterial>, handle: Handle<StandardMaterial>) {
    if material.0 != handle {
        material.0 = handle;
    }
}

fn shown(visible: bool) -> Visibility {
    if visible {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    }
}

fn show_previews(
    time: Res<Time<Real>>,
    aiming: Res<Aiming>,
    pointer: Res<Pointer>,
    grid: Res<Grid>,
    landmarks: Res<Landmarks>,
    index: EnemyIndex,
    enemies: EnemyLookup,
    materials: Res<PreviewMaterials>,
    mut denied: MessageReader<AbilityDenied>,
    mut reticle: Single<
        (
            &mut Reticle,
            &mut Transform,
            &mut Visibility,
            &mut MeshMaterial3d<StandardMaterial>,
        ),
        (Without<Laser>, Without<BarrageCircle>, Without<Health>),
    >,
    mut laser: Single<
        (
            &mut Transform,
            &mut Visibility,
            &mut MeshMaterial3d<StandardMaterial>,
        ),
        (
            With<Laser>,
            Without<Reticle>,
            Without<BarrageCircle>,
            Without<Health>,
        ),
    >,
    mut circle: Single<
        (
            &mut Transform,
            &mut Visibility,
            &mut MeshMaterial3d<StandardMaterial>,
        ),
        (
            With<BarrageCircle>,
            Without<Reticle>,
            Without<Laser>,
            Without<Health>,
        ),
    >,
    mut fill: Single<
        &mut MeshMaterial3d<StandardMaterial>,
        (
            With<BarrageFill>,
            Without<Reticle>,
            Without<Laser>,
            Without<BarrageCircle>,
        ),
    >,
) {
    let dt = time.delta_secs();
    let cursor = pointer.ground.map(|ground| ground.xz());
    let (reticle, reticle_transform, reticle_visibility, reticle_material) = &mut *reticle;
    if denied.read().any(|denied| denied.0 == Ability::Sniper) {
        reticle.denied = DENIED_FLASH_SECONDS;
    }
    reticle.denied = (reticle.denied - dt).max(0.0);

    // The sniper: the reticle on its target, and the laser sight to it.
    let sniper = (aiming.0 == Some(Ability::Sniper))
        .then_some(cursor)
        .flatten();
    let target =
        sniper.and_then(|cursor| sniper_mark(&index, cursor, |entity| look_up(&enemies, entity)));
    reticle_visibility.set_if_neq(shown(sniper.is_some()));
    let (laser_transform, laser_visibility, laser_material) = &mut *laser;
    laser_visibility.set_if_neq(shown(target.is_some()));
    if let Some(cursor) = sniper {
        let on_target = target.is_some() && reticle.denied <= 0.0;
        set_material(reticle_material, pick(&materials.reticle, on_target));
        let (at, size, height) = target.map_or((cursor, RETICLE_IDLE, LIFT), |mark| {
            let height = match mark.id.1 {
                Class::Swarmer => RETICLE_HEIGHT,
                Class::Brute => mark.radius * RETICLE_BRUTE_HEIGHT,
            };
            (mark.at, mark.radius + RETICLE_MARGIN, height)
        });
        let pop = 1.0 + RETICLE_DENIED_POP * reticle.denied / DENIED_FLASH_SECONDS;
        reticle_transform.translation = Vec3::new(at.x, height, at.y);
        reticle_transform.scale = Vec3::new(size * pop, 1.0, size * pop);
        reticle_transform.rotate_y(RETICLE_SPIN * dt);
        if target.is_some() {
            let from = sniper_muzzle(&landmarks);
            let to = Vec3::new(at.x, height, at.y);
            let along = to - from;
            set_material(laser_material, pick(&materials.laser, true));
            **laser_transform = Transform::from_translation(from + along / 2.0)
                .looking_to(along.normalize_or(Vec3::NEG_Z), Vec3::Y)
                .with_scale(Vec3::new(LASER_WIDTH, LASER_WIDTH, along.length()));
        }
    }

    // The barrage: the circle its shells will fall in.
    let barrage = (aiming.0 == Some(Ability::Barrage))
        .then_some(cursor)
        .flatten();
    let (circle_transform, circle_visibility, circle_material) = &mut *circle;
    circle_visibility.set_if_neq(shown(barrage.is_some()));
    if let Some(at) = barrage {
        let allowed = barrage_allowed(&grid, at);
        set_material(circle_material, pick(&materials.circle, allowed));
        set_material(&mut fill, pick(&materials.fill, allowed));
        circle_transform.translation = Vec3::new(at.x, LIFT, at.y);
    }
}
