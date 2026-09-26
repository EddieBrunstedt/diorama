//! The hearth: the island's heart and the player's base. A stone fire pit with a crackling fire.

use crate::island::{Tile, TileKind};
use crate::juice::Tween;
use crate::palette;
use crate::rng::Rng;
use crate::voxel::{VOXEL, VoxelMaterial, VoxelMesh, shade, vary};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

/// How long after its tile lands the hearth pops up.
const POP_DELAY: f32 = 0.05;
/// The hearth is modeled at tile scale, then enlarged a bit so it stands out as the island's heart.
const SCALE: f32 = 1.15;
const FIRELIGHT_INTENSITY: f32 = 120_000.0;
/// Seconds between embers rising from the fire.
const EMBER_INTERVAL: f32 = 0.14;

const MODEL_SALT: u64 = 5;

pub struct HearthPlugin;

impl Plugin for HearthPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VoxelMaterial>()
            .init_resource::<FireAssets>()
            .add_systems(Update, (spawn_hearth, flicker, spawn_embers, rise_embers));
    }
}

/// The root of the hearth's visuals, a child of the hearth tile.
#[derive(Component)]
pub struct Hearth;

/// A voxel flame that flickers around its resting shape.
#[derive(Component)]
struct Flame {
    rest: Vec3,
    size: Vec3,
    phase: f32,
}

#[derive(Component)]
struct Firelight;

#[derive(Component)]
struct Ember {
    age: f32,
    lifetime: f32,
    size: f32,
    drift: f32,
}

#[derive(Resource)]
struct FireAssets {
    cube: Handle<Mesh>,
    outer: Handle<StandardMaterial>,
    core: Handle<StandardMaterial>,
    ember: Handle<StandardMaterial>,
}

impl FromWorld for FireAssets {
    fn from_world(world: &mut World) -> Self {
        let cube = world
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::from_length(1.0));
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        // Black base color so sunlight doesn't wash the fire out to pastel: all color is glow.
        let mut glowing = |color: Color, strength: f32| {
            materials.add(StandardMaterial {
                base_color: Color::BLACK,
                emissive: color.to_linear() * strength,
                ..default()
            })
        };
        Self {
            cube,
            outer: glowing(palette::FLAME_OUTER, 1.2),
            core: glowing(palette::FLAME_CORE, 2.1),
            ember: glowing(palette::EMBER_BED, 1.2),
        }
    }
}

fn spawn_hearth(
    mut commands: Commands,
    tiles: Query<(Entity, &Tile, Option<&Tween>), Added<Tile>>,
    fire: Res<FireAssets>,
    material: Res<VoxelMaterial>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (tile, _, drop) in tiles
        .iter()
        .filter(|(_, tile, _)| tile.kind == TileKind::Hearth)
    {
        let delay = drop.map_or(0.0, Tween::remaining) + POP_DELAY;
        let pop = Tween::pop_in(Vec3::splat(SCALE), delay);
        let hearth = commands
            .spawn((
                Hearth,
                Mesh3d(meshes.add(hearth_mesh().build())),
                MeshMaterial3d(material.0.clone()),
                pop.start_transform(),
                Visibility::Hidden,
                pop,
                ChildOf(tile),
            ))
            .id();

        // Glowing bed of embers inside the ring.
        commands.spawn((
            Mesh3d(fire.cube.clone()),
            MeshMaterial3d(fire.ember.clone()),
            Transform::from_translation(Vec3::new(0.0, PLATFORM_TOP + 0.3, 0.0) * VOXEL)
                .with_scale(Vec3::new(4.0, 0.6, 4.0) * VOXEL),
            NotShadowCaster,
            ChildOf(hearth),
        ));

        // (center, size, is core) in voxels, above the platform.
        let flames = [
            (Vec3::new(0.0, 3.0, 0.0), Vec3::new(3.0, 2.4, 3.0), false),
            (Vec3::new(0.2, 4.4, -0.1), Vec3::new(2.0, 2.4, 2.0), true),
            (Vec3::new(-0.1, 6.0, 0.2), Vec3::new(1.2, 1.8, 1.2), true),
            (Vec3::new(0.3, 7.4, -0.2), Vec3::new(0.5, 0.8, 0.5), true),
            (Vec3::new(1.3, 3.8, 1.0), Vec3::new(1.0, 1.5, 1.0), false),
            (Vec3::new(-1.2, 3.6, -1.1), Vec3::new(0.9, 1.3, 0.9), false),
        ];
        for (k, (center, size, core)) in flames.into_iter().enumerate() {
            let flame = Flame {
                rest: (center + Vec3::Y * PLATFORM_TOP) * VOXEL,
                size: size * VOXEL,
                phase: k as f32 * 1.7,
            };
            commands.spawn((
                Mesh3d(fire.cube.clone()),
                MeshMaterial3d(if core {
                    fire.core.clone()
                } else {
                    fire.outer.clone()
                }),
                Transform::from_translation(flame.rest).with_scale(flame.size),
                flame,
                NotShadowCaster,
                ChildOf(hearth),
            ));
        }

        commands.spawn((
            Firelight,
            PointLight {
                color: palette::FIRELIGHT,
                intensity: FIRELIGHT_INTENSITY,
                range: 6.0,
                radius: 0.1,
                shadow_maps_enabled: true,
                ..default()
            },
            Transform::from_xyz(0.0, (PLATFORM_TOP + 8.0) * VOXEL, 0.0),
            ChildOf(hearth),
        ));
    }
}

