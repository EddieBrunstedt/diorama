//! The overview camera: steep perspective, looking straight up the map so grid rows stay level
//! on screen. It frames the whole map at the start, and keeps it framed as the window changes
//! size (e.g. when a tiling window manager resizes it) until the player moves the camera. When
//! the map grows, or new ground is offered, it glides out to show it ([`CameraRig::show`]).
//!
//! Pan with WASD or the arrow keys, or drag with the middle mouse button (the ground under the
//! cursor sticks to it). Scroll to zoom toward the cursor.

use crate::dev::ScriptedCursor;
use crate::grid::Grid;
use crate::juice::Shake;
use crate::pointer::{PointerSystems, cursor_position};
use crate::tuning::camera::*;
use crate::tuning::juice::SHAKE_MAX_OFFSET;
use bevy::{
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
    window::PrimaryWindow,
};

/// Scroll "lines" per pixel, for touchpads that scroll in pixels.
const PIXELS_PER_LINE: f32 = 40.0;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraRig>()
            .add_systems(Startup, spawn_camera)
            .add_systems(
                Update,
                (control_camera, place_camera)
                    .chain()
                    .before(PointerSystems),
            );
    }
}

#[derive(Component)]
pub struct MainCamera;

/// Where the camera looks and from how far. The camera's transform is this plus screen shake;
/// picking uses this, so the cell under the cursor doesn't jitter with the shake.
#[derive(Resource, Clone, Copy, Debug)]
pub struct CameraRig {
    /// The ground point in the middle of the view (world x and z).
    pub focus: Vec2,
    pub distance: f32,
    /// The distance the zoom is easing toward.
    pub target_distance: f32,
    /// Where a middle-button drag grabbed the ground.
    grab: Option<Vec3>,
    /// The map plus some room around it: the focus stays inside (world x and z).
    limits: Rect,
    /// The map area to keep framed, and the window aspect it was last framed for. Cleared once
    /// the player pans or zooms.
    framing: Option<(Rect, f32)>,
    /// Whether the next framing glides there rather than jumping.
    glide_next: bool,
    /// The focus and distance the camera is gliding to, if it's on its way.
    glide: Option<(Vec2, f32)>,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            focus: Vec2::ZERO,
            distance: MAX_DISTANCE,
            target_distance: MAX_DISTANCE,
            grab: None,
            limits: Rect::from_center_half_size(Vec2::ZERO, Vec2::splat(f32::MAX)),
            framing: None,
            glide_next: false,
            glide: None,
        }
    }
}

impl CameraRig {
    /// The camera's transform without shake.
    pub fn transform(&self) -> Transform {
        let pitch = PITCH_DEGREES.to_radians();
        let focus = Vec3::new(self.focus.x, 0.0, self.focus.y);
        let back = Vec3::new(0.0, pitch.sin(), pitch.cos());
        Transform::from_translation(focus + back * self.distance).looking_at(focus, Vec3::Y)
    }

    /// The ray from the camera (without shake) through a screen position.
    pub fn ray(&self, camera: &Camera, screen: Vec2) -> Option<Ray3d> {
        camera
            .viewport_to_world(&GlobalTransform::from(self.transform()), screen)
            .ok()
    }

    /// The map now covers `area` (world x and z): the camera may look around all of it, and if
    /// it's still framing the map, it glides to frame the new area. With `take_over`, it does so
    /// even if the player has moved the camera.
    pub fn show(&mut self, area: Rect, take_over: bool) {
        self.limits = area.inflate(PAN_OVERSCROLL);
        if take_over || self.framing.is_some() {
            self.framing = Some((area, f32::NAN));
            self.glide_next = true;
        }
    }

    /// The player moved the camera: it stops framing the map.
    fn take_control(&mut self) {
        self.framing = None;
        self.glide = None;
    }

    /// The ground point under a screen position.
    pub fn ground_point(&self, camera: &Camera, screen: Vec2) -> Option<Vec3> {
        self.ray(camera, screen)?
            .plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))
    }
}

