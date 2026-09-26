//! Towers shooting arrows at slimes during the wave. Which slime to shoot is plain data in
//! [`super::aim`]; here the tower recoils, and arrows arc to their slime and hit it.

use super::aim::{Candidate, pick_target};
use super::{Tower, TowerAssets};
use crate::juice::{Burst, Particles, Squash, Tween, Wobble};
use crate::palette;
use crate::slimes::{ArrowHit, MIDDLE_HEIGHT, Slime};
use crate::tuning::{ARROW_SPEED, TOWER_COOLDOWN, TOWER_DAMAGE, TOWER_RANGE};
use crate::voxel::{TILE_SIZE, VoxelMesh};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

/// Where arrows leave the tower: over the lookout's railing on the side facing the slime, so
/// they're not hidden under the roof.
const LOOKOUT_HEIGHT: f32 = 0.82;
const LOOKOUT_REACH: f32 = 0.34;
/// How high arrows arc: a little lob for every shot, so they show above the roofs, plus more per
/// tile flown.
const ARC: f32 = 0.18;
const ARC_PER_TILE: f32 = 0.12;
/// Shortest flight, so point-blank arrows are still visible.
const MIN_FLIGHT: f32 = 0.12;

/// A tower that shoots. `body` is the child that recoils.
#[derive(Component, Debug)]
pub struct Archer {
    body: Entity,
    /// Seconds until it can shoot again.
    reload: f32,
}

impl Archer {
    pub fn new(body: Entity) -> Self {
        Self { body, reload: 0.0 }
    }
}

/// An arrow on its way to a slime.
#[derive(Component, Debug)]
pub struct Arrow {
    target: Entity,
    from: Vec3,
    /// Where the slime was last seen.
    to: Vec3,
    arc: f32,
    age: f32,
    duration: f32,
}

/// Where an arrow is at `t` (0 to 1) of its flight: a shallow arc from `from` to `to`.
pub fn arrow_position(from: Vec3, to: Vec3, arc: f32, t: f32) -> Vec3 {
    from.lerp(to, t) + Vec3::Y * arc * 4.0 * t * (1.0 - t)
}

pub(super) fn shoot(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<TowerAssets>,
    mut towers: Query<(&GlobalTransform, &mut Archer), (With<Tower>, Without<Tween>)>,
    mut slimes: Query<(Entity, &mut Slime, &GlobalTransform)>,
    mut bodies: Query<(&mut Squash, &Transform, Option<&Wobble>)>,
    mut particles: Particles,
) {
    let dt = time.delta_secs();
    for (tower, mut archer) in &mut towers {
        archer.reload = (archer.reload - dt).max(0.0);
        if archer.reload > 0.0 {
            continue;
        }
        let from = tower.translation().xz();
        let candidates = slimes.iter().map(|(entity, slime, global)| Candidate {
            id: entity,
            position: global.translation().xz(),
            progress: slime.distance,
            doomed: slime.health.doomed(),
        });
        let Some(target) = pick_target(from, TOWER_RANGE * TILE_SIZE, candidates) else {
            continue;
        };
        let Ok((_, mut slime, global)) = slimes.get_mut(target) else {
            continue;
        };
        archer.reload = TOWER_COOLDOWN;
        slime.health.expect(TOWER_DAMAGE);

        let to = slime_middle(global);
        let toward = (to - tower.translation()).with_y(0.0).normalize_or(Vec3::X);
        let start = tower.translation() + Vec3::Y * LOOKOUT_HEIGHT + toward * LOOKOUT_REACH;
        let distance = start.distance(to);
        commands.spawn((
            Arrow {
                target,
                from: start,
                to,
                arc: ARC + ARC_PER_TILE * distance,
                age: 0.0,
                duration: (distance / (ARROW_SPEED * TILE_SIZE)).max(MIN_FLIGHT),
            },
            Mesh3d(assets.arrow.clone()),
            MeshMaterial3d(assets.voxel.clone()),
            Transform::from_translation(start).looking_to(to - start, Vec3::Y),
            NotShadowCaster,
        ));

        // Recoil: the tower squashes and rocks back from the shot.
        if let Ok((mut squash, transform, wobble)) = bodies.get_mut(archer.body) {
            squash.pulse(-0.08);
            let axis = Vec3::Y.cross(toward);
            commands
                .entity(archer.body)
                .insert(Wobble::new(wobble, transform, axis, -0.05));
        }
        particles.burst(
            &mut commands,
            start,
            Burst {
                colors: &[palette::PLANK, palette::UI_PAPER],
                count: 3,
                size: 0.03,
                spread: 0.6,
                lift: 0.8,
                lifetime: 0.35,
            },
        );
    }
}

