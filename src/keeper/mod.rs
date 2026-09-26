//! The little guy (the keeper) and his order queue.
//!
//! The queue, the wood plan, walking paths and route planning are plain data in [`queue`], [`nav`]
//! and [`route`]. The rest is the ECS side: clicking to give orders, carrying them out, animation
//! and showing the planned route on the island.

mod anim;
mod input;
mod model;
pub mod nav;
pub mod queue;
pub mod route;
mod route_view;
mod work;

pub use anim::shown_wood;
pub use input::{Aim, cursor_position};
pub use queue::{Backpack, OrderId, OrderKind, OrderQueue, Problem};
pub use route::PlannedOrder;

use crate::island::{Island, Tree, TreeId};
use crate::juice::{Squash, Tween};
use crate::towers::{BuildSite, Tower};
use crate::voxel::{VOXEL, VoxelMaterial};
use bevy::prelude::*;
use nav::NavGrid;
use route::{Progress, TreeLookup};
use std::collections::HashMap;

/// Where the keeper stands when the game starts: beside the hearth, facing the camera.
const START: Vec2 = Vec2::new(-0.55, 0.3);
const START_FACING: f32 = 0.55;
/// The keeper lands on the island once it has assembled.
const INTRO_DELAY: f32 = 1.9;
const INTRO_DROP_HEIGHT: f32 = 1.5;
/// The model is authored at voxel scale; this makes him a bit smaller than a tree.
const MODEL_SCALE: f32 = 1.05;

pub struct KeeperPlugin;

impl Plugin for KeeperPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OrderQueue>()
            .init_resource::<Backpack>()
            .init_resource::<NavGrid>()
            .init_resource::<TreeIndex>()
            .init_resource::<Plan>()
            .init_resource::<Aim>()
            .init_resource::<VoxelMaterial>()
            .init_resource::<KeeperAssets>()
            .add_systems(Startup, (spawn_keeper, input::spawn_aim_marker))
            .add_systems(
                Update,
                (
                    (input::aim, input::give_orders).chain(),
                    update_nav,
                    work::carry_out_orders,
                    update_plan,
                )
                    .chain()
                    .in_set(PlanSystems),
            )
            .add_systems(
                Update,
                (
                    anim::animate_keeper,
                    anim::show_backpack_logs,
                    route_view::draw_route,
                    route_view::sync_ghosts,
                    input::move_aim_marker,
                    input::wiggle_hovered_tree,
                )
                    .after(PlanSystems),
            )
            .add_observer(work::log_landed);
    }
}

/// Everything that changes the queue and the plan. Systems that show them run after this.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PlanSystems;

/// The keeper and what he's doing right now.
#[derive(Component, Debug, Default)]
pub struct Keeper {
    pub task: Task,
    /// The direction he wants to face (yaw, radians). He turns toward it smoothly.
    facing: f32,
    /// Seconds left of a "can't do that" shrug.
    shrug: f32,
    /// Steps taken, for the walk cycle.
    walk_phase: f32,
}

#[derive(Debug, Default, Clone)]
pub enum Task {
    #[default]
    Idle,
    Walking {
        order: OrderId,
        path: Vec<Vec2>,
        /// Index of the path point he's walking toward.
        next: usize,
        site: Option<Entity>,
    },
    Working {
        order: OrderId,
        elapsed: f32,
        duration: f32,
        /// Seconds per tool swing.
        swing: f32,
        site: Option<Entity>,
    },
}

impl Task {
    fn order(&self) -> Option<OrderId> {
        match self {
            Task::Idle => None,
            Task::Walking { order, .. } | Task::Working { order, .. } => Some(*order),
        }
    }

    fn site(&self) -> Option<Entity> {
        match self {
            Task::Idle => None,
            Task::Walking { site, .. } | Task::Working { site, .. } => *site,
        }
    }

    /// How far along the current chop or build is, from 0 to 1.
    pub fn work_progress(&self) -> Option<f32> {
        match self {
            Task::Working {
                elapsed, duration, ..
            } => Some((elapsed / duration).min(1.0)),
            _ => None,
        }
    }

    /// The rest of the walk: from `position` through the remaining path points.
    pub fn remaining_path(&self, position: Vec2) -> Vec<Vec2> {
        match self {
            Task::Walking { path, next, .. } => std::iter::once(position)
                .chain(path[*next..].iter().copied())
                .collect(),
            _ => Vec::new(),
        }
    }
}

#[derive(Component)]
struct KeeperBody;

/// The arm holding the tool; rotating it around x swings the tool.
#[derive(Component)]
struct KeeperArm;

#[derive(Component)]
struct KeeperFoot(usize);

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Tool {
    Axe,
    Hammer,
}

/// One of the logs stacked on the backpack, shown when there's that much wood.
#[derive(Component)]
struct BackpackLog {
    index: usize,
    shown: bool,
}

/// A log flying into the backpack. It only counts as carried (on screen) once it lands.
#[derive(Component)]
pub struct IncomingLog;