/// The closest distance at which the whole map fits in view, looking at its middle.
fn framing_distance(map: Rect, aspect: f32) -> f32 {
    let tan = (FOV_DEGREES.to_radians() / 2.0).tan() / FRAMING_MARGIN;
    let corners = [
        map.min,
        Vec2::new(map.min.x, map.max.y),
        Vec2::new(map.max.x, map.min.y),
        map.max,
    ];
    let fits = |distance: f32| {
        let rig = CameraRig {
            focus: map.center(),
            distance,
            ..default()
        };
        let view = rig.transform().to_matrix().inverse();
        corners.iter().all(|corner| {
            let p = view.transform_point3(Vec3::new(corner.x, 0.0, corner.y));
            let depth = -p.z;
            depth > 0.0 && (p.x / depth).abs() <= tan * aspect && (p.y / depth).abs() <= tan
        })
    };
    if fits(MIN_DISTANCE) {
        return MIN_DISTANCE;
    }
    let (mut low, mut high) = (MIN_DISTANCE, MAX_DISTANCE);
    for _ in 0..24 {
        let middle = (low + high) / 2.0;
        if fits(middle) {
            high = middle;
        } else {
            low = middle;
        }
    }
    high
}

/// The window's width over its height.
fn aspect(window: &Window) -> f32 {
    window.width() / window.height().max(1.0)
}

/// Frames `map` for the window's current shape, if that changed since it was last framed.
fn keep_framed(rig: &mut CameraRig, window: &Window) {
    let Some((map, framed_for)) = rig.framing else {
        return;
    };
    let aspect = aspect(window);
    if aspect == framed_for {
        return;
    }
    rig.framing = Some((map, aspect));
    let distance = framing_distance(map, aspect);
    if std::mem::take(&mut rig.glide_next) {
        rig.glide = Some((map.center(), distance));
    } else {
        rig.glide = None;
        rig.focus = map.center();
        rig.distance = distance;
        rig.target_distance = distance;
    }
}

/// Eases the camera along its glide, if it's on one.
fn glide(rig: &mut CameraRig, dt: f32) {
    let Some((focus, distance)) = rig.glide else {
        return;
    };
    let ease = 1.0 - (-GLIDE_RATE * dt).exp();
    rig.focus = rig.focus.lerp(focus, ease);
    rig.distance = rig.distance.lerp(distance, ease);
    rig.target_distance = rig.distance;
    if rig.focus.distance(focus) < 0.01 && (rig.distance - distance).abs() < 0.01 {
        rig.focus = focus;
        rig.distance = distance;
        rig.target_distance = distance;
        rig.glide = None;
    }
}

fn spawn_camera(
    mut commands: Commands,
    grid: Res<Grid>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut rig: ResMut<CameraRig>,
) {
    if let Some(bounds) = grid.bounds() {
        let min = bounds.min.as_vec2();
        let max = (bounds.max + IVec2::ONE).as_vec2();
        let map = Rect::from_corners(min, max);
        rig.limits = map.inflate(PAN_OVERSCROLL);
        rig.framing = Some((map, f32::NAN));
        keep_framed(&mut rig, &window);
    }
    commands.spawn((
        MainCamera,
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: FOV_DEGREES.to_radians(),
            ..default()
        }),
        rig.transform(),
    ));
}

