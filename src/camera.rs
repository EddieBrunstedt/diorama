//! The fixed diorama camera: an angled overview of the whole island, plus post-processing.

use crate::island::Island;
use crate::palette;
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

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(Update, frame_island);
    }
}

/// The main camera, for turning the cursor into a ray and world points into screen points.
#[derive(Component)]
pub struct DioramaCamera;

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

/// Keeps the whole island in view when the window is resized.
fn frame_island(
    island: Res<Island>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut camera: Single<(&mut Transform, &mut DistanceFog), With<DioramaCamera>>,
) {
    let aspect = window.width() / window.height().max(1.0);
    if !aspect.is_finite() {
        return;
    }
    let distance = camera_distance(&island, aspect);
    let (transform, fog) = &mut *camera;
    if transform.set_if_neq(camera_transform(distance)) {
        fog.falloff = fog_falloff(distance);
    }
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
