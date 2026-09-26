//! Carrying out orders: walking the path, swinging the axe or hammer, and what happens when the
//! work is done.

use super::queue::{OrderKind, Problem};
use super::{
    Backpack, IncomingLog, Keeper, KeeperAssets, KeeperBody, OrderQueue, Task, TreeIndex, model,
    nav::NavGrid, route,
};
use crate::island::{Felled, Tree};
use crate::juice::{Burst, Flight, FlightTarget, Landed, Particles, Squash, Tween, Wobble};
use crate::palette;
use crate::towers::{BuildSite, TowerAssets, finish_site, spawn_site};
use crate::tuning::{BUILD_SECONDS, CHOP_SECONDS, KEEPER_SPEED};
use crate::ui::{OrderFinished, Popup};
use crate::voxel::{VOXEL, VoxelMaterial};
use bevy::prelude::*;

/// Roughly how long one tool swing takes. Adjusted so a whole number of swings fits the work.
const CHOP_SWING: f32 = 0.5;
const HAMMER_SWING: f32 = 0.4;
/// Where in a swing the tool hits (0 = start of the wind-up, 1 = back at rest).
pub const STRIKE_AT: f32 = 0.8;
const SHRUG_TIME: f32 = 0.7;
/// Height above the keeper's feet for popups.
const HEAD_HEIGHT: f32 = 0.7;

