//! Structures as the player sees them. They follow the grid through [`StructureChanged`]:
//! a new structure drops in, slams down (squash, dust, a little shake), and a removed one pops
//! up and shrinks away in a spray of rubble.
//!
//! Walls show their wear: each hit shudders the wall and chips off concrete flecks, it looks
//! cracked below two thirds of its HP and battered (chunks gone, rebar showing) below a third,
//! and when it breaks it sinks and crumbles in a burst of rubble and dust.

use crate::build::{BuildDenied, BuildSystems, StructureChanged};
use crate::command_center;
use crate::flow::Class;
use crate::grid::{Structure, cell_center};
use crate::juice::{Burst, Particles, Shake, Squash, Tween, TweenDone, TweenTarget, Wobble};
use crate::palette;
use crate::shapes::{BoxMesh, ShapeMaterial, shade};
use crate::tuning::juice::*;
use crate::walls::{WallBroken, WallDamaged, WallSystems};
use bevy::{platform::collections::HashMap, prelude::*};

/// A wall's height, and its width (less than a cell, so neighboring walls show a seam).
const WALL_HEIGHT: f32 = 0.9;
const WALL_WIDTH: f32 = 0.92;
/// The hazard band's bottom and top, measured down from the top of the wall.
const HAZARD_BAND: (f32, f32) = (0.2, 0.08);

/// How tall a structure stands, from the ground it's on to its top.
pub fn structure_height(structure: Structure) -> f32 {
    match structure {
        Structure::Wall { .. } => WALL_HEIGHT,
        Structure::CommandCenter => command_center::HEIGHT,
    }
}

pub struct StructuresPlugin;

impl Plugin for StructuresPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShapeMaterial>()
            .init_resource::<StructureAssets>()
            .init_resource::<StructureViews>()
            .add_systems(
                Update,
                (
                    (follow_changes, wobble_denied).after(BuildSystems),
                    show_wear.after(WallSystems),
                ),
            )
            .add_observer(slam);
    }
}

/// How worn a wall looks.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wear {
    Intact,
    /// Cracks across its faces and top.
    Cracked,
    /// Chunks knocked off the top, rebar showing, deep cracks.
    Battered,
}

impl Wear {
    /// How a wall with `hp` out of `max` left looks.
    pub fn of(hp: u32, max: u32) -> Self {
        let left = hp as f32 / max.max(1) as f32;
        if left < WALL_BATTERED {
            Wear::Battered
        } else if left < WALL_CRACKED {
            Wear::Cracked
        } else {
            Wear::Intact
        }
    }

    fn of_structure(structure: Structure) -> Self {
        structure
            .health()
            .map_or(Wear::Intact, |(hp, max)| Wear::of(hp, max))
    }
}

#[derive(Resource)]
struct StructureAssets {
    /// A wall mesh per [`Wear`].
    walls: [Handle<Mesh>; 3],
}

impl StructureAssets {
    fn wall(&self, wear: Wear) -> Handle<Mesh> {
        self.walls[wear as usize].clone()
    }
}

impl FromWorld for StructureAssets {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        Self {
            walls: [Wear::Intact, Wear::Cracked, Wear::Battered]
                .map(|wear| meshes.add(wall_mesh(wear))),
        }
    }
}

/// A block of wall covering `min` to `max` on the ground (around the wall's middle), `top` tall:
/// concrete, with the hazard band where it reaches that high.
fn wall_block(mesh: &mut BoxMesh, min: Vec2, max: Vec2, top: f32) {
    let (band_low, band_high) = (WALL_HEIGHT - HAZARD_BAND.0, WALL_HEIGHT - HAZARD_BAND.1);
    let concrete = palette::CONCRETE;
    let layer = |mesh: &mut BoxMesh, low: f32, high: f32, top: Color, bottom: Color| {
        if high > low {
            mesh.cuboid_graded(
                Vec3::new(min.x, low, min.y),
                Vec3::new(max.x, high, max.y),
                top,
                bottom,
            );
        }
    };
    layer(
        mesh,
        0.0,
        top.min(band_low),
        shade(concrete, 0.85),
        shade(concrete, 0.45),
    );
    layer(
        mesh,
        band_low,
        top.min(band_high),
        palette::HAZARD,
        palette::HAZARD,
    );
    layer(mesh, band_high, top, concrete, concrete);
}

/// A crack along `points`, lying on a face that looks along `normal`: little dark squares
/// stepped along each segment.
fn crack(mesh: &mut BoxMesh, normal: Vec3, points: &[Vec3]) {
    const STEP: f32 = 0.025;
    const WIDTH: f32 = 0.017;
    const DEPTH: f32 = 0.006;
    let extent = Vec3::splat(WIDTH) * (Vec3::ONE - normal.abs()) + normal.abs() * DEPTH;
    for pair in points.windows(2) {
        let steps = (pair[0].distance(pair[1]) / STEP).ceil().max(1.0) as usize;
        for i in 0..=steps {
            let at = pair[0].lerp(pair[1], i as f32 / steps as f32);
            mesh.cuboid(at - extent, at + extent, palette::CRACK);
        }
    }
}

