//! The mortar barrage coming down. When one is called in, the nearest Artillery Command swings its
//! rack toward the target and launches its shells one after another, each on a high arc to its own
//! spot in the circle (see [`barrage_pattern`]). The shells are the towers' own
//! ([`Shell`]): they land, blast and hurt like a mortar's.
//!
//! Until the last one lands, the circle stays marked on the ground in red, filling in as the
//! first landing nears, and a marker blinks where each shell is about to hit. Each landing shakes
//! the camera, capped higher than a mortar's, so a barrage rumbles.

use super::targeting::{barrage_pattern, landing_times};
use super::{AbilitySystems, BarrageCalled};
use crate::grid::Kind;
use crate::juice::Shake;
use crate::palette;
use crate::rng::{Rng, RunSeed};
use crate::shapes::{ShapeMaterial, ring_mesh};
use crate::towers::targeting::turn_toward;
use crate::towers::{Blast, Shell, Tower, TowerAssets, TowerFired};
use crate::tuning::abilities::*;
use crate::tuning::juice::*;
use crate::waves::Phase;
use bevy::{light::NotShadowCaster, prelude::*};

/// How high the zone and the markers float over the ground.
const LIFT: f32 = 0.035;
/// A marker's size, and the zone's line width.
const MARKER_RADIUS: f32 = 0.32;
const ZONE_LINE: f32 = 0.09;

pub struct BarragePlugin;

impl Plugin for BarragePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BarrageAssets>()
            .add_systems(Startup, seed_barrages)
            .add_systems(OnExit(Phase::GameOver), clear_barrages)
            .add_systems(
                Update,
                (call_in, run_barrages).chain().after(AbilitySystems),
            );
    }
}

#[derive(Resource)]
struct BarrageRng(Rng);

fn seed_barrages(mut commands: Commands, seed: Res<RunSeed>) {
    commands.insert_resource(BarrageRng(Rng::new(seed.0 ^ 0xBA22)));
}

#[derive(Resource)]
struct BarrageAssets {
    ring: Handle<Mesh>,
    fill: Handle<Mesh>,
    marker: Handle<Mesh>,
    glow: Handle<StandardMaterial>,
    fill_material: Handle<StandardMaterial>,
}

impl FromWorld for BarrageAssets {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        let ring = meshes.add(ring_mesh(BARRAGE_RADIUS - ZONE_LINE, BARRAGE_RADIUS, 96));
        let fill = meshes.add(ring_mesh(0.0, 1.0, 64));
        let marker = meshes.add(ring_mesh(MARKER_RADIUS * 0.55, MARKER_RADIUS, 24));
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let glow = materials.add(StandardMaterial {
            base_color: (palette::BARRAGE_ZONE.to_linear() * 1.6)
                .with_alpha(1.0)
                .into(),
            unlit: true,
            ..default()
        });
        let fill_material = materials.add(StandardMaterial {
            base_color: palette::BARRAGE_ZONE.with_alpha(palette::AIM_FILL_ALPHA),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        });
        Self {
            ring,
            fill,
            marker,
            glow,
            fill_material,
        }
    }
}

/// A barrage coming down around its entity's position.
#[derive(Component)]
struct Barrage {
    /// Seconds since it was called in.
    age: f32,
    center: Vec2,
    /// The Artillery Command launching it, if one stands.
    battery: Option<Entity>,
    shells: Vec<Planned>,
    /// The fill that grows as the first landing nears.
    fill: Entity,
}

/// One of a barrage's shells: where and when (seconds after the call) it lands, and its marker.
struct Planned {
    at: Vec2,
    lands: f32,
    launched: bool,
    landed: bool,
    marker: Entity,
}

fn call_in(
    mut commands: Commands,
    mut called: MessageReader<BarrageCalled>,
    assets: Res<BarrageAssets>,
    towers: Query<(Entity, &Tower, &Transform)>,
    mut rng: ResMut<BarrageRng>,
) {
    for call in called.read() {
        let center = call.center;
        // The nearest Artillery Command fires it.
        let battery = towers
            .iter()
            .filter(|(_, tower, _)| tower.kind == Kind::ArtilleryCommand)
            .min_by(|(_, _, a), (_, _, b)| {
                let distance = |t: &Transform| t.translation.xz().distance_squared(center);
                distance(a).total_cmp(&distance(b))
            })
            .map(|(entity, ..)| entity);
        let spots = barrage_pattern(
            center,
            BARRAGE_RADIUS,
            BARRAGE_SHELLS,
            BARRAGE_JITTER,
            &mut rng.0,
        );
        let times = landing_times(spots.len(), BARRAGE_FIRST_LANDS, BARRAGE_SPREAD_SECONDS);
        let ground = Vec3::new(center.x, LIFT, center.y);
        let mut zone = commands.spawn((
            Mesh3d(assets.ring.clone()),
            MeshMaterial3d(assets.glow.clone()),
            Transform::from_translation(ground),
            Visibility::Inherited,
            NotShadowCaster,
        ));
        let mut fill = Entity::PLACEHOLDER;
        let mut shells = Vec::new();
        zone.with_children(|zone| {
            fill = zone
                .spawn((
                    Mesh3d(assets.fill.clone()),
                    MeshMaterial3d(assets.fill_material.clone()),
                    Transform::from_scale(Vec3::ZERO),
                    NotShadowCaster,
                ))
                .id();
            for (&at, &lands) in spots.iter().zip(&times) {
                let offset = at - center;
                let marker = zone
                    .spawn((
                        Mesh3d(assets.marker.clone()),
                        MeshMaterial3d(assets.glow.clone()),
                        Transform::from_xyz(offset.x, 0.005, offset.y),
                        Visibility::Hidden,
                        NotShadowCaster,
                    ))
                    .id();
                shells.push(Planned {
                    at,
                    lands,
                    launched: false,
                    landed: false,
                    marker,
                });
            }
        });
        zone.insert(Barrage {
            age: 0.0,
            center,
            battery,
            shells,
            fill,
        });
    }
}

