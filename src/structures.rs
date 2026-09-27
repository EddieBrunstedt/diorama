//! Structures as the player sees them. They follow the grid through [`StructureChanged`]:
//! a new structure drops in, slams down (squash, dust, a little shake), and a removed one pops
//! up and shrinks away in a spray of rubble.

use crate::build::{BuildDenied, BuildSystems, StructureChanged};
use crate::command_center;
use crate::grid::{Structure, cell_center};
use crate::juice::{Burst, Particles, Shake, Squash, Tween, TweenDone, TweenTarget, Wobble};
use crate::palette;
use crate::shapes::{BoxMesh, ShapeMaterial, shade};
use crate::tuning::juice::*;
use bevy::{platform::collections::HashMap, prelude::*};

/// A wall's height, and its width (less than a cell, so neighboring walls show a seam).
const WALL_HEIGHT: f32 = 0.9;
const WALL_WIDTH: f32 = 0.92;
/// The hazard band's bottom and top, measured down from the top of the wall.
const HAZARD_BAND: (f32, f32) = (0.2, 0.08);

/// How tall a structure stands, from the ground it's on to its top.
pub fn structure_height(structure: Structure) -> f32 {
    match structure {
        Structure::Wall => WALL_HEIGHT,
        Structure::CommandCenter => command_center::HEIGHT,
    }
}

pub struct StructuresPlugin;

impl Plugin for StructuresPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShapeMaterial>()
            .init_resource::<StructureAssets>()
            .init_resource::<StructureViews>()
            .add_systems(Update, (follow_changes, wobble_denied).after(BuildSystems))
            .add_observer(slam);
    }
}

#[derive(Resource)]
struct StructureAssets {
    wall: Handle<Mesh>,
}

impl FromWorld for StructureAssets {
    fn from_world(world: &mut World) -> Self {
        // Concrete, with a band of hazard yellow just under the top.
        let mut mesh = BoxMesh::default();
        let half = WALL_WIDTH / 2.0;
        let (band_low, band_high) = (WALL_HEIGHT - HAZARD_BAND.0, WALL_HEIGHT - HAZARD_BAND.1);
        let concrete = palette::CONCRETE;
        mesh.cuboid_graded(
            Vec3::new(-half, 0.0, -half),
            Vec3::new(half, band_low, half),
            shade(concrete, 0.85),
            shade(concrete, 0.45),
        );
        mesh.cuboid_graded(
            Vec3::new(-half, band_low, -half),
            Vec3::new(half, band_high, half),
            palette::HAZARD,
            palette::HAZARD,
        );
        mesh.cuboid_graded(
            Vec3::new(-half, band_high, -half),
            Vec3::new(half, WALL_HEIGHT, half),
            concrete,
            concrete,
        );
        Self {
            wall: world.resource_mut::<Assets<Mesh>>().add(mesh.build()),
        }
    }
}

/// The entity showing each cell's structure.
#[derive(Resource, Default)]
struct StructureViews(HashMap<IVec2, Entity>);

/// A structure standing on (or dropping onto) a cell. Removed once it's on its way out.
#[derive(Component)]
struct StructureView;

fn follow_changes(
    mut commands: Commands,
    mut changes: MessageReader<StructureChanged>,
    mut views: ResMut<StructureViews>,
    assets: Res<StructureAssets>,
    material: Res<ShapeMaterial>,
    transforms: Query<(&Transform, Has<Tween>), With<StructureView>>,
    mut particles: Particles,
) {
    let changes: Vec<_> = changes.read().map(|changed| changed.0).collect();
    let arriving = changes
        .iter()
        .filter(|change| change.after.is_some())
        .count();
    let stagger = LINE_STAGGER.min(MAX_RIPPLE_SECONDS / arriving.max(1) as f32);
    let mut landing = 0;
    for change in changes {
        let center = cell_center(change.cell);
        if change.before.is_some()
            && let Some(entity) = views.0.remove(&change.cell)
        {
            match transforms.get(entity) {
                Ok((transform, false)) => {
                    shrink_away(&mut commands, entity, transform.scale);
                    particles.burst(
                        &mut commands,
                        center + Vec3::Y * WALL_HEIGHT * 0.6,
                        0.3,
                        Burst {
                            colors: &palette::RUBBLE,
                            count: RUBBLE_COUNT,
                            size: RUBBLE_SIZE,
                            spread: RUBBLE_SPREAD,
                            lift: RUBBLE_LIFT,
                            lifetime: RUBBLE_LIFETIME,
                        },
                    );
                }
                // Still dropping (or only just spawned): it never landed, so it just vanishes,
                // the same instant the drag lets go of it.
                _ => commands.entity(entity).despawn(),
            }
        }
        if let Some(structure) = change.after {
            let mesh = match structure {
                Structure::Wall => assets.wall.clone(),
                // Never built or removed: it stands from the start (see `command_center`).
                Structure::CommandCenter => continue,
            };
            let delay = stagger * landing as f32;
            landing += 1;
            let drop = Tween::new(
                TweenTarget::Translation {
                    from: center + Vec3::Y * DROP_HEIGHT,
                    to: center,
                },
                EaseFunction::QuadraticIn,
                delay,
                DROP_SECONDS,
            );
            let entity = commands
                .spawn((
                    StructureView,
                    Mesh3d(mesh),
                    MeshMaterial3d(material.0.clone()),
                    drop.start_transform(),
                    // Hidden until its drop starts.
                    Visibility::Hidden,
                    drop,
                    Squash::new(Vec3::ONE),
                ))
                .id();
            views.0.insert(change.cell, entity);
        }
    }
}

fn shrink_away(commands: &mut Commands, entity: Entity, from: Vec3) {
    commands
        .entity(entity)
        .remove::<(StructureView, Squash, Wobble, Tween)>()
        .insert(
            Tween::new(
                TweenTarget::Scale {
                    from,
                    to: Vec3::splat(0.01),
                },
                EaseFunction::BackIn,
                0.0,
                REMOVE_SECONDS,
            )
            .then_despawn(),
        );
}

/// A structure hits the ground: squash, dust and a bump of the camera.
fn slam(
    done: On<TweenDone>,
    mut commands: Commands,
    mut views: Query<(&Transform, &mut Squash), With<StructureView>>,
    mut particles: Particles,
    mut shake: ResMut<Shake>,
) {
    let Ok((transform, mut squash)) = views.get_mut(done.entity) else {
        return;
    };
    squash.pulse(LAND_SQUASH);
    shake.add_capped(LAND_SHAKE, LAND_SHAKE_MAX);
    particles.burst(
        &mut commands,
        transform.translation + Vec3::Y * 0.05,
        WALL_WIDTH / 2.0,
        Burst {
            colors: &palette::DUST,
            count: DUST_COUNT,
            size: DUST_SIZE,
            spread: DUST_SPREAD,
            lift: DUST_LIFT,
            lifetime: DUST_LIFETIME,
        },
    );
}

/// A click that can't build shakes the structure in the way, if there is one.
fn wobble_denied(
    mut commands: Commands,
    mut denials: MessageReader<BuildDenied>,
    views: Res<StructureViews>,
    walls: Query<(&Transform, Option<&Wobble>), With<StructureView>>,
) {
    for denial in denials.read() {
        let Some(&entity) = views.0.get(&denial.cell) else {
            continue;
        };
        if let Ok((transform, existing)) = walls.get(entity) {
            let wobble = Wobble::new(existing, transform, Vec3::Z, DENIED_WOBBLE);
            commands.entity(entity).insert(wobble);
        }
    }
}