#[derive(Resource)]
struct KeeperAssets {
    body: Handle<Mesh>,
    foot: Handle<Mesh>,
    arm: Handle<Mesh>,
    axe: Handle<Mesh>,
    hammer: Handle<Mesh>,
    log: Handle<Mesh>,
}

impl FromWorld for KeeperAssets {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        Self {
            body: meshes.add(model::body_mesh().build()),
            foot: meshes.add(model::foot_mesh().build()),
            arm: meshes.add(model::arm_mesh().build()),
            axe: meshes.add(model::axe_mesh().build()),
            hammer: meshes.add(model::hammer_mesh().build()),
            log: meshes.add(model::log_mesh().build()),
        }
    }
}

/// Where each standing tree is, for planning and pathfinding.
#[derive(Resource, Default, PartialEq)]
pub struct TreeIndex(HashMap<TreeId, Vec2>);

impl TreeLookup for TreeIndex {
    fn tree_position(&self, id: TreeId) -> Option<Vec2> {
        self.0.tree_position(id)
    }
}

/// The planned queue: problems, wood and paths for every order. Recomputed when anything changes.
#[derive(Resource, Default)]
pub struct Plan {
    pub orders: Vec<PlannedOrder>,
    progress: Option<Progress>,
}

impl Plan {
    pub fn get(&self, id: OrderId) -> Option<&PlannedOrder> {
        self.orders.iter().find(|planned| planned.order.id == id)
    }
}

fn spawn_keeper(mut commands: Commands, assets: Res<KeeperAssets>, material: Res<VoxelMaterial>) {
    let drop = Tween::drop_in(
        Vec3::new(START.x, 0.0, START.y),
        INTRO_DROP_HEIGHT,
        INTRO_DELAY,
    );
    let mesh = |handle: &Handle<Mesh>| (Mesh3d(handle.clone()), MeshMaterial3d(material.0.clone()));
    let voxels = |v: Vec3| Transform::from_translation(v * VOXEL);

    commands
        .spawn((
            Keeper {
                facing: START_FACING,
                ..default()
            },
            Transform {
                rotation: Quat::from_rotation_y(START_FACING),
                scale: Vec3::splat(MODEL_SCALE),
                ..drop.start_transform()
            },
            Visibility::Hidden,
            drop,
        ))
        .with_children(|keeper| {
            keeper
                .spawn((
                    KeeperBody,
                    mesh(&assets.body),
                    Transform::default(),
                    Squash::new(Vec3::ONE),
                ))
                .with_children(|body| {
                    body.spawn((KeeperArm, mesh(&assets.arm), voxels(model::SHOULDER)))
                        .with_children(|arm| {
                            arm.spawn((Tool::Axe, mesh(&assets.axe), Transform::default()));
                            arm.spawn((
                                Tool::Hammer,
                                mesh(&assets.hammer),
                                Transform::default(),
                                Visibility::Hidden,
                            ));
                        });
                    for index in 0..crate::tuning::BACKPACK_CAPACITY as usize {
                        body.spawn((
                            BackpackLog {
                                index,
                                shown: false,
                            },
                            mesh(&assets.log),
                            voxels(model::log_slot(index)),
                            Visibility::Hidden,
                        ));
                    }
                });
            for (index, foot) in model::FEET.into_iter().enumerate() {
                keeper.spawn((KeeperFoot(index), mesh(&assets.foot), voxels(foot)));
            }
        });
}

/// Rebuilds the walkable ground and the tree index from what's on the island.
fn update_nav(
    island: Res<Island>,
    trees: Query<&Tree>,
    sites: Query<&BuildSite>,
    towers: Query<&Tower>,
    mut nav: ResMut<NavGrid>,
    mut index: ResMut<TreeIndex>,
) {
    let mut grid = NavGrid::new(&island);
    let mut positions = HashMap::new();
    for tree in &trees {
        grid.block_point(tree.position);
        positions.insert(tree.id, tree.position);
    }
    for cell in sites
        .iter()
        .map(|site| site.cell)
        .chain(towers.iter().map(|tower| tower.cell))
    {
        grid.block_tower(cell);
    }
    nav.set_if_neq(grid);
    index.set_if_neq(TreeIndex(positions));
}

fn update_plan(
    queue: Res<OrderQueue>,
    backpack: Res<Backpack>,
    nav: Res<NavGrid>,
    trees: Res<TreeIndex>,
    keeper: Query<(&Keeper, &Transform)>,
    mut plan: ResMut<Plan>,
) {
    let Ok((keeper, transform)) = keeper.single() else {
        return;
    };
    let position = transform.translation.xz();
    let progress = match (&keeper.task, queue.started()) {
        (Task::Walking { path, .. }, Some(_)) => Progress::Started {
            end: *path.last().unwrap_or(&position),
        },
        (Task::Working { .. }, Some(_)) => Progress::Started { end: position },
        _ => Progress::NotStarted { keeper: position },
    };
    let changed =
        queue.is_changed() || backpack.is_changed() || nav.is_changed() || trees.is_changed();
    if !changed && plan.progress == Some(progress) {
        return;
    }
    plan.orders = route::plan(queue.orders(), progress, &backpack, &nav, &*trees);
    plan.progress = Some(progress);
}
