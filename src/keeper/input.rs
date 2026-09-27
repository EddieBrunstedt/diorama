//! Giving orders with the mouse: what's under the cursor, clicking to queue it, and a marker on
//! the island showing what a click would do.
//!
//! Click a tree to chop it, an empty meadow to build there (a tower, or the catapult: see
//! [`BuildChoice`]), a hurt tower to repair it, the catapult to man it, anywhere else to walk
//! there. Shift-click puts the order right after the current one instead of at the end.
//!
//! While the keeper is in the catapult, a plain click works the catapult instead (see
//! [`crate::catapult`]); shift-click still gives orders, queued behind it.

use super::queue::{Building, Order};
use super::{OrderKind, OrderQueue};
use crate::camera::DioramaCamera;
use crate::catapult::{Catapult, Manned};
use crate::dev::ScriptedCursor;
use crate::island::{Island, TileKind, Tree};
use crate::juice::{Squash, Wobble};
use crate::palette;
use crate::phase::{GameState, keeper_working};
use crate::towers::{BuildSite, Tower};
use crate::ui::{FlashOrder, Popup};
use crate::voxel::{TILE_SIZE, bracket_mesh};
use bevy::light::NotShadowCaster;
use bevy::picking::hover::HoverMap;
use bevy::picking::pointer::PointerId;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::collections::HashMap;

/// A tree's clickable box: half width and height, in world units at scale 1.
const TREE_HALF_WIDTH: f32 = 0.24;
const TREE_HEIGHT: f32 = 0.7;
/// Clickable boxes of a tower and of the catapult (half width, height).
const TOWER_HALF_WIDTH: f32 = 0.36;
const TOWER_HEIGHT: f32 = 1.5;
const CATAPULT_HALF_WIDTH: f32 = 0.46;
const CATAPULT_HEIGHT: f32 = 0.75;
/// The aim marker's size around a tree, relative to a whole tile.
const TREE_MARKER_SCALE: f32 = 0.42;
pub const TOWER_KEY: KeyCode = KeyCode::KeyT;
pub const CATAPULT_KEY: KeyCode = KeyCode::KeyC;

/// What's under the cursor, and the order a click would give.
#[derive(Resource, Default, Debug)]
pub struct Aim {
    pub target: Option<AimTarget>,
    /// The spot on the ground plane under the cursor, unless it's over the UI.
    pub ground: Option<Vec3>,
}

/// What a click on a free meadow builds.
#[derive(Resource, Default, Debug)]
pub struct BuildChoice {
    pub building: Building,
    /// There's a catapult already (standing, going up or queued), and there's room for one.
    pub catapult_taken: bool,
}

/// Picks what to build (e.g. a build button was clicked).
#[derive(Message, Clone, Copy, Debug)]
pub struct ChooseBuilding(pub Building);

/// What stands on a cell, for deciding what a click there does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Occupant {
    Tower { damaged: bool },
    Catapult,
    Site(Building),
}

#[derive(Clone, Copy, Debug)]
pub struct AimTarget {
    pub kind: OrderKind,
    /// The spot on the island being aimed at.
    pub point: Vec3,
    pub tree: Option<Entity>,
}

/// The corner brackets showing what a click would target, tinted by the kind of order.
#[derive(Component)]
pub(super) struct AimMarker {
    materials: [Handle<StandardMaterial>; 5],
}

/// The ground order for a spot on the island: repair a hurt tower or man the catapult on it,
/// build `building` on a free meadow, otherwise walk. `None` off the island.
pub fn ground_order(
    island: &Island,
    point: Vec2,
    occupied: &HashMap<IVec2, Occupant>,
    building: Building,
) -> Option<OrderKind> {
    let cell = (point / TILE_SIZE).round().as_ivec2();
    let kind = island.get(cell)?;
    Some(match occupied.get(&cell) {
        Some(Occupant::Tower { damaged: true }) => OrderKind::Repair(cell),
        Some(Occupant::Catapult) => OrderKind::Man(cell),
        None if kind == TileKind::Meadow => OrderKind::Build(cell, building),
        _ => OrderKind::Walk(point),
    })
}

/// Whether there's a catapult on the island already: standing, going up, or planned.
pub fn catapult_taken<'a>(
    occupants: impl IntoIterator<Item = &'a Occupant>,
    orders: &[Order],
) -> bool {
    occupants.into_iter().any(|occupant| {
        matches!(
            occupant,
            Occupant::Catapult | Occupant::Site(Building::Catapult)
        )
    }) || orders
        .iter()
        .any(|order| matches!(order.kind, OrderKind::Build(_, Building::Catapult)))
}