fn control_camera(
    time: Res<Time<Real>>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    window: Single<&Window, With<PrimaryWindow>>,
    scripted: Option<Res<ScriptedCursor>>,
    camera: Single<&Camera, With<MainCamera>>,
    mut rig: ResMut<CameraRig>,
) {
    // Real time, so the camera moves the same during hit-stop or a dev speed-up.
    let dt = time.delta_secs();
    let cursor = cursor_position(&window, scripted.as_deref());

    // Keyboard pan. Holding Ctrl leaves the keys to shortcuts (Ctrl+Z is not "pan left").
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let mut pan = Vec2::ZERO;
    if !ctrl {
        for (key, arrow, direction) in [
            (KeyCode::KeyW, KeyCode::ArrowUp, Vec2::NEG_Y),
            (KeyCode::KeyS, KeyCode::ArrowDown, Vec2::Y),
            (KeyCode::KeyA, KeyCode::ArrowLeft, Vec2::NEG_X),
            (KeyCode::KeyD, KeyCode::ArrowRight, Vec2::X),
        ] {
            if keys.any_pressed([key, arrow]) {
                pan += direction;
            }
        }
    }
    let speed = PAN_SPEED * rig.distance;
    rig.focus += pan.normalize_or_zero() * speed * dt;
    if pan != Vec2::ZERO {
        rig.take_control();
    }

    // Zoom, keeping the ground under the cursor where it is.
    let lines = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / PIXELS_PER_LINE,
    };
    if lines != 0.0 {
        rig.take_control();
        rig.target_distance =
            (rig.target_distance * (1.0 - ZOOM_STEP).powf(lines)).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }
    if rig.distance != rig.target_distance {
        let before = cursor.and_then(|cursor| rig.ground_point(&camera, cursor));
        let ease = 1.0 - (-ZOOM_EASE_RATE * dt).exp();
        rig.distance = rig.distance.lerp(rig.target_distance, ease);
        if (rig.distance - rig.target_distance).abs() < 0.01 {
            rig.distance = rig.target_distance;
        }
        let after = cursor.and_then(|cursor| rig.ground_point(&camera, cursor));
        if let (Some(before), Some(after)) = (before, after) {
            rig.focus += (before - after).xz();
        }
    }

    // Middle-button drag: move the camera so the grabbed ground point stays under the cursor.
    if buttons.just_pressed(MouseButton::Middle) {
        rig.take_control();
        rig.grab = cursor.and_then(|cursor| rig.ground_point(&camera, cursor));
    }
    if !buttons.pressed(MouseButton::Middle) {
        rig.grab = None;
    }
    if let Some(grab) = rig.grab
        && let Some(under) = cursor.and_then(|cursor| rig.ground_point(&camera, cursor))
    {
        rig.focus += (grab - under).xz();
    }

    // Until the player takes over, the map stays framed whatever the window does.
    keep_framed(&mut rig, &window);
    glide(&mut rig, dt);

    let limits = rig.limits;
    rig.focus = rig.focus.clamp(limits.min, limits.max);
}

/// Puts the camera where the rig says, plus shake.
fn place_camera(
    time: Res<Time<Real>>,
    rig: Res<CameraRig>,
    shake: Res<Shake>,
    mut transform: Single<&mut Transform, With<MainCamera>>,
) {
    let mut placed = rig.transform();
    let offset = shake.offset(time.elapsed_secs()) * SHAKE_MAX_OFFSET * rig.distance;
    placed.translation += placed.rotation * offset.extend(0.0);
    transform.set_if_neq(placed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_backs_off_for_bigger_maps_and_narrow_windows() {
        let square = |size| Rect::from_center_size(Vec2::ZERO, Vec2::splat(size));
        let small = framing_distance(square(20.0), 16.0 / 9.0);
        let big = framing_distance(square(40.0), 16.0 / 9.0);
        let narrow = framing_distance(square(20.0), 0.5);
        assert!(big > small);
        assert!(narrow > small);
    }

    #[test]
    fn the_rig_looks_at_its_focus() {
        let rig = CameraRig {
            focus: Vec2::new(3.0, -2.0),
            distance: 30.0,
            ..default()
        };
        let transform = rig.transform();
        let ray = Ray3d::new(transform.translation, transform.forward());
        let hit = ray
            .plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))
            .unwrap();
        assert!(hit.xz().distance(rig.focus) < 1e-3);
        assert!((transform.translation.distance(hit) - 30.0).abs() < 1e-3);
    }
}