pub(super) fn fly_arrows(
    mut commands: Commands,
    time: Res<Time>,
    mut arrows: Query<(Entity, &mut Arrow, &mut Transform)>,
    slimes: Query<&GlobalTransform, With<Slime>>,
    mut hits: MessageWriter<ArrowHit>,
    mut particles: Particles,
) {
    for (entity, mut arrow, mut transform) in &mut arrows {
        arrow.age += time.delta_secs();
        // Follow the slime as it hops along.
        let target = slimes.get(arrow.target).ok();
        if let Some(global) = target {
            arrow.to = slime_middle(global);
        }
        let t = (arrow.age / arrow.duration).min(1.0);
        let at = arrow_position(arrow.from, arrow.to, arrow.arc, t);
        let heading = at - transform.translation;
        if heading.length_squared() > 1e-8 {
            transform.look_to(heading, Vec3::Y);
        }
        transform.translation = at;
        if t < 1.0 {
            continue;
        }
        commands.entity(entity).despawn();
        if target.is_some() {
            hits.write(ArrowHit {
                slime: arrow.target,
                damage: TOWER_DAMAGE,
            });
        } else {
            // The slime got away (into the hearth): the arrow thuds into the ground.
            particles.burst(
                &mut commands,
                at,
                Burst {
                    colors: &[palette::PLANK, palette::DIRT],
                    count: 3,
                    size: 0.03,
                    spread: 0.5,
                    lift: 0.8,
                    lifetime: 0.4,
                },
            );
        }
    }
}

/// Where arrows aim: the slime's middle, wherever and however big it is.
fn slime_middle(slime: &GlobalTransform) -> Vec3 {
    slime.transform_point(Vec3::Y * MIDDLE_HEIGHT)
}

/// An arrow pointing along -z (Bevy's forward), in voxels: a shaft, a metal tip and blue
/// fletching to match the tower's roof.
pub(super) fn arrow_mesh() -> VoxelMesh {
    let mut mesh = VoxelMesh::default();
    mesh.cuboid(
        Vec3::new(-0.18, -0.18, -1.4),
        Vec3::new(0.18, 0.18, 1.4),
        palette::PLANK,
    );
    mesh.cuboid(
        Vec3::new(-0.34, -0.34, -1.95),
        Vec3::new(0.34, 0.34, -1.4),
        palette::TOOL_METAL,
    );
    for (min, max) in [
        (Vec3::new(-0.6, -0.08, 0.7), Vec3::new(0.6, 0.08, 1.4)),
        (Vec3::new(-0.08, -0.6, 0.7), Vec3::new(0.08, 0.6, 1.4)),
    ] {
        mesh.cuboid(min, max, palette::ROOF_TOP);
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_arc_from_the_tower_to_the_slime() {
        let (from, to) = (Vec3::new(0.0, 1.0, 0.0), Vec3::new(2.0, 0.0, 0.0));
        assert_eq!(arrow_position(from, to, 0.3, 0.0), from);
        assert_eq!(arrow_position(from, to, 0.3, 1.0), to);
        let middle = arrow_position(from, to, 0.3, 0.5);
        assert!((middle.y - (0.5 + 0.3)).abs() < 1e-6);
    }
}
