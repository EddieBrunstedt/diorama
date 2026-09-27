//! What the mouse points at: a cell, never a mesh. The cursor's ray walks across the grid from
//! above, and the first cell whose column (terrain plus structure) it passes through is the one
//! it points at. Pointing at the top of a wall picks the wall's cell, not the ground behind it.

use crate::camera::{CameraRig, MainCamera};
use crate::dev::ScriptedCursor;
use crate::grid::{Grid, Structure, Terrain, cell_at};
use crate::map::terrain_height;
use crate::structures::structure_height;
use bevy::{prelude::*, window::PrimaryWindow};

/// Nothing on the map stands taller than this, so the ray walk starts where the ray comes down
/// through it. [`column_top`] checks it in debug builds.
const CEILING: f32 = 3.0;

pub struct PointerPlugin;

impl Plugin for PointerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Pointer>()
            .add_systems(Update, update_pointer.in_set(PointerSystems));
    }
}

/// Runs [`update_pointer`]. Anything reading [`Pointer`] should run after it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PointerSystems;

/// Where the cursor points this frame. All of it is `None` when the cursor is outside the window.
#[derive(Resource, Default, Debug)]
pub struct Pointer {
    /// The cursor's ray from the camera, without screen shake.
    pub ray: Option<Ray3d>,
    /// Where the ray hits the ground plane.
    pub ground: Option<Vec3>,
    /// The cell the cursor points at on the map as it is (it may be outside the map).
    pub cell: Option<IVec2>,
}

impl Pointer {
    /// The cell the cursor points at, given how tall each cell's column is.
    pub fn pick(&self, top: impl Fn(IVec2) -> f32) -> Option<IVec2> {
        self.ray.and_then(|ray| pick_cell(ray, CEILING, top))
    }
}

/// The cursor in logical pixels: the dev input script's fake cursor if one runs, else the real one.
pub fn cursor_position(window: &Window, scripted: Option<&ScriptedCursor>) -> Option<Vec2> {
    scripted
        .map(|cursor| cursor.0)
        .or_else(|| window.cursor_position())
}

/// How high the top of a cell's column is: its terrain, plus the structure standing on it.
pub fn cell_top(cell: IVec2, terrain: Terrain, structure: Option<Structure>) -> f32 {
    terrain_height(cell, terrain) + structure.map_or(0.0, structure_height)
}

/// How high the top of what stands on a cell is (0 outside the map).
pub fn column_top(grid: &Grid, cell: IVec2) -> f32 {
    let top = grid
        .cell(cell)
        .map_or(0.0, |data| cell_top(cell, data.terrain, data.structure));
    debug_assert!(
        top <= CEILING,
        "column at {cell} is above the picking ceiling"
    );
    top
}

/// The first cell whose column the ray passes through, walking from where the ray comes down
/// through `ceiling` to where it hits the ground. `top` gives each column's height (at most
/// `ceiling`, and 0 for bare ground or outside the map). If nothing stands in the way, that's the
/// cell on the ground under the ray. `None` if the ray never comes down to the ground.
pub fn pick_cell(ray: Ray3d, ceiling: f32, top: impl Fn(IVec2) -> f32) -> Option<IVec2> {
    let (origin, direction) = (ray.origin, *ray.direction);
    if direction.y >= 0.0 || origin.y <= 0.0 {
        return None;
    }
    // Distances along the ray: where the walk starts, and where it ends on the ground.
    let start = ((ceiling - origin.y) / direction.y).max(0.0);
    let ground = -origin.y / direction.y;

    // A DDA over the cells in the ray's shadow on the ground: from cell to cell, stepping across
    // whichever cell border (x or z) the ray reaches first.
    let mut cell = cell_at(ray.get_point(start));
    // Per axis: the step, how far along the ray the next border is, and how far apart they are.
    let axis = |position: f32, direction: f32, cell: i32| {
        if direction == 0.0 {
            return (0, f32::INFINITY, f32::INFINITY);
        }
        let border = if direction > 0.0 { cell + 1 } else { cell };
        let next = (border as f32 - position) / direction;
        (direction.signum() as i32, next, 1.0 / direction.abs())
    };
    let (step_x, mut next_x, delta_x) = axis(origin.x, direction.x, cell.x);
    let (step_z, mut next_z, delta_z) = axis(origin.z, direction.z, cell.y);
    loop {
        // The ray only goes down, so it's lowest where it leaves the cell.
        let leave = next_x.min(next_z).min(ground);
        if leave >= ground || origin.y + direction.y * leave <= top(cell) {
            return Some(cell);
        }
        if next_x < next_z {
            cell.x += step_x;
            next_x += delta_x;
        } else {
            cell.y += step_z;
            next_z += delta_z;
        }
    }
}