pub(super) fn carry_out_orders(
    mut commands: Commands,
    time: Res<Time>,
    mut queue: ResMut<OrderQueue>,
    mut backpack: ResMut<Backpack>,
    nav: Res<NavGrid>,
    tree_index: Res<TreeIndex>,
    keeper: Single<(&mut Keeper, &mut Transform), Without<Tween>>,
    body: Single<(Entity, &GlobalTransform), With<KeeperBody>>,
    trees: Query<(Entity, &Tree, &Transform, Option<&Wobble>), Without<Keeper>>,
    sites: Query<&BuildSite>,
    mut squashes: Query<&mut Squash>,
    assets: (Res<KeeperAssets>, Res<TowerAssets>, Res<VoxelMaterial>),
    mut particles: Particles,
    mut popups: MessageWriter<Popup>,
    mut finished: MessageWriter<OrderFinished>,
) {
    let (mut keeper, mut transform) = keeper.into_inner();
    let (body, body_transform) = *body;
    let (keeper_assets, tower_assets, voxel) = assets;
    let position = transform.translation.xz();
    let head = transform.translation + Vec3::Y * HEAD_HEIGHT;

    // The player removed the order he was working on.
    if let Some(order) = keeper.task.order()
        && queue.started().map(|started| started.id) != Some(order)
    {
        if let Some(site) = keeper.task.site() {
            commands.entity(site).despawn();
            particles.burst(
                &mut commands,
                Vec3::new(0.0, 0.1, 0.0) + site_position(&sites, site),
                Burst {
                    colors: &[palette::PLANK, palette::HEARTH_STONE],
                    count: 10,
                    size: 0.05,
                    spread: 1.0,
                    lift: 1.4,
                    lifetime: 0.6,
                },
            );
        }
        keeper.task = Task::Idle;
    }

    let task = std::mem::take(&mut keeper.task);
    keeper.task = match task {
        Task::Idle => {
            let Some(order) = queue.orders().first().copied() else {
                return;
            };
            let problem = backpack.problem(&order.kind);
            let mut nav = nav.clone();
            let path = problem
                .is_none()
                .then(|| route::path_for(&order.kind, position, &mut nav, &*tree_index))
                .flatten();
            match path {
                Some(path) => {
                    queue.start();
                    let site = match order.kind {
                        OrderKind::Build(cell) => {
                            Some(spawn_site(&mut commands, &tower_assets, &voxel, cell))
                        }
                        _ => None,
                    };
                    Task::Walking {
                        order: order.id,
                        path,
                        next: 1,
                        site,
                    }
                }
                None => {
                    // Skip it, visibly.
                    queue.remove(order.id);
                    let problem = problem.unwrap_or(Problem::Unreachable);
                    popups.write(Popup::problem(head, problem.message()));
                    keeper.shrug = SHRUG_TIME;
                    Task::Idle
                }
            }
        }

        Task::Walking {
            order,
            path,
            mut next,
            site,
        } => {
            let mut step = KEEPER_SPEED * time.delta_secs();
            let mut at = position;
            while step > 0.0 && next < path.len() {
                let to = path[next] - at;
                let distance = to.length();
                if distance <= step {
                    at = path[next];
                    step -= distance;
                    next += 1;
                } else {
                    at += to / distance * step;
                    step = 0.0;
                }
            }
            if let Some(&ahead) = path.get(next)
                && ahead.distance(at) > 1e-3
            {
                keeper.face(ahead - at);
            }
            let walked = at.distance(position);
            keeper.walk_phase += walked / super::anim::STRIDE;
            transform.translation.x = at.x;
            transform.translation.z = at.y;

            if next < path.len() {
                Task::Walking {
                    order,
                    path,
                    next,
                    site,
                }
            } else {
                let kind = queue.started().map(|started| started.kind);
                match kind {
                    Some(OrderKind::Chop(_)) => {
                        Task::working(order, CHOP_SECONDS, CHOP_SWING, site)
                    }
                    Some(OrderKind::Build(_)) => {
                        Task::working(order, BUILD_SECONDS, HAMMER_SWING, site)
                    }
                    _ => {
                        finished.write(OrderFinished(order));
                        queue.finish();
                        if let Ok(mut squash) = squashes.get_mut(body) {
                            squash.pulse(-0.12);
                        }
                        Task::Idle
                    }
                }
            }
        }

        Task::Working {
            order,
            elapsed,
            duration,
            swing,
            site,
        } => {
            let Some(kind) = queue.started().map(|started| started.kind) else {
                return;
            };
            let now = elapsed + time.delta_secs();
            let tree = match kind {
                OrderKind::Chop(id) => trees.iter().find(|(_, tree, ..)| tree.id == id),
                _ => None,
            };
            let target = match (kind, tree) {
                (OrderKind::Build(cell), _) => cell.as_vec2(),
                (_, Some((_, tree, ..))) => tree.position,
                _ => position,
            };
            keeper.face(target - position);

            for _ in strikes(elapsed, swing)..strikes(now, swing) {
                if let Ok(mut squash) = squashes.get_mut(body) {
                    squash.pulse(-0.1);
                }
                let toward = (target - position).normalize_or(Vec2::X);
                match (kind, tree, site) {
                    (OrderKind::Chop(_), Some((entity, tree, tree_transform, wobble)), _) => {
                        let axis = Vec3::Y.cross(Vec3::new(toward.x, 0.0, toward.y));
                        commands.entity(entity).insert(Wobble::new(
                            wobble,
                            tree_transform,
                            axis,
                            0.14,
                        ));
                        particles.burst(
                            &mut commands,
                            Vec3::new(tree.position.x, 0.18, tree.position.y)
                                - Vec3::new(toward.x, 0.0, toward.y) * 0.06,
                            Burst {
                                colors: &[palette::LOG_END, palette::BARK],
                                count: 5,
                                size: 0.035,
                                spread: 0.9,
                                lift: 1.6,
                                lifetime: 0.6,
                            },
                        );
                    }
                    (OrderKind::Build(cell), _, Some(site)) => {
                        if let Ok(site) = sites.get(site)
                            && let Ok(mut squash) = squashes.get_mut(site.body)
                        {
                            squash.pulse(-0.15);
                        }
                        particles.burst(
                            &mut commands,
                            Vec3::new(cell.x as f32, 0.25, cell.y as f32),
                            Burst {
                                colors: &[palette::PLANK, palette::DIRT, palette::ROAD],
                                count: 6,
                                size: 0.04,
                                spread: 0.9,
                                lift: 1.3,
                                lifetime: 0.5,
                            },
                        );
                    }
                    _ => {}
                }
            }

            if now < duration {
                Task::Working {
                    order,
                    elapsed: now,
                    duration,
                    swing,
                    site,
                }
            } else {
                finished.write(OrderFinished(order));
                finish_order(
                    &mut commands,
                    kind,
                    tree.map(|(entity, tree, ..)| (entity, tree)),
                    site,
                    position,
                    &mut backpack,
                    &mut queue,
                    (body, body_transform),
                    (&keeper_assets, &tower_assets, &voxel),
                );
                Task::Idle
            }
        }
    };
}