/// Height of the raised platform the fire pit sits on (voxels).
const PLATFORM_TOP: f32 = 1.2;

/// A raised stone platform with a fire pit, logs and a woodpile, in voxels around the tile center.
fn hearth_mesh() -> VoxelMesh {
    let mut rng = Rng::new(MODEL_SALT);
    let mut mesh = VoxelMesh::default();

    // Two stepped tiers of stone.
    mesh.cuboid(
        Vec3::new(-3.6, 0.0, -3.6),
        Vec3::new(3.6, 0.6, 3.6),
        shade(palette::HEARTH_STONE, 0.8),
    );
    mesh.cuboid(
        Vec3::new(-2.9, 0.6, -2.9),
        Vec3::new(2.9, PLATFORM_TOP, 2.9),
        shade(palette::HEARTH_STONE, 0.92),
    );

    // Ring of stones, each a slightly different height and color, some with moss on top.
    for i in -3..3 {
        for j in -3..3 {
            if !(i == -3 || i == 2 || j == -3 || j == 2) {
                continue;
            }
            let (x, z) = (i as f32, j as f32);
            let top = PLATFORM_TOP + rng.range(1.3, 2.1);
            let color = vary(palette::HEARTH_STONE, &mut rng, 0.06, 10.0);
            mesh.cuboid(
                Vec3::new(x, PLATFORM_TOP, z),
                Vec3::new(x + 1.0, top, z + 1.0),
                color,
            );
            if rng.chance(0.2) {
                let moss = vary(palette::FOREST_MOSS, &mut rng, 0.04, 8.0);
                mesh.cuboid(
                    Vec3::new(x + 0.1, top, z + 0.1),
                    Vec3::new(x + 0.9, top + 0.15, z + 0.9),
                    moss,
                );
            }
        }
    }

    // Charred logs stacked log-cabin style in the fire.
    let charred = shade(palette::BARK, 0.55);
    let log_bottom = PLATFORM_TOP + 0.6;
    for offset in [-1.05, 1.05] {
        mesh.cuboid(
            Vec3::new(-1.9, log_bottom, offset - 0.45),
            Vec3::new(1.9, log_bottom + 0.9, offset + 0.45),
            charred,
        );
        mesh.cuboid(
            Vec3::new(offset - 0.45, log_bottom + 0.9, -1.9),
            Vec3::new(offset + 0.45, log_bottom + 1.8, 1.9),
            charred,
        );
    }

    // A woodpile against the platform's south side, with light log ends facing out.
    let logs = [(3.65, 0.0), (4.25, 0.0), (3.95, 0.6)];
    for (z, y) in logs {
        let bark = vary(palette::BARK, &mut rng, 0.04, 6.0);
        let (x0, x1) = (-3.2, -0.4);
        mesh.cuboid(Vec3::new(x0, y, z), Vec3::new(x1, y + 0.6, z + 0.6), bark);
        for (from, to) in [(x0 - 0.06, x0), (x1, x1 + 0.06)] {
            mesh.cuboid(
                Vec3::new(from, y + 0.1, z + 0.1),
                Vec3::new(to, y + 0.5, z + 0.5),
                palette::LOG_END,
            );
        }
    }
    mesh
}