/// Picks what a click on a free meadow builds: the build buttons, or T and C. After planning the
/// catapult it goes back to towers, since there's only room for one.
pub(super) fn choose_building(
    keys: Res<ButtonInput<KeyCode>>,
    mut chosen: MessageReader<ChooseBuilding>,
    mut choice: ResMut<BuildChoice>,
) {
    let mut wanted = chosen.read().last().map(|choose| choose.0);
    if keys.just_pressed(TOWER_KEY) {
        wanted = Some(Building::Tower);
    }
    if keys.just_pressed(CATAPULT_KEY) {
        wanted = Some(Building::Catapult);
    }
    if let Some(building) = wanted
        && choice.building != building
    {
        choice.building = building;
    }
}

/// The cursor in logical pixels: the dev input script's fake cursor if one runs, else the real one.
pub fn cursor_position(window: &Window, scripted: Option<&ScriptedCursor>) -> Option<Vec2> {
    scripted
        .map(|cursor| cursor.0)
        .or_else(|| window.cursor_position())
}

/// Where a ray enters an axis-aligned box, as a distance along the ray.
pub fn ray_box(origin: Vec3, direction: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let inverse = direction.recip();
    let (a, b) = ((min - origin) * inverse, (max - origin) * inverse);
    let near = a.min(b).max_element();
    let far = a.max(b).min_element();
    (near <= far && far >= 0.0).then_some(near.max(0.0))
}

pub(super) fn aim(
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<DioramaCamera>>,
    scripted: Option<Res<ScriptedCursor>>,
    hover: Res<HoverMap>,
    ui_nodes: Query<(), With<Node>>,
    keys: Res<ButtonInput<KeyCode>>,
    island: Res<Island>,
    queue: Res<OrderQueue>,
    trees: Query<(Entity, &Tree)>,
    sites: Query<&BuildSite>,
    towers: Query<&Tower>,
    catapults: Query<(&Catapult, Has<Manned>)>,
    state: Res<State<GameState>>,
    mut choice: ResMut<BuildChoice>,
    mut aim: ResMut<Aim>,
) {
    aim.target = None;
    aim.ground = None;
    let occupied: HashMap<IVec2, Occupant> = sites
        .iter()
        .map(|site| (site.cell, Occupant::Site(site.building)))
        .chain(towers.iter().map(|tower| {
            let damaged = tower.health.is_damaged();
            (tower.cell, Occupant::Tower { damaged })
        }))
        .chain(
            catapults
                .iter()
                .map(|(catapult, _)| (catapult.cell, Occupant::Catapult)),
        )
        .collect();
    let taken = catapult_taken(occupied.values(), queue.orders());
    if choice.catapult_taken != taken {
        choice.catapult_taken = taken;
    }
    if !keeper_working(*state.get()) {
        return;
    }
    // The hover map also holds the window itself, so only count UI nodes.
    let over_ui = hover
        .get(&PointerId::Mouse)
        .is_some_and(|hits| hits.keys().any(|&entity| ui_nodes.contains(entity)));
    let (camera, camera_transform) = *camera;
    let Some(ray) = cursor_position(&window, scripted.as_deref())
        .filter(|_| !over_ui)
        .and_then(|cursor| camera.viewport_to_world(camera_transform, cursor).ok())
    else {
        return;
    };
    aim.ground = ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y));
    // In the catapult, a plain click throws; shift-click still gives orders.
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if catapults.iter().any(|(_, manned)| manned) && !shift {
        return;
    }

    // Things standing up are hit before the ground behind them: trees, towers, the catapult.
    let column = |at: Vec2, half: f32, height: f32| {
        let base = Vec3::new(at.x, 0.0, at.y);
        let half = Vec3::new(half, 0.0, half);
        ray_box(
            ray.origin,
            *ray.direction,
            base - half,
            base + half + Vec3::Y * height,
        )
    };
    let tree_hits = trees.iter().filter_map(|(entity, tree)| {
        let distance = column(
            tree.position,
            TREE_HALF_WIDTH * tree.scale,
            TREE_HEIGHT * tree.scale,
        )?;
        let target = AimTarget {
            kind: OrderKind::Chop(tree.id),
            point: Vec3::new(tree.position.x, 0.0, tree.position.y),
            tree: Some(entity),
        };
        Some((distance, target))
    });
    let structure = |cell: IVec2, kind: OrderKind| AimTarget {
        kind,
        point: Vec3::new(cell.x as f32, 0.0, cell.y as f32) * TILE_SIZE,
        tree: None,
    };
    let tower_hits = towers.iter().filter_map(|tower| {
        let at = tower.cell.as_vec2() * TILE_SIZE;
        let distance = column(at, TOWER_HALF_WIDTH, TOWER_HEIGHT)?;
        let kind = if tower.health.is_damaged() {
            OrderKind::Repair(tower.cell)
        } else {
            OrderKind::Walk(at)
        };
        Some((distance, structure(tower.cell, kind)))
    });
    let catapult_hits = catapults.iter().filter_map(|(catapult, _)| {
        let at = catapult.cell.as_vec2() * TILE_SIZE;
        let distance = column(at, CATAPULT_HALF_WIDTH, CATAPULT_HEIGHT)?;
        Some((
            distance,
            structure(catapult.cell, OrderKind::Man(catapult.cell)),
        ))
    });
    let hit = tree_hits
        .chain(tower_hits)
        .chain(catapult_hits)
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, target)) = hit {
        aim.target = Some(target);
        return;
    }

    let Some(point) = aim.ground else {
        return;
    };
    aim.target =
        ground_order(&island, point.xz(), &occupied, choice.building).map(|kind| AimTarget {
            kind,
            point,
            tree: None,
        });
}

