//! The little guy (the keeper) and his order queue.
//!
//! The queue, the wood plan, walking paths, route planning and timing are plain data in [`queue`],
//! [`nav`], [`route`] and [`timeline`]. The rest is the ECS side: clicking to give orders, carrying
//! them out, getting in and out of the catapult ([`manning`]), animation and showing the planned
//! route on the island.

mod anim;
mod input;
mod manning;
mod model;
pub mod nav;
pub mod queue;
pub mod route;
mod route_view;
pub mod timeline;
mod work;

pub use anim::shown_wood;
pub use input::{Aim, BuildChoice, ChooseBuilding, cursor_position};
pub use queue::{Backpack, Building, OrderId, OrderKind, OrderQueue, Phase, Problem};
pub use route::PlannedOrder;
pub use timeline::Slot;

use crate::catapult::Catapult;
use crate::island::{Island, Tree, TreeId};
use crate::juice::{Squash, Tween};
use crate::phase::{GameState, PrepareClock, building_allowed, keeper_working, wave_deadline};
use crate::slimes::WaveRun;
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
            .init_resource::<Timeline>()
            .init_resource::<AimPreview>()
            .init_resource::<Aim>()
            .init_resource::<BuildChoice>()
            .add_message::<ChooseBuilding>()
            .init_resource::<VoxelMaterial>()
            .init_resource::<KeeperAssets>()
            .add_systems(Startup, input::spawn_aim_marker)
            .add_systems(OnEnter(GameState::Intro), spawn_keeper)
            .add_systems(OnExit(GameState::GameOver), clear_keeper)
            .add_systems(
                Update,
                (
                    (
                        input::choose_building,
                        input::aim,
                        input::give_orders.run_if(working),
                    )
                        .chain(),
                    update_nav,
                    manning::work_catapult_seat.run_if(working),
                    work::carry_out_orders.run_if(working),
                    update_plan,
                    update_timeline,
                    preview_aim,
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

/// Run condition: the keeper is taking and carrying out orders (not while a tile is being
/// picked, and not after game over).
pub fn working(state: Res<State<GameState>>) -> bool {
    keeper_working(*state.get())
}

/// Which phase orders are planned for: getting ready for the next wave (which is also what the
/// tile offer plans for), the wave, or the moment after its last slime (`cleared`) is gone.
pub fn plan_phase(state: GameState, cleared: bool) -> Phase {
    if building_allowed(state) {
        Phase::Preparing
    } else if cleared {
        Phase::WaveOver
    } else {
        Phase::Wave
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
    /// In the catapult (or hopping in: `board` goes from 0 to 1), until called out.
    Manning {
        order: OrderId,
        catapult: Entity,
        /// Where he stood to get in, and gets out to.
        stand: Vec2,
        board: f32,
    },
    /// Hopping out of the catapult, back to where he got in.
    Leaving { from: Vec3, to: Vec2, t: f32 },
}

impl Task {
    fn order(&self) -> Option<OrderId> {
        match self {
            Task::Idle | Task::Leaving { .. } => None,
            Task::Walking { order, .. }
            | Task::Working { order, .. }
            | Task::Manning { order, .. } => Some(*order),
        }
    }

    fn site(&self) -> Option<Entity> {
        match self {
            Task::Walking { site, .. } | Task::Working { site, .. } => *site,
            _ => None,
        }
    }

    /// In the catapult, or getting in or out of it: he's not on his own feet.
    pub fn in_catapult(&self) -> bool {
        matches!(self, Task::Manning { .. } | Task::Leaving { .. })
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

    /// Seconds until the order in progress (of this `kind`) is done: the rest of the walk and all
    /// of the work, or what's left of the work. (Manning the catapult never is: see
    /// [`timeline::schedule`].)
    pub fn seconds_left(&self, position: Vec2, kind: &OrderKind) -> f32 {
        match self {
            Task::Idle | Task::Manning { .. } | Task::Leaving { .. } => 0.0,
            Task::Walking { .. } => {
                route::walk_seconds(&self.remaining_path(position)) + kind.work_seconds()
            }
            Task::Working {
                elapsed, duration, ..
            } => (duration - elapsed).max(0.0),
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
    /// What the plan was made for: the keeper's progress, and the phase.
    planned_for: Option<(Progress, Phase)>,
}

impl Plan {
    pub fn get(&self, id: OrderId) -> Option<&PlannedOrder> {
        self.orders.iter().find(|planned| planned.order.id == id)
    }
}

/// When each planned order will happen, from now. Rebuilt every frame, since time passes.
#[derive(Resource, Default)]
pub struct Timeline {
    /// One per planned order, in queue order.
    pub slots: Vec<Slot>,
    /// Seconds until the wave starts, if there's a prepare deadline.
    pub deadline: Option<f32>,
}

impl Timeline {
    pub fn first_late(&self) -> Option<usize> {
        timeline::first_late(&self.slots)
    }

    pub fn late_count(&self) -> usize {
        self.slots.iter().filter(|slot| slot.late).count()
    }

    /// When an order inserted at `index` would start: once the orders before it are done.
    pub fn start_at(&self, index: usize) -> f32 {
        index
            .checked_sub(1)
            .and_then(|previous| self.slots.get(previous))
            .map_or(0.0, |slot| slot.end)
    }
}

/// The order a click would give right now, planned as if it were in the queue.
#[derive(Resource, Default)]
pub struct AimPreview {
    /// The order as planned, and where in the queue it would go.
    planned: Option<(PlannedOrder, usize)>,
    /// What the preview was planned for, to replan only when that changes.
    planned_for: Option<(OrderKind, bool)>,
}

impl AimPreview {
    /// The previewed order as planned, and when it would happen on the current timeline.
    pub fn get(&self, timeline: &Timeline) -> Option<(&PlannedOrder, Slot)> {
        let (planned, index) = self.planned.as_ref()?;
        let start = timeline.start_at(*index);
        let end = start + planned.seconds();
        let late = planned.problem.is_none()
            && timeline
                .deadline
                .is_some_and(|deadline| timeline::is_late(end, deadline));
        Some((planned, Slot { start, end, late }))
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

/// Clears the old run: the keeper, his orders and his wood.
fn clear_keeper(
    mut commands: Commands,
    keeper: Query<Entity, With<Keeper>>,
    mut queue: ResMut<OrderQueue>,
    mut backpack: ResMut<Backpack>,
) {
    for entity in &keeper {
        commands.entity(entity).despawn();
    }
    queue.clear();
    *backpack = Backpack::default();
}

/// Rebuilds the walkable ground and the tree index from what's on the island.
fn update_nav(
    island: Res<Island>,
    trees: Query<&Tree>,
    sites: Query<&BuildSite>,
    towers: Query<&Tower>,
    catapults: Query<&Catapult>,
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
        .chain(catapults.iter().map(|catapult| catapult.cell))
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
    (state, run): (Res<State<GameState>>, Res<WaveRun>),
    keeper: Query<(&Keeper, &Transform)>,
    mut plan: ResMut<Plan>,
) {
    let Ok((keeper, transform)) = keeper.single() else {
        return;
    };
    let planned_for = (
        progress(keeper, transform, &queue),
        plan_phase(*state.get(), run.cleared()),
    );
    let changed =
        queue.is_changed() || backpack.is_changed() || nav.is_changed() || trees.is_changed();
    if !changed && plan.planned_for == Some(planned_for) {
        return;
    }
    let (progress, phase) = planned_for;
    plan.orders = route::plan(queue.orders(), progress, &backpack, phase, &nav, &*trees);
    plan.planned_for = Some(planned_for);
}

/// Where the keeper is in carrying out the queue, for planning.
fn progress(keeper: &Keeper, transform: &Transform, queue: &OrderQueue) -> Progress {
    let position = transform.translation.xz();
    match (&keeper.task, queue.started()) {
        (Task::Walking { path, .. }, Some(_)) => Progress::Started {
            end: *path.last().unwrap_or(&position),
        },
        (Task::Working { .. }, Some(_)) => Progress::Started { end: position },
        (Task::Manning { stand, .. }, Some(_)) => Progress::Started { end: *stand },
        (Task::Leaving { to, .. }, _) => Progress::NotStarted { keeper: *to },
        _ => Progress::NotStarted { keeper: position },
    }
}

/// Time left on the order in progress, if one is.
fn started_left(keeper: &Keeper, transform: &Transform, queue: &OrderQueue) -> Option<f32> {
    let order = queue.started()?;
    Some(
        keeper
            .task
            .seconds_left(transform.translation.xz(), &order.kind),
    )
}

fn update_timeline(
    queue: Res<OrderQueue>,
    plan: Res<Plan>,
    state: Res<State<GameState>>,
    clock: Res<PrepareClock>,
    keeper: Query<(&Keeper, &Transform)>,
    mut timeline: ResMut<Timeline>,
) {
    let Ok((keeper, transform)) = keeper.single() else {
        return;
    };
    let deadline = wave_deadline(*state.get(), &clock);
    let started = started_left(keeper, transform, &queue);
    timeline.slots = timeline::schedule(&plan.orders, started, deadline);
    timeline.deadline = deadline;
}

/// Plans the order under the cursor as if it were queued, so the cursor label can say how long
/// it would take and whether it would be done in time.
fn preview_aim(
    aim: Res<Aim>,
    keys: Res<ButtonInput<KeyCode>>,
    queue: Res<OrderQueue>,
    plan: Res<Plan>,
    backpack: Res<Backpack>,
    nav: Res<NavGrid>,
    trees: Res<TreeIndex>,
    (state, run): (Res<State<GameState>>, Res<WaveRun>),
    keeper: Query<(&Keeper, &Transform)>,
    mut preview: ResMut<AimPreview>,
) {
    let next = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let wanted = aim
        .target
        .map(|target| (target.kind, next))
        .filter(|(kind, _)| queue.find_same_target(kind).is_none() && !queue.is_full());
    let Some((kind, next)) = wanted else {
        preview.planned = None;
        preview.planned_for = None;
        return;
    };
    let Ok((keeper, transform)) = keeper.single() else {
        return;
    };
    if preview.planned_for == wanted && !plan.is_changed() {
        return;
    }
    let (orders, index) = queue.with_candidate(kind, next);
    let planned = route::plan(
        &orders,
        progress(keeper, transform, &queue),
        &backpack,
        plan_phase(*state.get(), run.cleared()),
        &nav,
        &*trees,
    );
    preview.planned = planned
        .into_iter()
        .nth(index)
        .map(|planned| (planned, index));
    preview.planned_for = wanted;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tuning::{CHOP_SECONDS, KEEPER_SPEED};

    #[test]
    fn orders_are_planned_for_the_phase_ahead() {
        assert_eq!(plan_phase(GameState::Prepare, false), Phase::Preparing);
        assert_eq!(plan_phase(GameState::TileOffer, true), Phase::Preparing);
        assert_eq!(plan_phase(GameState::Wave, false), Phase::Wave);
        assert_eq!(plan_phase(GameState::Wave, true), Phase::WaveOver);
    }

    #[test]
    fn time_left_on_the_order_in_progress() {
        let chop = OrderKind::Chop(TreeId {
            cell: IVec2::ZERO,
            index: 0,
        });
        let walking = Task::Walking {
            order: OrderQueue::default().push(chop).unwrap(),
            path: vec![Vec2::ZERO, Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0)],
            next: 2,
            site: None,
        };
        // Halfway down the second leg: half a tile to go, then the chop.
        let left = walking.seconds_left(Vec2::new(1.0, 0.5), &chop);
        assert!((left - (0.5 / KEEPER_SPEED + CHOP_SECONDS)).abs() < 1e-5);

        let working = Task::working(OrderQueue::default().push(chop).unwrap(), 2.0, 0.5, None);
        let Task::Working { order, swing, .. } = working else {
            unreachable!()
        };
        let working = Task::Working {
            order,
            elapsed: 1.5,
            duration: 2.0,
            swing,
            site: None,
        };
        assert!((working.seconds_left(Vec2::ZERO, &chop) - 0.5).abs() < 1e-5);
        assert_eq!(Task::Idle.seconds_left(Vec2::ZERO, &chop), 0.0);
    }
}
