//! Sky, sunlight, clouds and haze around the island, lit in the current mood (see
//! [`crate::mood`]).

use crate::camera::{DioramaCamera, Techniques};
use crate::mood::{CurrentMood, Mood, MoodSystems};
use crate::palette;
use crate::rng::Rng;
use crate::voxel::{VoxelMesh, vary};
use bevy::{
    light::{
        CascadeShadowConfigBuilder, DirectionalLightShadowMap, FogVolume, NotShadowCaster,
        VolumetricLight,
    },
    prelude::*,
};

/// Clouds drifting underneath the island.
const LOW_CLOUDS: usize = 9;
/// Clouds far out near the horizon, behind the island.
const FAR_CLOUDS: usize = 7;
/// Distance of the sky backdrop from the camera.
const BACKDROP_DISTANCE: f32 = 200.0;

pub struct EnvironmentPlugin;

impl Plugin for EnvironmentPlugin {
    fn build(&self, app: &mut App) {
        let calm = Mood::CALM;
        app.insert_resource(ClearColor(calm.sky_horizon))
            .insert_resource(GlobalAmbientLight {
                color: calm.skylight,
                brightness: calm.skylight_brightness,
                ..default()
            })
            .insert_resource(DirectionalLightShadowMap { size: 4096 })
            .init_resource::<CloudMaterial>()
            .add_systems(Startup, (spawn_sun, spawn_clouds, spawn_haze))
            .add_systems(
                Update,
                (
                    spawn_sky,
                    drift_clouds,
                    (light_sky, color_sky).after(MoodSystems),
                ),
            );
    }
}

#[derive(Component)]
struct Sun;

fn spawn_sun(mut commands: Commands, techniques: Res<Techniques>) {
    let calm = Mood::CALM;
    let mut sun = commands.spawn((
        Sun,
        DirectionalLight {
            color: calm.sun,
            illuminance: calm.sun_illuminance,
            shadow_maps_enabled: true,
            contact_shadows_enabled: techniques.contact_shadows,
            ..default()
        },
        Transform::default().looking_to(calm.sun_direction(), Vec3::Y),
        // The island is small, so one tight cascade gives the crispest shadows.
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            minimum_distance: 0.1,
            maximum_distance: 40.0,
            ..default()
        }
        .build(),
    ));
    if techniques.volumetric {
        sun.insert(VolumetricLight);
    }
}

/// The haze the light shafts shine through.
#[derive(Component)]
struct Haze;

/// A layer of haze around the island that the sun and the hearth shine through in shafts.
fn spawn_haze(mut commands: Commands, techniques: Res<Techniques>) {
    if !techniques.volumetric {
        return;
    }
    commands.spawn((
        Haze,
        FogVolume {
            density_factor: Mood::CALM.haze,
            ..default()
        },
        Transform::from_xyz(0.0, -3.0, 0.0).with_scale(Vec3::new(18.0, 9.0, 18.0)),
    ));
}

/// The sun, the skylight, the haze and the clouds' glow follow the mood.
fn light_sky(
    current: Res<CurrentMood>,
    mut sun: Single<(&mut DirectionalLight, &mut Transform), With<Sun>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
    mut haze: Query<&mut FogVolume, With<Haze>>,
    clouds: Res<CloudMaterial>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !current.is_changed() {
        return;
    }
    let mood = &current.mood;
    let (light, transform) = &mut *sun;
    light.color = mood.sun;
    light.illuminance = mood.sun_illuminance;
    **transform = Transform::default().looking_to(mood.sun_direction(), Vec3::Y);
    ambient.color = mood.skylight;
    ambient.brightness = mood.skylight_brightness;
    clear.0 = mood.sky_horizon;
    for mut volume in &mut haze {
        volume.density_factor = mood.haze;
    }
    if let Some(mut material) = materials.get_mut(&clouds.0) {
        material.emissive = mood.cloud_glow.to_linear();
    }
}

/// The gradient sky behind everything, fixed to the camera.
#[derive(Component)]
struct Sky(Handle<Mesh>);

