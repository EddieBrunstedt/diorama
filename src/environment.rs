//! Sky, sunlight and clouds: the warm late-afternoon mood around the island.

use crate::palette;
use crate::rng::Rng;
use crate::voxel::{VoxelMesh, vary};
use bevy::{
    light::{CascadeShadowConfigBuilder, DirectionalLightShadowMap, NotShadowCaster},
    prelude::*,
};

const SUN_ILLUMINANCE: f32 = 10_000.0;
const SKY_BRIGHTNESS: f32 = 480.0;
/// Clouds drifting underneath the island.
const LOW_CLOUDS: usize = 9;
/// Clouds far out near the horizon, behind the island.
const FAR_CLOUDS: usize = 7;

pub struct EnvironmentPlugin;

impl Plugin for EnvironmentPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(palette::SKY_HORIZON))
            .insert_resource(GlobalAmbientLight {
                color: palette::SKYLIGHT,
                brightness: SKY_BRIGHTNESS,
                ..default()
            })
            .insert_resource(DirectionalLightShadowMap { size: 4096 })
            .init_resource::<CloudMaterial>()
            .add_systems(Startup, (spawn_sun, spawn_clouds))
            .add_systems(Update, drift_clouds);
    }
}

fn spawn_sun(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            color: palette::SUNLIGHT,
            illuminance: SUN_ILLUMINANCE,
            shadow_maps_enabled: true,
            ..default()
        },
        // Low from the south-west: long shadows, lit faces toward the camera, shaded east faces.
        Transform::from_xyz(-5.0, 6.5, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
        // The island is small, so one tight cascade gives the crispest shadows.
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            minimum_distance: 0.1,
            maximum_distance: 40.0,
            ..default()
        }
        .build(),
    ));
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
            emissive: LinearRgba::rgb(0.35, 0.33, 0.36),
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