/// Cheap smooth noise in roughly [-1, 1]: a few out-of-phase sines.
fn wobble(t: f32, phase: f32) -> f32 {
    ((t * 7.3 + phase).sin() * 0.5
        + (t * 13.1 + phase * 2.3).sin() * 0.3
        + (t * 23.7 + phase).sin() * 0.2)
        .clamp(-1.0, 1.0)
}

fn flicker(
    time: Res<Time>,
    mut flames: Query<(&Flame, &mut Transform)>,
    mut lights: Query<&mut PointLight, With<Firelight>>,
) {
    let t = time.elapsed_secs();
    for (flame, mut transform) in &mut flames {
        let w = wobble(t, flame.phase);
        let stretch = 1.0 + 0.18 * w;
        transform.scale = flame.size * Vec3::new(1.0 - 0.08 * w, stretch, 1.0 - 0.08 * w);
        // Keep the flame's base in place while it stretches, plus a tiny sway.
        let sway = Vec3::new(
            wobble(t * 0.7, flame.phase + 3.0),
            0.0,
            wobble(t * 0.6, flame.phase + 5.0),
        );
        transform.translation =
            flame.rest + Vec3::Y * (stretch - 1.0) * flame.size.y * 0.5 + sway * 0.12 * VOXEL;
    }
    for mut light in &mut lights {
        light.intensity = FIRELIGHT_INTENSITY * (1.0 + 0.12 * wobble(t * 1.3, 0.0));
    }
}

fn spawn_embers(
    mut commands: Commands,
    time: Res<Time>,
    mut cooldown: Local<f32>,
    mut rng: Local<Option<Rng>>,
    hearths: Query<&GlobalTransform, (With<Hearth>, Without<Tween>)>,
    fire: Res<FireAssets>,
) {
    *cooldown -= time.delta_secs();
    if *cooldown > 0.0 {
        return;
    }
    let rng = rng.get_or_insert_with(|| Rng::new(0xE3BE_7125));
    *cooldown = EMBER_INTERVAL * rng.range(0.5, 1.5);
    for hearth in &hearths {
        let start = hearth.translation()
            + Vec3::new(
                rng.range(-1.2, 1.2),
                rng.range(5.5, 7.0),
                rng.range(-1.2, 1.2),
            ) * VOXEL;
        let size = rng.range(0.18, 0.32) * VOXEL;
        commands.spawn((
            Ember {
                age: 0.0,
                lifetime: rng.range(1.4, 2.6),
                size,
                drift: rng.range(0.0, std::f32::consts::TAU),
            },
            Mesh3d(fire.cube.clone()),
            MeshMaterial3d(fire.ember.clone()),
            Transform::from_translation(start).with_scale(Vec3::splat(size)),
            NotShadowCaster,
        ));
    }
}

fn rise_embers(
    mut commands: Commands,
    time: Res<Time>,
    mut embers: Query<(Entity, &mut Ember, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (entity, mut ember, mut transform) in &mut embers {
        ember.age += dt;
        let life = ember.age / ember.lifetime;
        if life >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let sway = Vec3::new(
            (ember.age * 2.1 + ember.drift).sin(),
            0.0,
            (ember.age * 1.7 + ember.drift).cos(),
        );
        transform.translation += (Vec3::Y * 0.45 + sway * 0.12) * dt;
        transform.scale = Vec3::splat(ember.size * (1.0 - life));
    }
}
