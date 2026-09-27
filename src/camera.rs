//! The fixed diorama camera: an angled overview of the whole island, plus post-processing.

use crate::island::{Island, cell_to_world};
use crate::juice::Shake;
use crate::palette;
use crate::voxel::TILE_SIZE;
use bevy::{
    core_pipeline::tonemapping::Tonemapping,
    light::NotShadowCaster,
    post_process::bloom::Bloom,
    prelude::*,
    render::view::{ColorGrading, ColorGradingGlobal},
    window::PrimaryWindow,
};

/// Narrow field of view: less perspective distortion, more "miniature on a table".
const FOV_DEGREES: f32 = 28.0;
/// How far the camera looks down.
const PITCH_DEGREES: f32 = 40.0;
/// Rotation around the island (0 = looking north from the south).
const YAW_DEGREES: f32 = 32.0;
/// Camera distance as a multiple of the island's radius. Lower is closer.
/// Leaves room around the island for UI and for the island to grow.
const FRAMING: f32 = 6.6;
/// Seen from this angle the island is about as wide as it is tall on screen, so only windows
/// narrower than this move the camera back to keep it in view horizontally.
const MIN_ASPECT: f32 = 1.2;
/// Where the camera looks: a bit below the surface, so the floating rock is framed too.
const LOOK_AT: Vec3 = Vec3::new(0.0, -0.45, 0.0);
/// Distance of the sky backdrop from the camera.
const BACKDROP_DISTANCE: f32 = 200.0;
/// How quickly the camera eases to a new framing (per second, exponential), e.g. when the island
/// grows or UI covers part of the screen.
const REFRAME_RATE: f32 = 4.0;
/// Room (logical pixels) kept between the island and the window's edge or UI covering it.
const UI_MARGIN: f32 = 20.0;
/// How much bigger than the starting island (on screen) the island may grow before the camera
/// moves to keep it in view.
const GROWTH_ROOM: f32 = 1.3;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CoveredRight>()
            .add_systems(Startup, spawn_camera)
            .add_systems(Update, frame_island);
    }
}

/// The main camera, for turning the cursor into a ray and world points into screen points.
#[derive(Component)]
pub struct DioramaCamera;

/// How much of the screen's right side UI is covering (logical pixels), e.g. the tile offer.
/// The camera slides the island to the left to keep it clear, if there's room.
#[derive(Resource, Default)]
pub struct CoveredRight(pub f32);

fn spawn_camera(
    mut commands: Commands,
    island: Res<Island>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let fov = FOV_DEGREES.to_radians();
    let distance = camera_distance(&island, 16.0 / 9.0);
    let camera = commands
        .spawn((
            DioramaCamera,
            Camera3d::default(),
            Projection::Perspective(PerspectiveProjection { fov, ..default() }),
            camera_transform(distance),
            Tonemapping::TonyMcMapface,
            ColorGrading {
                global: ColorGradingGlobal {
                    post_saturation: 1.12,
                    ..default()
                },
                ..default()
            },
            Bloom {
                intensity: 0.18,
                ..Bloom::NATURAL
            },
            DistanceFog {
                color: palette::FOG,
                falloff: fog_falloff(distance),
                ..default()
            },
        ))
        .id();

    // A gradient sky behind everything, fixed to the camera.
    let height = 2.0 * BACKDROP_DISTANCE * (fov / 2.0).tan() * 1.2;
    commands.spawn((
        Mesh3d(meshes.add(sky_gradient(Vec2::new(height * 3.0, height)))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            fog_enabled: false,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, -BACKDROP_DISTANCE),
        NotShadowCaster,
        ChildOf(camera),
    ));
}

fn camera_distance(island: &Island, aspect: f32) -> f32 {
    island.world_radius() * FRAMING * (MIN_ASPECT / aspect).max(1.0)
}

fn camera_transform(distance: f32) -> Transform {
    let (yaw, pitch) = (YAW_DEGREES.to_radians(), PITCH_DEGREES.to_radians());
    let direction = Vec3::new(
        yaw.sin() * pitch.cos(),
        pitch.sin(),
        yaw.cos() * pitch.cos(),
    );
    Transform::from_translation(LOOK_AT + direction * distance).looking_at(LOOK_AT, Vec3::Y)
}

/// Fog starts just past the island, so clouds and the sky fade into haze but the island stays crisp.
fn fog_falloff(distance: f32) -> FogFalloff {
    FogFalloff::Linear {
        start: distance + 4.0,
        end: distance + 45.0,
    }
}