/// Wraps up a chop or build: wood in or out of the backpack, the tree falls or the tower drops.
fn finish_order(
    commands: &mut Commands,
    kind: OrderKind,
    tree: Option<(Entity, &Tree)>,
    site: Option<Entity>,
    position: Vec2,
    backpack: &mut Backpack,
    queue: &mut OrderQueue,
    (body, body_transform): (Entity, &GlobalTransform),
    (keeper_assets, tower_assets, voxel): (&KeeperAssets, &TowerAssets, &VoxelMaterial),
) {
    // Done.
    let wood_before = backpack.wood;
    backpack.apply(&kind);
    queue.finish();
    match (kind, tree, site) {
        (OrderKind::Chop(_), Some((entity, tree, ..)), _) => {
            commands
                .entity(entity)
                .insert(Felled::new(tree, tree.position - position));
            let slot = backpack.wood.saturating_sub(1) as usize;
            commands.spawn((
                IncomingLog,
                Mesh3d(keeper_assets.log.clone()),
                MeshMaterial3d(voxel.0.clone()),
                Transform::from_xyz(tree.position.x, 0.3, tree.position.y)
                    .with_scale(Vec3::splat(0.9)),
                Flight::new(
                    Vec3::new(tree.position.x, 0.3, tree.position.y),
                    FlightTarget::Entity(body, model::log_slot(slot) * VOXEL),
                    0.5,
                    0.55,
                )
                .with_spin(9.0),
            ));
        }
        (OrderKind::Build(cell), _, Some(site)) => {
            // The wood leaves the backpack and flies onto the site, then the tower drops in.
            let site_top = Vec3::new(cell.x as f32, 0.3, cell.y as f32);
            for (k, slot) in (backpack.wood..wood_before).rev().enumerate() {
                let from = body_transform.transform_point(model::log_slot(slot as usize) * VOXEL);
                commands.spawn((
                    Mesh3d(keeper_assets.log.clone()),
                    MeshMaterial3d(voxel.0.clone()),
                    Transform::from_translation(from).with_scale(Vec3::splat(0.9)),
                    Flight::new(
                        from,
                        FlightTarget::Point(site_top),
                        0.35 + 0.1 * k as f32,
                        0.22 + 0.05 * k as f32,
                    )
                    .with_spin(-8.0),
                ));
            }
            finish_site(commands, tower_assets, voxel, site, cell);
        }
        _ => {}
    }
}

impl Task {
    fn working(order: super::OrderId, duration: f32, swing: f32, site: Option<Entity>) -> Self {
        let swings = (duration / swing).round().max(1.0);
        Task::Working {
            order,
            elapsed: 0.0,
            duration,
            swing: duration / swings,
            site,
        }
    }
}

impl Keeper {
    fn face(&mut self, direction: Vec2) {
        if direction.length_squared() > 1e-6 {
            self.facing = direction.x.atan2(direction.y);
        }
    }
}

/// How many tool strikes have landed after `elapsed` seconds of work.
fn strikes(elapsed: f32, swing: f32) -> i32 {
    ((elapsed / swing) - STRIKE_AT).floor() as i32 + 1
}

fn site_position(sites: &Query<&BuildSite>, site: Entity) -> Vec3 {
    sites
        .get(site)
        .map_or(Vec3::ZERO, |site| crate::island::cell_to_world(site.cell))
}

/// A chopped log landed in the backpack.
pub(super) fn log_landed(
    landed: On<Landed>,
    logs: Query<(), With<IncomingLog>>,
    keeper: Query<&GlobalTransform, With<Keeper>>,
    mut body: Query<&mut Squash, With<KeeperBody>>,
    mut popups: MessageWriter<Popup>,
) {
    if !logs.contains(landed.entity) {
        return;
    }
    if let Ok(mut squash) = body.single_mut() {
        squash.pulse(-0.18);
    }
    if let Ok(keeper) = keeper.single() {
        popups.write(Popup::new(
            keeper.translation() + Vec3::Y * HEAD_HEIGHT,
            "+1 wood",
            palette::LOG_END,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strikes_land_once_per_swing() {
        let swing = 0.5;
        assert_eq!(strikes(0.0, swing), 0);
        assert_eq!(strikes(0.39, swing), 0);
        assert_eq!(strikes(0.41, swing), 1);
        // The last strike lands before the work is done.
        assert_eq!(strikes(1.99, swing), 4);
    }
}