pub(super) fn give_orders(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    aim: Res<Aim>,
    mut choice: ResMut<BuildChoice>,
    mut queue: ResMut<OrderQueue>,
    trees: Query<(&Transform, Option<&Wobble>), With<Tree>>,
    mut marker: Query<&mut Squash, With<AimMarker>>,
    mut commands: Commands,
    mut flashes: MessageWriter<FlashOrder>,
    mut popups: MessageWriter<Popup>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(target) = aim.target else {
        return;
    };
    if let Some(existing) = queue.find_same_target(&target.kind) {
        flashes.write(FlashOrder(existing));
        return;
    }
    let catapult = matches!(target.kind, OrderKind::Build(_, Building::Catapult));
    if catapult && choice.catapult_taken {
        popups.write(Popup::problem(
            target.point + Vec3::Y * 0.3,
            "Only room for one catapult",
        ));
        return;
    }
    let next = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let added = if next {
        queue.push_front(target.kind)
    } else {
        queue.push(target.kind)
    };
    if added.is_none() {
        popups.write(Popup::problem(
            target.point + Vec3::Y * 0.3,
            "Too many orders",
        ));
        return;
    }
    if let Ok(mut squash) = marker.single_mut() {
        squash.pulse(0.35);
    }
    if catapult {
        choice.building = Building::Tower;
    }
    if let Some(tree) = target.tree
        && let Ok((transform, wobble)) = trees.get(tree)
    {
        commands
            .entity(tree)
            .insert(Wobble::new(wobble, transform, Vec3::X, 0.06));
    }
}

pub(super) fn spawn_aim_marker(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Mostly white so the brackets stand out on grass, tinted with the order's color.
    let mut material = |color: Color| {
        materials.add(StandardMaterial {
            base_color: Color::WHITE.mix(&color, 0.45),
            unlit: true,
            ..default()
        })
    };
    let materials = [
        material(palette::ORDER_CHOP),
        material(palette::ORDER_BUILD),
        material(palette::ORDER_WALK),
        material(palette::ORDER_REPAIR),
        material(palette::ORDER_CATAPULT),
    ];
    commands.spawn((
        AimMarker {
            materials: materials.clone(),
        },
        Mesh3d(meshes.add(bracket_mesh().build())),
        MeshMaterial3d(materials[0].clone()),
        Transform::default(),
        Squash::new(Vec3::ONE),
        Visibility::Hidden,
        NotShadowCaster,
    ));
}