/// A wall: concrete, with a band of hazard yellow just under the top, as worn as `wear`.
fn wall_mesh(wear: Wear) -> Mesh {
    let mut mesh = BoxMesh::default();
    let half = WALL_WIDTH / 2.0;
    let top = WALL_HEIGHT;
    // Points on the front face (toward the camera), the top, and the right side.
    let front = |x: f32, y: f32| Vec3::new(x, y, half);
    let on_top = |x: f32, z: f32| Vec3::new(x, top, z);
    let side = |z: f32, y: f32| Vec3::new(half, y, z);
    match wear {
        Wear::Intact => wall_block(&mut mesh, Vec2::splat(-half), Vec2::splat(half), top),
        Wear::Cracked => {
            wall_block(&mut mesh, Vec2::splat(-half), Vec2::splat(half), top);
            let points = [(-0.12, top), (-0.04, 0.72), (-0.15, 0.5), (-0.07, 0.3)];
            crack(&mut mesh, Vec3::Z, &points.map(|(x, y)| front(x, y)));
            crack(&mut mesh, Vec3::Z, &[front(-0.04, 0.72), front(0.16, 0.6)]);
            let points = [(-0.12, half), (-0.02, 0.15), (0.12, -0.1), (0.08, -half)];
            crack(&mut mesh, Vec3::Y, &points.map(|(x, z)| on_top(x, z)));
            let points = [(0.2, top), (0.05, 0.62), (0.2, 0.4)];
            crack(&mut mesh, Vec3::X, &points.map(|(z, y)| side(z, y)));
        }
        Wear::Battered => {
            // A solid base, and the top in quarters: one knocked off, one knocked down.
            let base = 0.58;
            wall_block(&mut mesh, Vec2::splat(-half), Vec2::splat(half), base);
            let quarters = [
                (Vec2::new(-half, 0.0), Vec2::new(0.0, half), base + 0.05),
                (Vec2::new(0.0, 0.0), Vec2::new(half, half), top),
                (Vec2::new(-half, -half), Vec2::new(0.0, 0.0), 0.68),
                (Vec2::new(0.0, -half), Vec2::new(half, 0.0), top),
            ];
            for (min, max, height) in quarters {
                wall_block(&mut mesh, min, max, height);
            }
            // The broken quarter shows rough concrete and bent rebar.
            mesh.cuboid(
                Vec3::new(-half + 0.04, base, 0.04),
                Vec3::new(-0.04, base + 0.07, half - 0.04),
                palette::BROKEN_CONCRETE,
            );
            for (x, z, height, bend) in [(-0.3, 0.2, 0.26, 0.1), (-0.14, 0.33, 0.2, -0.08)] {
                let (low, high) = (base, base + height);
                mesh.cuboid(
                    Vec3::new(x - 0.015, low, z - 0.015),
                    Vec3::new(x + 0.015, high, z + 0.015),
                    palette::REBAR,
                );
                let (from, to) = (x.min(x + bend), x.max(x + bend));
                mesh.cuboid(
                    Vec3::new(from - 0.015, high - 0.03, z - 0.015),
                    Vec3::new(to + 0.015, high, z + 0.015),
                    palette::REBAR,
                );
            }
            let points = [
                (0.1, top),
                (0.02, 0.7),
                (0.14, 0.5),
                (0.04, 0.26),
                (0.12, 0.05),
            ];
            crack(&mut mesh, Vec3::Z, &points.map(|(x, y)| front(x, y)));
            let points = [(-0.3, base), (-0.22, 0.4), (-0.34, 0.18)];
            crack(&mut mesh, Vec3::Z, &points.map(|(x, y)| front(x, y)));
            crack(&mut mesh, Vec3::Z, &[front(0.02, 0.7), front(0.3, 0.64)]);
            let points = [(0.05, half), (0.2, 0.2), (0.12, 0.02)];
            crack(&mut mesh, Vec3::Y, &points.map(|(x, z)| on_top(x, z)));
            crack(&mut mesh, Vec3::Y, &[on_top(0.3, -0.05), on_top(0.2, -0.4)]);
            let points = [(0.3, top), (0.1, 0.6), (0.25, 0.35), (0.12, 0.1)];
            crack(&mut mesh, Vec3::X, &points.map(|(z, y)| side(z, y)));
        }
    }
    mesh.build()
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
    mut broken: MessageReader<WallBroken>,
    mut views: ResMut<StructureViews>,
    assets: Res<StructureAssets>,
    material: Res<ShapeMaterial>,
    transforms: Query<(&Transform, Has<Tween>), With<StructureView>>,
    mut particles: Particles,
    mut shake: ResMut<Shake>,
) {
    let broken: Vec<WallBroken> = broken.read().copied().collect();
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
            let breaking = broken.iter().find(|wall| wall.cell == change.cell);
            match (transforms.get(entity), breaking) {
                (Ok((transform, _)), Some(wall)) => {
                    crumble(&mut commands, entity, transform.scale);
                    break_apart(&mut particles, &mut commands, wall);
                    shake.add_capped(BREAK_SHAKE, BREAK_SHAKE_MAX);
                }
                (Ok((transform, false)), None) => {
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
                Structure::Wall { .. } => assets.wall(Wear::of_structure(structure)),
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
                    Wear::of_structure(structure),
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

/// A broken wall sinks into the ground and crumbles away.
fn crumble(commands: &mut Commands, entity: Entity, from: Vec3) {
    commands
        .entity(entity)
        .remove::<(StructureView, Squash, Wobble, Tween)>()
        .insert(
            Tween::new(
                TweenTarget::Scale {
                    from,
                    to: Vec3::new(1.15, 0.02, 1.15),
                },
                EaseFunction::QuadraticIn,
                0.0,
                CRUMBLE_SECONDS,
            )
            .then_despawn(),
        );
}

/// A big burst of rubble and dust where a wall broke, thrown away from what broke it.
fn break_apart(particles: &mut Particles, commands: &mut Commands, wall: &WallBroken) {
    let center = cell_center(wall.cell);
    let rubble = match wall.by {
        Class::Swarmer => RUBBLE_SPREAD,
        Class::Brute => RUBBLE_SPREAD * 1.6,
    };
    particles.burst(
        commands,
        center + Vec3::Y * WALL_HEIGHT * 0.5,
        WALL_WIDTH / 2.0,
        Burst {
            colors: &palette::RUBBLE,
            count: BREAK_RUBBLE_COUNT,
            size: RUBBLE_SIZE * 1.3,
            spread: rubble,
            lift: RUBBLE_LIFT,
            lifetime: RUBBLE_LIFETIME * 1.4,
        },
    );
    particles.burst(
        commands,
        center + Vec3::Y * 0.1,
        WALL_WIDTH / 2.0,
        Burst {
            colors: &palette::DUST,
            count: BREAK_DUST_COUNT,
            size: DUST_SIZE * 1.2,
            spread: DUST_SPREAD,
            lift: DUST_LIFT,
            lifetime: DUST_LIFETIME * 1.3,
        },
    );
}

/// Walls taking hits: they shudder, chip, and look more worn as their HP drops.
fn show_wear(
    mut commands: Commands,
    mut damaged: MessageReader<WallDamaged>,
    views: Res<StructureViews>,
    assets: Res<StructureAssets>,
    mut walls: Query<
        (&Transform, Option<&Wobble>, &mut Mesh3d, &mut Wear),
        (With<StructureView>, Without<Tween>),
    >,
    mut particles: Particles,
) {
    let mut chips = 0;
    for hit in damaged.read() {
        let Some(&entity) = views.0.get(&hit.cell) else {
            continue;
        };
        let Ok((transform, wobble, mut mesh, mut wear)) = walls.get_mut(entity) else {
            continue;
        };
        let now = Wear::of(hit.hp, hit.max);
        if *wear != now {
            *wear = now;
            mesh.0 = assets.wall(now);
        }
        let brute = hit.by == Class::Brute;
        // Tip away from whatever hit it. Swarmer bites don't restart a shudder already going,
        // or a pile of them would keep the wall jittering.
        let center = cell_center(hit.cell);
        let away = (center.xz() - hit.from).normalize_or(Vec2::Y);
        if brute || wobble.is_none() {
            let amount = if brute {
                WALL_SMASH_WOBBLE
            } else {
                WALL_HIT_WOBBLE
            };
            let axis = Vec3::new(away.y, 0.0, -away.x);
            commands
                .entity(entity)
                .insert(Wobble::new(wobble, transform, axis, amount));
        }
        // Flecks off the face that was hit.
        if chips < MAX_CHIP_BURSTS {
            chips += 1;
            let half = WALL_WIDTH / 2.0;
            let face = hit.from.clamp(
                center.xz() - Vec2::splat(half),
                center.xz() + Vec2::splat(half),
            );
            let count = if brute { CHIP_COUNT * 4 } else { CHIP_COUNT };
            particles.burst(
                &mut commands,
                Vec3::new(face.x, WALL_HEIGHT * 0.55, face.y),
                0.1,
                Burst {
                    colors: &palette::RUBBLE,
                    count,
                    size: CHIP_SIZE,
                    spread: CHIP_SPREAD,
                    lift: CHIP_LIFT,
                    lifetime: CHIP_LIFETIME,
                },
            );
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walls_look_more_worn_as_they_lose_hp() {
        assert_eq!(Wear::of(100, 100), Wear::Intact);
        assert_eq!(Wear::of(67, 100), Wear::Intact);
        assert_eq!(Wear::of(65, 100), Wear::Cracked);
        assert_eq!(Wear::of(34, 100), Wear::Cracked);
        assert_eq!(Wear::of(32, 100), Wear::Battered);
        assert_eq!(Wear::of(1, 100), Wear::Battered);
        assert_eq!(Wear::of_structure(Structure::WALL), Wear::Intact);
        assert_eq!(Wear::of_structure(Structure::CommandCenter), Wear::Intact);
    }
}
