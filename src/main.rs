use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Diorama".into(),
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .run();
}

/// Placeholder scene so we can see the engine running: a small voxel platform with one block on top.
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let grass = materials.add(Color::srgb(0.45, 0.75, 0.35));
    let wood = materials.add(Color::srgb(0.6, 0.4, 0.25));

    for x in -3..=3 {
        for z in -3..=3 {
            commands.spawn((
                Mesh3d(cube.clone()),
                MeshMaterial3d(grass.clone()),
                Transform::from_xyz(x as f32, 0.0, z as f32),
            ));
        }
    }
    commands.spawn((
        Mesh3d(cube),
        MeshMaterial3d(wood),
        Transform::from_xyz(0.0, 1.0, 0.0),
    ));

    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(8.0, 9.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