fn update_pointer(
    window: Single<&Window, With<PrimaryWindow>>,
    scripted: Option<Res<ScriptedCursor>>,
    camera: Single<&Camera, With<MainCamera>>,
    rig: Res<CameraRig>,
    grid: Res<Grid>,
    mut pointer: ResMut<Pointer>,
) {
    pointer.ray =
        cursor_position(&window, scripted.as_deref()).and_then(|cursor| rig.ray(&camera, cursor));
    pointer.ground = pointer
        .ray
        .and_then(|ray| ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)));
    pointer.cell = pointer.pick(|cell| column_top(&grid, cell));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;
    use bevy::platform::collections::HashMap;

    const WALL: f32 = 0.9;

    /// A ray coming down toward `-Z` at `pitch` degrees (like the camera's), through `point`.
    fn ray_through(point: Vec3, pitch: f32, sideways: f32) -> Ray3d {
        let pitch = pitch.to_radians();
        let back = Vec3::new(sideways, pitch.tan(), 1.0).normalize();
        Ray3d::new(point + back * 30.0, Dir3::new(-back).unwrap())
    }

    fn columns(list: &[((i32, i32), f32)]) -> impl Fn(IVec2) -> f32 {
        let map: HashMap<IVec2, f32> = list
            .iter()
            .map(|&((x, y), height)| (IVec2::new(x, y), height))
            .collect();
        move |cell| map.get(&cell).copied().unwrap_or(0.0)
    }

    fn pick(ray: Ray3d, top: impl Fn(IVec2) -> f32) -> Option<IVec2> {
        pick_cell(ray, CEILING, top)
    }

    #[test]
    fn bare_ground_picks_the_cell_under_the_ray() {
        let mut rng = Rng::new(3);
        for _ in 0..500 {
            let point = Vec3::new(rng.range(-20.0, 20.0), 0.0, rng.range(-20.0, 20.0));
            let ray = ray_through(point, rng.range(40.0, 89.0), rng.range(-0.5, 0.5));
            let ground = ray
                .plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))
                .unwrap();
            assert_eq!(pick(ray, |_| 0.0), Some(cell_at(ground)), "{ray:?}");
        }
    }

    #[test]
    fn the_back_of_a_walls_top_picks_the_wall() {
        let top = columns(&[((0, 0), WALL)]);
        // Steep and shallow rays, like the bottom and top of the screen.
        for pitch in [77.0, 47.0] {
            let back_edge = ray_through(Vec3::new(0.5, WALL, 0.02), pitch, 0.0);
            assert_eq!(pick(back_edge, &top), Some(IVec2::new(0, 0)), "{pitch}°");
            // The ground plane alone would have picked the cell behind.
            let ground = back_edge
                .plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))
                .unwrap();
            assert_eq!(cell_at(ground), IVec2::new(0, -1));
            // Just past the back edge, the ray misses the wall and finds the ground behind it.
            let past = ray_through(Vec3::new(0.5, WALL, -0.02), pitch, 0.0);
            assert_eq!(pick(past, &top), Some(IVec2::new(0, -1)), "{pitch}°");
        }
    }

    #[test]
    fn the_front_face_picks_the_wall() {
        let top = columns(&[((0, 0), WALL)]);
        let ray = ray_through(Vec3::new(0.3, 0.4, 0.999), 62.0, 0.0);
        assert_eq!(pick(ray, &top), Some(IVec2::new(0, 0)));
    }

    #[test]
    fn a_wall_in_front_hides_the_ground_behind_it() {
        let top = columns(&[((0, 0), WALL)]);
        // Aimed at the ground just behind the wall: the wall is in the way.
        let hidden = ray_through(Vec3::new(0.5, 0.0, -0.1), 62.0, 0.0);
        assert_eq!(pick(hidden, &top), Some(IVec2::new(0, 0)));
        // Far enough behind, the ground shows again.
        let shown = ray_through(Vec3::new(0.5, 0.0, -0.9), 62.0, 0.0);
        assert_eq!(pick(shown, &top), Some(IVec2::new(0, -1)));
    }

    #[test]
    fn rays_that_never_reach_the_ground_pick_nothing() {
        let up = Ray3d::new(Vec3::new(0.0, 5.0, 0.0), Dir3::Y);
        let level = Ray3d::new(Vec3::new(0.0, 5.0, 0.0), Dir3::NEG_Z);
        assert_eq!(pick(up, |_| 0.0), None);
        assert_eq!(pick(level, |_| 0.0), None);
    }

    /// The walk agrees with stepping along the ray in tiny steps, in every direction.
    #[test]
    fn matches_a_brute_force_march() {
        let mut rng = Rng::new(11);
        let heights: Vec<((i32, i32), f32)> = (-6..6)
            .flat_map(|y| (-6..6).map(move |x| (x, y)))
            .map(|cell| (cell, rng.pick(&[0.0, 0.0, 0.5, WALL, 2.0])))
            .collect();
        let top = columns(&heights);
        for _ in 0..200 {
            let target = Vec3::new(rng.range(-5.0, 5.0), 0.0, rng.range(-5.0, 5.0));
            let angle = rng.range(0.0, std::f32::consts::TAU);
            let slope = rng.range(0.3, 4.0);
            let back = Vec3::new(angle.cos(), slope, angle.sin()).normalize();
            let ray = Ray3d::new(target + back * 20.0, Dir3::new(-back).unwrap());
            let mut t = (CEILING - ray.origin.y) / ray.direction.y;
            let marched = loop {
                let point = ray.get_point(t);
                let cell = cell_at(point);
                if point.y <= top(cell) {
                    break cell;
                }
                t += 1e-4;
            };
            assert_eq!(pick(ray, &top), Some(marched), "{ray:?}");
        }
    }
}
