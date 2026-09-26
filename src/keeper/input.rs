//! Giving orders with the mouse: what's under the cursor, clicking to queue it, and a marker on
//! the island showing what a click would do.
//!
//! Click a tree to chop it, an empty meadow to build a tower there, anywhere else to walk there.
//! Shift-click puts the order right after the current one instead of at the end.

use super::{OrderKind, OrderQueue};
use crate::camera::DioramaCamera;
use crate::dev::ScriptedCursor;
use crate::island::{Island, TileKind, Tree};
use crate::juice::{Squash, Wobble};
use crate::palette;
use crate::towers::{BuildSite, Tower};
use crate::ui::{FlashOrder, Popup};
use crate::voxel::{TILE_SIZE, VoxelMesh};
use bevy::light::NotShadowCaster;
use bevy::picking::hover::HoverMap;
use bevy::picking::pointer::PointerId;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::collections::HashSet;

/// A tree's clickable box: half width and height, in world units at scale 1.
const TREE_HALF_WIDTH: f32 = 0.24;
const TREE_HEIGHT: f32 = 0.7;
/// The aim marker's size around a tree, relative to a whole tile.
const TREE_MARKER_SCALE: f32 = 0.42;

/// What's under the cursor, and the order a click would give.
#[derive(Resource, Default, Debug)]
pub struct Aim {
    pub target: Option<AimTarget>,
}

#[derive(Clone, Copy, Debug)]
pub struct AimTarget {
    pub kind: OrderKind,
    /// The spot on the island being aimed at.
    pub point: Vec3,
    pub tree: Option<Entity>,
}

/// The corner brackets showing what a click would target.
#[derive(Component)]
pub(super) struct AimMarker {
    materials: [Handle<StandardMaterial>; 3],
}

/// The ground order for a spot on the island: build on free meadow, otherwise walk.
/// `None` off the island.
pub fn ground_order(island: &Island, point: Vec2, occupied: &HashSet<IVec2>) -> Option<OrderKind> {
    let cell = (point / TILE_SIZE).round().as_ivec2();
    match island.get(cell)? {
        TileKind::Meadow if !occupied.contains(&cell) => Some(OrderKind::Build(cell)),
        _ => Some(OrderKind::Walk(point)),
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
    island: Res<Island>,
    trees: Query<(Entity, &Tree)>,
    sites: Query<&BuildSite>,
    towers: Query<&Tower>,
    mut aim: ResMut<Aim>,
) {
    aim.target = None;
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

    let tree_hit = trees
        .iter()
        .filter_map(|(entity, tree)| {
            let half = Vec3::new(TREE_HALF_WIDTH, 0.0, TREE_HALF_WIDTH) * tree.scale;
            let base = Vec3::new(tree.position.x, 0.0, tree.position.y);
            let top = Vec3::Y * TREE_HEIGHT * tree.scale;
            ray_box(ray.origin, *ray.direction, base - half, base + half + top)
                .map(|distance| (distance, entity, tree))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, entity, tree)) = tree_hit {
        aim.target = Some(AimTarget {
            kind: OrderKind::Chop(tree.id),
            point: Vec3::new(tree.position.x, 0.0, tree.position.y),
            tree: Some(entity),
        });
        return;
    }

    let Some(point) = ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))
    else {
        return;
    };
    let occupied: HashSet<IVec2> = sites
        .iter()
        .map(|site| site.cell)
        .chain(towers.iter().map(|tower| tower.cell))
        .collect();
    aim.target = ground_order(&island, point.xz(), &occupied).map(|kind| AimTarget {
        kind,
        point,
        tree: None,
    });
}

pub(super) fn give_orders(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    aim: Res<Aim>,
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

/// Four L-shaped corners around a tile, in voxels.
fn bracket_mesh() -> VoxelMesh {
    let mut mesh = VoxelMesh::default();
    const OUTER: f32 = 5.2;
    const ARM: f32 = 2.2;
    const THICK: f32 = 0.7;
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let corner = Vec3::new(sx * OUTER, 0.0, sz * OUTER);
        let inward_x = Vec3::new(-sx * ARM, 0.5, -sz * THICK);
        let inward_z = Vec3::new(-sx * THICK, 0.5, -sz * ARM);
        for inward in [inward_x, inward_z] {
            let other = corner + inward;
            mesh.cuboid(corner.min(other), corner.max(other), Color::WHITE);
        }
    }
    mesh
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
        OrderKind::Build(cell) => (cell.as_vec2() * TILE_SIZE, 1.0, 1),
        OrderKind::Walk(point) => (point, 0.3, 2),
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
        let mut occupied = HashSet::new();
        assert_eq!(
            ground_order(&island, Vec2::new(-1.1, 0.9), &occupied),
            Some(OrderKind::Build(IVec2::new(-1, 1)))
        );
        occupied.insert(IVec2::new(-1, 1));
        assert_eq!(
            ground_order(&island, Vec2::new(-1.1, 0.9), &occupied),
            Some(OrderKind::Walk(Vec2::new(-1.1, 0.9)))
        );
        assert!(matches!(
            ground_order(&island, Vec2::new(1.0, -1.0), &occupied),
            Some(OrderKind::Walk(_))
        ));
        assert_eq!(ground_order(&island, Vec2::new(4.0, 0.0), &occupied), None);
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