fn spawn_sky(
    mut commands: Commands,
    cameras: Query<(Entity, &Projection), Added<DioramaCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (camera, projection) in &cameras {
        let Projection::Perspective(perspective) = projection else {
            continue;
        };
        let height = 2.0 * BACKDROP_DISTANCE * (perspective.fov / 2.0).tan() * 1.2;
        let calm = Mood::CALM;
        let mesh = meshes.add(sky_gradient(
            Vec2::new(height * 3.0, height),
            calm.sky_top,
            calm.sky_horizon,
        ));
        commands.spawn((
            Sky(mesh.clone()),
            Mesh3d(mesh),
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
}

fn color_sky(current: Res<CurrentMood>, skies: Query<&Sky>, mut meshes: ResMut<Assets<Mesh>>) {
    if !current.is_changed() {
        return;
    }
    for sky in &skies {
        if let Some(mut mesh) = meshes.get_mut(&sky.0) {
            paint_gradient(&mut mesh, current.mood.sky_top, current.mood.sky_horizon);
        }
    }
}

/// A quad with a vertical color gradient from the horizon color up to the sky color.
fn sky_gradient(size: Vec2, top: Color, horizon: Color) -> Mesh {
    let mut mesh = Mesh::from(Rectangle::from_size(size));
    paint_gradient(&mut mesh, top, horizon);
    mesh
}

fn paint_gradient(mesh: &mut Mesh, top: Color, horizon: Color) {
    let top = top.to_linear().to_f32_array();
    let bottom = horizon.to_linear().to_f32_array();
    let colors: Vec<[f32; 4]> = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(|positions| positions.as_float3())
        .expect("rectangle mesh has positions")
        .iter()
        .map(|p| if p[1] > 0.0 { top } else { bottom })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
}

/// A voxel cloud slowly circling the island below its rim.
#[derive(Component)]
struct Cloud {
    radius: f32,
    angle: f32,
    height: f32,
    speed: f32,
}

/// Clouds glow a little so they stay bright white instead of taking on the fog's color.
#[derive(Resource)]
struct CloudMaterial(Handle<StandardMaterial>);

impl FromWorld for CloudMaterial {
    fn from_world(world: &mut World) -> Self {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        Self(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            emissive: Mood::CALM.cloud_glow.to_linear(),
            perceptual_roughness: 1.0,
            reflectance: 0.0,
            ..default()
        }))
    }
}

fn spawn_clouds(
    mut commands: Commands,
    material: Res<CloudMaterial>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let mut rng = Rng::new(0xC10D);
    let low = (0..LOW_CLOUDS).map(|k| (k, LOW_CLOUDS, 1.5..9.0, -8.0..-4.0));
    let far = (0..FAR_CLOUDS).map(|k| (k, FAR_CLOUDS, 16.0..26.0, -3.0..0.5));
    for (k, count, radius, height) in low.chain(far) {
        let cloud = Cloud {
            radius: rng.range(radius.start, radius.end),
            angle: k as f32 / count as f32 * std::f32::consts::TAU + rng.range(-0.3, 0.3),
            height: rng.range(height.start, height.end),
            speed: rng.range(0.012, 0.025),
        };
        commands.spawn((
            Mesh3d(meshes.add(cloud_mesh(&mut rng).build())),
            MeshMaterial3d(material.0.clone()),
            cloud_transform(&cloud, 0.0),
            cloud,
            NotShadowCaster,
        ));
    }
}

/// A flat-bottomed, lumpy-topped cloud made of a few overlapping slabs.
fn cloud_mesh(rng: &mut Rng) -> VoxelMesh {
    let mut mesh = VoxelMesh::default();
    let puffs = 4 + rng.index(4);
    let length = rng.range(14.0, 26.0);
    for k in 0..puffs {
        let along = (k as f32 / (puffs - 1) as f32 - 0.5) * length;
        // Taller in the middle, lower toward the ends.
        let middle = 1.0 - (2.0 * along / length).abs();
        let half = Vec3::new(
            rng.range(3.0, 5.0),
            1.0 + middle * rng.range(1.5, 3.0),
            rng.range(2.5, 4.5),
        )
        .round();
        let center = Vec3::new(along.round(), half.y, rng.range(-1.5, 1.5).round());
        let top = vary(palette::CLOUD, rng, 0.01, 4.0);
        mesh.cuboid_graded(center - half, center + half, top, palette::CLOUD_SHADE);
    }
    mesh
}

fn cloud_transform(cloud: &Cloud, time: f32) -> Transform {
    let angle = cloud.angle + cloud.speed * time;
    let bob = (time * 0.4 + cloud.angle * 3.0).sin() * 0.08;
    Transform::from_xyz(
        angle.cos() * cloud.radius,
        cloud.height + bob,
        angle.sin() * cloud.radius,
    )
    // Keep clouds lined up with their direction of travel.
    .with_rotation(Quat::from_rotation_y(-angle))
}

fn drift_clouds(time: Res<Time>, mut clouds: Query<(&Cloud, &mut Transform)>) {
    for (cloud, mut transform) in &mut clouds {
        *transform = cloud_transform(cloud, time.elapsed_secs());
    }
}
