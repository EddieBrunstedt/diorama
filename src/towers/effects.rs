//! What towers' shots look like: muzzle flashes, tracers and sparks for the autocannon, a stream
//! of fire for the flamer, and for the mortar a thump, a shell trailing sparks, and a blast with
//! a flash of light, a shock ring, dirt and embers. Badly damaged towers smoke.

use super::{Shell, ShellLanded, Tower, TowerFired, TowerSystems, Turret};
use crate::grid::{Grid, Kind};
use crate::juice::{Burst, Fleeting, Look, Particles, Shake, Spray, Squash};
use crate::palette;
use crate::shapes::ring_mesh;
use crate::tuning::juice::*;
use crate::tuning::towers::{FLAMER_CONE_DEGREES, FLAMER_RANGE};
use bevy::{light::NotShadowCaster, platform::collections::HashMap, prelude::*};

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EffectAssets>().add_systems(
            Update,
            (
                show_shots,
                burn,
                trail_shells,
                show_blasts,
                smoke_damaged_towers,
            )
                .after(TowerSystems),
        );
    }
}

#[derive(Resource)]
struct EffectAssets {
    cube: Handle<Mesh>,
    ring: Handle<Mesh>,
    flash: Handle<StandardMaterial>,
    tracer: Handle<StandardMaterial>,
    blast_ring: Handle<StandardMaterial>,
}

impl FromWorld for EffectAssets {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        let cube = meshes.add(Cuboid::from_length(1.0));
        let ring = meshes.add(ring_mesh(0.8, 1.0, 40));
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let mut glow = |color: Color| {
            materials.add(StandardMaterial {
                base_color: (color.to_linear() * GLOW_BRIGHTNESS).with_alpha(1.0).into(),
                unlit: true,
                ..default()
            })
        };
        Self {
            cube,
            ring,
            flash: glow(palette::MUZZLE_FLASH),
            tracer: glow(palette::TRACER),
            blast_ring: glow(palette::BLAST_RING),
        }
    }
}

fn show_shots(
    mut commands: Commands,
    mut fired: MessageReader<TowerFired>,
    assets: Res<EffectAssets>,
    mut towers: Query<(&Children, &mut Squash), With<Tower>>,
    mut turrets: Query<&mut Turret>,
    mut particles: Particles,
) {
    for shot in fired.read() {
        let kick = match shot.kind {
            Kind::Autocannon => AUTOCANNON_KICK,
            _ => MORTAR_KICK,
        };
        if let Ok((children, mut squash)) = towers.get_mut(shot.tower) {
            for &child in children {
                if let Ok(mut turret) = turrets.get_mut(child) {
                    turret.kick = kick;
                }
            }
            if matches!(shot.kind, Kind::Mortar | Kind::ArtilleryCommand) {
                squash.pulse(MORTAR_THUMP);
            }
        }
        let along = shot.to - shot.from;
        let Some(direction) = along.try_normalize() else {
            continue;
        };
        match shot.kind {
            Kind::Autocannon => {
                // A flash at the muzzle, a streak to the target, sparks where it hits.
                let flash = Vec3::splat(0.22);
                commands.spawn((
                    Fleeting::new(MUZZLE_FLASH_SECONDS, Look::Shrink(flash)),
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(assets.flash.clone()),
                    Transform::from_translation(shot.from + direction * 0.08)
                        .looking_to(direction, Vec3::Y)
                        .with_scale(flash),
                    NotShadowCaster,
                ));
                let tracer = Vec3::new(TRACER_WIDTH, TRACER_WIDTH, along.length());
                commands.spawn((
                    Fleeting::new(TRACER_SECONDS, Look::Thin(tracer)),
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(assets.tracer.clone()),
                    Transform::from_translation(shot.from + along / 2.0)
                        .looking_to(direction, Vec3::Y)
                        .with_scale(tracer),
                    NotShadowCaster,
                ));
                particles.spray(
                    &mut commands,
                    shot.to,
                    Spray {
                        colors: &palette::SPARKS,
                        count: SPARK_COUNT,
                        size: 0.05,
                        direction: (-direction + Vec3::Y).normalize(),
                        cone: 0.9,
                        speed: (1.5, 3.5),
                        gravity: 1.0,
                        grow: 1.0,
                        lifetime: 0.2,
                        glow: true,
                    },
                );
            }
            Kind::Mortar | Kind::ArtilleryCommand => {
                particles.spray(
                    &mut commands,
                    shot.from,
                    Spray {
                        colors: &palette::SMOKE,
                        count: 8,
                        size: 0.16,
                        direction: Vec3::Y,
                        cone: 0.8,
                        speed: (0.4, 1.2),
                        gravity: -0.05,
                        grow: 2.5,
                        lifetime: 0.7,
                        glow: false,
                    },
                );
                particles.spray(
                    &mut commands,
                    shot.from,
                    Spray {
                        colors: &palette::FLAME,
                        count: 5,
                        size: 0.12,
                        direction: Vec3::Y,
                        cone: 0.5,
                        speed: (1.0, 2.0),
                        gravity: 0.0,
                        grow: 1.5,
                        lifetime: 0.12,
                        glow: true,
                    },
                );
            }
            Kind::Flamer | Kind::Wall => {}
        }
    }
}

