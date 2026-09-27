//! Showing the plan on the island: a dotted route through the queued orders, and ghosts where
//! towers and the catapult are planned.

use super::{Building, Keeper, OrderKind, OrderQueue, Plan};
use crate::juice::Particles;
use crate::palette;
use crate::towers::{Ghost, TowerAssets, ghost_bundle, ghost_material};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use std::collections::HashMap;

/// Distance between route dots.
const DOT_SPACING: f32 = 0.13;
const DOT_SIZE: f32 = 0.055;
/// Dots closer than this to the start of a leg are left out, so they don't sit under the keeper.
const DOT_START_GAP: f32 = 0.12;
const DOT_HEIGHT: f32 = 0.025;

/// One dot of the planned route. Dots are pooled and reused as the route changes.
#[derive(Component)]
pub(super) struct RouteDot;

/// Places dots along a path, measured back from its end so they stay put while the keeper walks
/// the first leg. Returns (position, distance from the end) pairs.
pub fn dots_along(path: &[Vec2], spacing: f32, start_gap: f32) -> Vec<(Vec2, f32)> {
    let mut dots = Vec::new();
    let total: f32 = path.windows(2).map(|pair| pair[0].distance(pair[1])).sum();
    let mut from_end = 0.0;
    // Walk the path backward, dropping a dot every `spacing`.
    let mut next_dot = 0.0;
    for pair in path.windows(2).rev() {
        let (a, b) = (pair[0], pair[1]);
        let length = a.distance(b);
        while next_dot <= from_end + length {
            let distance_from_start = total - next_dot;
            if distance_from_start < start_gap {
                return dots;
            }
            let t = (next_dot - from_end) / length.max(1e-6);
            dots.push((b.lerp(a, t), next_dot));
            next_dot += spacing;
        }
        from_end += length;
    }
    dots
}

pub(super) fn draw_route(
    mut commands: Commands,
    time: Res<Time>,
    plan: Res<Plan>,
    keeper: Query<(&Keeper, &Transform), Without<RouteDot>>,
    mut dots: Query<(&mut Transform, &mut Visibility), With<RouteDot>>,
    mut pool: Local<Vec<Entity>>,
    mut particles: Particles,
) {
    let Ok((keeper, transform)) = keeper.single() else {
        return;
    };
    let mut paths = vec![keeper.task.remaining_path(transform.translation.xz())];
    paths.extend(plan.orders.iter().map(|planned| planned.path.clone()));

    let mut placed = Vec::new();
    let mut offset = 0.0;
    for path in &paths {
        let dots = dots_along(path, DOT_SPACING, DOT_START_GAP);
        // Keep the "marching" wave continuous from one leg to the next.
        let length: f32 = path.windows(2).map(|pair| pair[0].distance(pair[1])).sum();
        placed.extend(
            dots.into_iter()
                .map(|(at, from_end)| (at, offset + length - from_end)),
        );
        offset += length;
    }

    while pool.len() < placed.len() {
        let dot = commands
            .spawn((
                RouteDot,
                particles.cube(palette::ROUTE),
                Transform::from_scale(Vec3::ZERO),
                Visibility::Hidden,
                NotShadowCaster,
            ))
            .id();
        pool.push(dot);
    }
    let t = time.elapsed_secs();
    for (k, &entity) in pool.iter().enumerate() {
        let Ok((mut transform, mut visibility)) = dots.get_mut(entity) else {
            continue;
        };
        let Some(&(at, along)) = placed.get(k) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        // A gentle wave running along the route toward where it goes.
        let wave = 0.5 + 0.5 * (along * 9.0 - t * 5.0).sin();
        transform.translation = Vec3::new(at.x, DOT_HEIGHT + wave * 0.012, at.y);
        transform.scale = Vec3::new(1.0, 0.45, 1.0) * DOT_SIZE * (0.8 + 0.35 * wave);
    }
}

/// Keeps a ghost on every planned build that hasn't started yet.
pub(super) fn sync_ghosts(
    mut commands: Commands,
    plan: Res<Plan>,
    queue: Res<OrderQueue>,
    assets: Res<TowerAssets>,
    mut ghosts: Local<HashMap<(IVec2, Building), Entity>>,
    mut materials: Query<&mut MeshMaterial3d<StandardMaterial>, With<Ghost>>,
) {
    if !plan.is_changed() {
        return;
    }
    let started = queue.started().map(|order| order.id);
    let wanted: HashMap<(IVec2, Building), bool> = plan
        .orders
        .iter()
        .filter(|planned| Some(planned.order.id) != started)
        .filter_map(|planned| match planned.order.kind {
            // A build waiting for the wave to end will still happen, so its ghost stays blue.
            OrderKind::Build(cell, building) => Some((
                (cell, building),
                planned.problem.is_some_and(|problem| !problem.holds()),
            )),
            _ => None,
        })
        .collect();

    ghosts.retain(|key, entity| {
        let keep = wanted.contains_key(key);
        if !keep {
            commands.entity(*entity).despawn();
        }
        keep
    });
    for (&(cell, building), &problem) in &wanted {
        match ghosts.get(&(cell, building)) {
            Some(&entity) => {
                if let Ok(mut material) = materials.get_mut(entity) {
                    let want = ghost_material(&assets, problem);
                    if material.0 != want {
                        material.0 = want;
                    }
                }
            }
            None => {
                let entity = commands
                    .spawn(ghost_bundle(&assets, cell, building, problem))
                    .id();
                ghosts.insert((cell, building), entity);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dots_are_spaced_back_from_the_end() {
        let path = [Vec2::ZERO, Vec2::new(1.0, 0.0)];
        let dots = dots_along(&path, 0.25, 0.1);
        let xs: Vec<f32> = dots.iter().map(|(at, _)| at.x).collect();
        assert_eq!(xs, vec![1.0, 0.75, 0.5, 0.25]);
    }

    #[test]
    fn dots_follow_corners() {
        let path = [Vec2::ZERO, Vec2::new(0.5, 0.0), Vec2::new(0.5, 0.5)];
        let dots = dots_along(&path, 0.25, 0.0);
        assert_eq!(dots.len(), 5);
        assert!(dots[1].0.distance(Vec2::new(0.5, 0.25)) < 1e-5);
        assert!(dots[2].0.distance(Vec2::new(0.5, 0.0)) < 1e-5);
        assert!(dots[4].0.distance(Vec2::ZERO) < 1e-5);
    }

    #[test]
    fn empty_paths_have_no_dots() {
        assert!(dots_along(&[], 0.25, 0.0).is_empty());
        assert!(dots_along(&[Vec2::ONE], 0.25, 0.0).is_empty());
    }
}