/// Keeps the island in view as it grows and when the window is resized, keeps it clear of UI
/// covering the right side, and shakes the camera.
///
/// The framing is tuned for the starting island, with room around it to grow. The island may
/// grow into that room (a bit past the starting island's size on screen) before the camera slides
/// or backs off to keep it all in view.
fn frame_island(
    time: Res<Time>,
    island: Res<Island>,
    shake: Res<Shake>,
    covered: Res<CoveredRight>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut camera: Single<(&Camera, &mut Transform, &mut DistanceFog), With<DioramaCamera>>,
    mut starting: Local<Option<Island>>,
    mut framed_at: Local<Option<f32>>,
    mut pan: Local<Vec2>,
) {
    let size = window.size();
    let aspect = size.x / size.y.max(1.0);
    if !aspect.is_finite() {
        return;
    }
    let starting = starting.get_or_insert_with(Island::starting);
    let (camera, transform, fog) = &mut *camera;
    let base = camera_distance(starting, aspect);
    let base_transform = camera_transform(base);

    // Where the island may be on screen: its starting size plus room to grow, inside the window
    // and clear of covering UI.
    let Some(start) = screen_bounds(camera, &base_transform, starting.tiles().map(|(c, _)| c))
    else {
        return;
    };
    let mut room = Rect::from_center_half_size(start.center(), start.half_size() * GROWTH_ROOM);
    room.min = room.min.max(Vec2::splat(UI_MARGIN));
    room.max = room
        .max
        .min(size - Vec2::new(covered.0 + UI_MARGIN, UI_MARGIN));
    // While placing a tile, the free spots around the island count too.
    let cells: Vec<IVec2> = if covered.0 > 0.0 {
        island
            .tiles()
            .map(|(cell, _)| cell)
            .chain(island.edge_cells())
            .collect()
    } else {
        island.tiles().map(|(cell, _)| cell).collect()
    };
    let Some(needed) = screen_bounds(camera, &base_transform, cells.iter().copied()) else {
        return;
    };
    // On screen, sizes shrink in proportion to the distance.
    let zoom = (needed.size() / room.size().max(Vec2::ONE))
        .max_element()
        .max(1.0);
    let wanted = base * zoom;

    let ease = 1.0 - (-REFRAME_RATE * time.delta_secs()).exp();
    let distance = match *framed_at {
        // Snap on the first frame and when the window is resized a lot; ease otherwise.
        Some(current) if (current - wanted).abs() < wanted * 0.5 => current.lerp(wanted, ease),
        _ => wanted,
    };
    let mut framed = camera_transform(distance);

    // Slide the island (by moving the camera the other way) just enough to bring it into its room.
    let wanted_pan =
        screen_bounds(camera, &framed, cells.iter().copied()).map_or(Vec2::ZERO, |at| {
            let push = |low: f32, high: f32, min: f32, max: f32| {
                if high > max {
                    (high - max).min(low - min).max(0.0)
                } else if low < min {
                    -(min - low).min(max - high).max(0.0)
                } else {
                    0.0
                }
            };
            Vec2::new(
                push(at.min.x, at.max.x, room.min.x, room.max.x),
                push(at.min.y, at.max.y, room.min.y, room.max.y),
            )
        });
    *pan = pan.lerp(wanted_pan, ease);
    let fov = FOV_DEGREES.to_radians();
    let world_per_pixel = 2.0 * distance * (fov / 2.0).tan() / size.y.max(1.0);
    // Screen y points down, camera y up.
    let offset = shake.offset(time.elapsed_secs()) + Vec2::new(pan.x, -pan.y) * world_per_pixel;
    framed.translation += framed.rotation * Vec3::new(offset.x, offset.y, 0.0);
    transform.set_if_neq(framed);
    if framed_at.is_none_or(|at| (at - distance).abs() > 0.01) {
        fog.falloff = fog_falloff(distance);
    }
    *framed_at = Some(distance);
}

/// The screen rectangle covering the surfaces of `cells`, seen from `transform`.
fn screen_bounds(
    camera: &Camera,
    transform: &Transform,
    cells: impl Iterator<Item = IVec2>,
) -> Option<Rect> {
    let global = GlobalTransform::from(*transform);
    let half = TILE_SIZE / 2.0;
    cells
        .flat_map(|cell| {
            let center = cell_to_world(cell);
            [(-half, -half), (half, -half), (-half, half), (half, half)]
                .map(|(x, z)| center + Vec3::new(x, 0.0, z))
        })
        .filter_map(|corner| camera.world_to_viewport(&global, corner).ok())
        .fold(None, |bounds: Option<Rect>, point| {
            Some(bounds.map_or(Rect::from_corners(point, point), |bounds| {
                bounds.union_point(point)
            }))
        })
}

/// A quad with a vertical color gradient from the horizon color up to the sky color.
fn sky_gradient(size: Vec2) -> Mesh {
    let mut mesh = Mesh::from(Rectangle::from_size(size));
    let top = palette::SKY_TOP.to_linear().to_f32_array();
    let bottom = palette::SKY_HORIZON.to_linear().to_f32_array();
    let colors: Vec<[f32; 4]> = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(|positions| positions.as_float3())
        .expect("rectangle mesh has positions")
        .iter()
        .map(|p| if p[1] > 0.0 { top } else { bottom })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh
}