pub(super) fn move_aim_marker(
    time: Res<Time>,
    aim: Res<Aim>,
    mut marker: Query<(
        &AimMarker,
        &mut Transform,
        &mut Squash,
        &mut Visibility,
        &mut MeshMaterial3d<StandardMaterial>,
    )>,
) {
    let Ok((marker, mut transform, mut squash, mut visibility, mut material)) = marker.single_mut()
    else {
        return;
    };
    let Some(target) = aim.target else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    let (at, size, color) = match target.kind {
        OrderKind::Chop(_) => (target.point.xz(), TREE_MARKER_SCALE, 0),
        OrderKind::Build(cell, _) => (cell.as_vec2() * TILE_SIZE, 1.0, 1),
        OrderKind::Walk(point) => (point, 0.3, 2),
        OrderKind::Repair(cell) => (cell.as_vec2() * TILE_SIZE, 1.0, 3),
        OrderKind::Man(cell) => (cell.as_vec2() * TILE_SIZE, 1.0, 4),
    };
    let appeared = *visibility == Visibility::Hidden;
    visibility.set_if_neq(Visibility::Inherited);
    let goal = Vec3::new(at.x, 0.02, at.y);
    // Glide between targets rather than jumping, but snap when (re)appearing.
    let blend = if appeared {
        1.0
    } else {
        (time.delta_secs() * 25.0).min(1.0)
    };
    transform.translation = transform.translation.lerp(goal, blend);
    let breathe = 1.0 + 0.04 * (time.elapsed_secs() * 5.0).sin();
    squash.rest = Vec3::splat(size * breathe);
    if material.0 != marker.materials[color] {
        material.0 = marker.materials[color].clone();
        squash.pulse(0.2);
    }
}

/// A hovered tree gives a little wiggle, so it's clear what a click would chop.
pub(super) fn wiggle_hovered_tree(
    mut commands: Commands,
    aim: Res<Aim>,
    trees: Query<(&Transform, Option<&Wobble>), With<Tree>>,
    mut last: Local<Option<Entity>>,
) {
    let hovered = aim.target.and_then(|target| target.tree);
    if hovered == *last {
        return;
    }
    *last = hovered;
    if let Some(tree) = hovered
        && let Ok((transform, wobble)) = trees.get(tree)
    {
        commands
            .entity(tree)
            .insert(Wobble::new(wobble, transform, Vec3::Z, 0.04));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_orders_build_on_free_meadow_and_walk_elsewhere() {
        let island = Island::starting();
        let mut occupied = HashMap::new();
        let tower = Building::Tower;
        let spot = Vec2::new(-1.1, 0.9);
        assert_eq!(
            ground_order(&island, spot, &occupied, tower),
            Some(OrderKind::tower(IVec2::new(-1, 1)))
        );
        assert_eq!(
            ground_order(&island, spot, &occupied, Building::Catapult),
            Some(OrderKind::Build(IVec2::new(-1, 1), Building::Catapult))
        );
        occupied.insert(IVec2::new(-1, 1), Occupant::Tower { damaged: false });
        assert_eq!(
            ground_order(&island, spot, &occupied, tower),
            Some(OrderKind::Walk(spot))
        );
        assert!(matches!(
            ground_order(&island, Vec2::new(1.0, -1.0), &occupied, tower),
            Some(OrderKind::Walk(_))
        ));
        assert_eq!(
            ground_order(&island, Vec2::new(4.0, 0.0), &occupied, tower),
            None
        );
    }

    #[test]
    fn hurt_towers_are_repaired_and_the_catapult_is_manned() {
        let island = Island::starting();
        let cell = IVec2::new(-1, 1);
        let spot = Vec2::new(-1.1, 0.9);
        let at = |occupant| {
            ground_order(
                &island,
                spot,
                &HashMap::from([(cell, occupant)]),
                Building::Tower,
            )
        };
        assert_eq!(
            at(Occupant::Tower { damaged: true }),
            Some(OrderKind::Repair(cell))
        );
        assert_eq!(at(Occupant::Catapult), Some(OrderKind::Man(cell)));
        assert_eq!(
            at(Occupant::Site(Building::Tower)),
            Some(OrderKind::Walk(spot))
        );
    }

    #[test]
    fn only_one_catapult() {
        let mut queue = OrderQueue::default();
        assert!(!catapult_taken(
            &[Occupant::Tower { damaged: false }],
            queue.orders()
        ));
        assert!(catapult_taken(&[Occupant::Catapult], queue.orders()));
        assert!(catapult_taken(
            &[Occupant::Site(Building::Catapult)],
            queue.orders()
        ));
        queue.push(OrderKind::Build(IVec2::ZERO, Building::Catapult));
        assert!(catapult_taken(&[], queue.orders()));
    }

    #[test]
    fn ray_box_hits_and_misses() {
        let (min, max) = (Vec3::new(-1.0, 0.0, -1.0), Vec3::new(1.0, 2.0, 1.0));
        let down = Vec3::NEG_Y;
        assert_eq!(ray_box(Vec3::new(0.0, 5.0, 0.0), down, min, max), Some(3.0));
        assert_eq!(ray_box(Vec3::new(3.0, 5.0, 0.0), down, min, max), None);
        assert_eq!(ray_box(Vec3::new(0.0, -1.0, 0.0), down, min, max), None);
    }
}