/// Flamers that are burning pour fire through their cone.
fn burn(
    mut commands: Commands,
    time: Res<Time>,
    towers: Query<(Entity, &Tower, &Transform)>,
    mut particles: Particles,
    mut owed: Local<HashMap<Entity, f32>>,
) {
    let dt = time.delta_secs();
    owed.retain(|entity, _| towers.contains(*entity));
    for (entity, tower, transform) in &towers {
        if !tower.burning {
            owed.remove(&entity);
            continue;
        }
        let owed = owed.entry(entity).or_default();
        *owed += FLAME_RATE * dt;
        let count = *owed as usize;
        *owed -= count as f32;
        if count == 0 {
            continue;
        }
        let direction = Vec3::new(tower.aim.x, -0.05, tower.aim.y).normalize();
        let speed = FLAMER_RANGE / FLAME_LIFETIME;
        particles.spray(
            &mut commands,
            tower.muzzle(transform.translation),
            Spray {
                colors: &palette::FLAME,
                count,
                size: FLAME_SIZE,
                direction,
                cone: FLAMER_CONE_DEGREES.to_radians() / 2.0 * 0.8,
                speed: (speed * 0.7, speed * 1.1),
                gravity: -0.15,
                grow: 2.2,
                lifetime: FLAME_LIFETIME,
                glow: true,
            },
        );
    }
}

/// Shells trail sparks, a few a frame at most.
fn trail_shells(
    mut commands: Commands,
    time: Res<Time>,
    shells: Query<(&Shell, &Transform)>,
    mut particles: Particles,
) {
    const TRAIL_RATE: f32 = 50.0;
    let dt = time.delta_secs();
    for (shell, transform) in &shells {
        if shell.landed() {
            continue;
        }
        let count = ((shell.age * TRAIL_RATE).floor() - ((shell.age - dt) * TRAIL_RATE).floor())
            .clamp(0.0, 3.0) as usize;
        if count == 0 {
            continue;
        }
        particles.spray(
            &mut commands,
            transform.translation,
            Spray {
                colors: &palette::FLAME,
                count,
                size: 0.07,
                direction: Vec3::Y,
                cone: 1.0,
                speed: (0.0, 0.3),
                gravity: 0.0,
                grow: 0.5,
                lifetime: 0.25,
                glow: true,
            },
        );
    }
}

fn show_blasts(
    mut commands: Commands,
    mut landed: MessageReader<ShellLanded>,
    assets: Res<EffectAssets>,
    mut particles: Particles,
    mut shake: ResMut<Shake>,
) {
    for blast in landed.read() {
        let at = blast.at;
        commands.spawn((
            Fleeting::new(
                BLAST_FLASH_SECONDS,
                Look::Light {
                    intensity: BLAST_LIGHT,
                },
            ),
            PointLight {
                color: palette::BLAST_LIGHT,
                intensity: BLAST_LIGHT,
                range: blast.radius * 5.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(at + Vec3::Y * 0.8),
        ));
        commands.spawn((
            Fleeting::new(
                BLAST_RING_SECONDS,
                Look::Ring {
                    radius: blast.radius,
                },
            ),
            Mesh3d(assets.ring.clone()),
            MeshMaterial3d(assets.blast_ring.clone()),
            Transform::from_translation(at + Vec3::Y * 0.04).with_scale(Vec3::ZERO),
            NotShadowCaster,
        ));
        commands.spawn((
            Fleeting::new(BLAST_FLASH_SECONDS, Look::Shrink(Vec3::splat(0.9))),
            Mesh3d(assets.cube.clone()),
            MeshMaterial3d(assets.flash.clone()),
            Transform::from_translation(at + Vec3::Y * 0.3).with_scale(Vec3::splat(0.9)),
            NotShadowCaster,
        ));
        particles.burst(
            &mut commands,
            at + Vec3::Y * 0.1,
            0.3,
            Burst {
                colors: &palette::DUST,
                count: BLAST_DIRT_COUNT,
                size: DUST_SIZE,
                spread: DUST_SPREAD * 1.3,
                lift: RUBBLE_LIFT * 1.2,
                lifetime: DUST_LIFETIME,
            },
        );
        particles.spray(
            &mut commands,
            at + Vec3::Y * 0.2,
            Spray {
                colors: &palette::FLAME,
                count: BLAST_EMBER_COUNT,
                size: 0.1,
                direction: Vec3::Y,
                cone: 1.1,
                speed: (2.0, 5.0),
                gravity: 0.8,
                grow: 1.0,
                lifetime: 0.4,
                glow: true,
            },
        );
        shake.add_capped(BLAST_SHAKE, BLAST_SHAKE_MAX);
    }
}

/// Towers low on HP puff dark smoke now and then.
fn smoke_damaged_towers(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<Grid>,
    towers: Query<(&Tower, &Transform)>,
    mut particles: Particles,
    mut since: Local<f32>,
) {
    *since += time.delta_secs();
    if *since < TOWER_SMOKE_INTERVAL {
        return;
    }
    *since = 0.0;
    for (tower, transform) in &towers {
        let Some((hp, max)) = grid.structure(tower.cell).and_then(|s| s.health()) else {
            continue;
        };
        if hp as f32 >= max as f32 * WALL_BATTERED {
            continue;
        }
        particles.spray(
            &mut commands,
            transform.translation + Vec3::Y * 0.7,
            Spray {
                colors: &palette::SMOKE,
                count: 2,
                size: 0.14,
                direction: Vec3::Y,
                cone: 0.4,
                speed: (0.3, 0.8),
                gravity: -0.05,
                grow: 2.5,
                lifetime: 1.0,
                glow: false,
            },
        );
    }
}