fn run_barrages(
    mut commands: Commands,
    time: Res<Time>,
    tower_assets: Res<TowerAssets>,
    material: Res<ShapeMaterial>,
    mut barrages: Query<(Entity, &mut Barrage)>,
    mut towers: Query<(&mut Tower, &Transform)>,
    mut parts: Query<(&mut Transform, &mut Visibility), Without<Tower>>,
    mut fired: MessageWriter<TowerFired>,
    mut shake: ResMut<Shake>,
) {
    let dt = time.delta_secs();
    for (entity, mut barrage) in &mut barrages {
        barrage.age += dt;
        let age = barrage.age;
        let center = barrage.center;
        // The rack swings toward the target, and each shell launches in time to land when planned.
        let battery = barrage.battery.and_then(|battery| {
            let (mut tower, transform) = towers.get_mut(battery).ok()?;
            let toward = center - transform.translation.xz();
            tower.aim = turn_toward(tower.aim, toward, BARRAGE_TURN_RATE, dt);
            Some((battery, tower.muzzle(transform.translation)))
        });
        let mut done = true;
        for shell in &mut barrage.shells {
            if !shell.launched && age >= shell.lands - BARRAGE_FLIGHT_SECONDS {
                shell.launched = true;
                // With its Artillery Command gone, the shells still come (from off the map).
                let from =
                    battery.map_or(Vec3::new(center.x, 30.0, center.y), |(_, muzzle)| muzzle);
                let to = Vec3::new(shell.at.x, 0.0, shell.at.y);
                // Launched this frame, so it lands on time even if the frame ran long.
                let flight = (shell.lands - age).max(0.05);
                let blast = Blast {
                    damage: BARRAGE_DAMAGE,
                    radius: BARRAGE_BLAST_RADIUS,
                    edge: BARRAGE_EDGE_DAMAGE,
                };
                commands.spawn((
                    Mesh3d(tower_assets.shell()),
                    MeshMaterial3d(material.0.clone()),
                    Transform::from_translation(from),
                    Shell::new(from, to, flight, BARRAGE_ARC, blast),
                ));
                if let Some((tower, _)) = battery {
                    fired.write(TowerFired {
                        tower,
                        kind: Kind::ArtilleryCommand,
                        from,
                        to,
                    });
                }
            }
            if !shell.landed && age >= shell.lands {
                shell.landed = true;
                shake.add_capped(BARRAGE_SHAKE, BARRAGE_SHAKE_MAX);
            }
            done &= shell.landed;
            // The marker blinks, faster and bigger as the shell comes down.
            if let Ok((mut transform, mut visibility)) = parts.get_mut(shell.marker) {
                let left = shell.lands - age;
                let shown = !shell.landed && left <= BARRAGE_MARKER_SECONDS;
                let blink = (left * 18.0).sin() > -0.3;
                visibility.set_if_neq(if shown && blink {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
                let close = 1.0 - (left / BARRAGE_MARKER_SECONDS).clamp(0.0, 1.0);
                transform.scale = Vec3::splat(0.7 + 0.8 * close);
            }
        }
        // The fill grows toward the edge as the first landing nears.
        if let Ok((mut transform, _)) = parts.get_mut(barrage.fill) {
            let grow = (age / BARRAGE_FIRST_LANDS).clamp(0.0, 1.0);
            let radius = BARRAGE_RADIUS * EaseFunction::QuadraticOut.sample_clamped(grow);
            transform.scale = Vec3::new(radius, 1.0, radius);
        }
        if done {
            commands.entity(entity).despawn();
        }
    }
}

/// A new run: nothing is coming down anymore.
fn clear_barrages(mut commands: Commands, barrages: Query<Entity, With<Barrage>>) {
    for entity in &barrages {
        commands.entity(entity).despawn();
    }
}
