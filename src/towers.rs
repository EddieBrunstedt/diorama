//! Towers: construction sites while the keeper builds, finished towers dropping into place, and
//! see-through ghosts where towers are planned.

use crate::island::cell_to_world;
use crate::juice::{Burst, Particles, Squash, Tween};
use crate::palette;
use crate::rng::Rng;
use crate::voxel::{VoxelMaterial, VoxelMesh, shade, vary};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

/// How high a finished tower drops from.
const DROP_HEIGHT: f32 = 1.4;
/// Seconds between finishing the build and the tower dropping in (the logs fly over first).
const DROP_DELAY: f32 = 0.3;
/// How much ghosts glow, so they read as plans rather than real buildings.
const GHOST_GLOW: f32 = 0.35;
/// Ghosts are a bit smaller than the real tower.
const GHOST_SCALE: f32 = 0.9;

const MODEL_SALT: u64 = 6;

pub struct TowersPlugin;

impl Plugin for TowersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VoxelMaterial>()
            .init_resource::<TowerAssets>()
            .add_systems(Update, (land_towers, bob_ghosts));
    }
}

/// A finished tower.
#[derive(Component, Debug)]
pub struct Tower {
    pub cell: IVec2,
}

/// A tower being built. `body` is the child that squashes when hammered.
#[derive(Component, Debug)]
pub struct BuildSite {
    pub cell: IVec2,
    pub body: Entity,
}

/// A see-through preview of a planned tower.
#[derive(Component, Debug)]
pub struct Ghost;

/// A tower that is still dropping into place.
#[derive(Component)]
struct Landing {
    body: Entity,
}

#[derive(Resource)]
pub struct TowerAssets {
    tower: Handle<Mesh>,
    site: Handle<Mesh>,
    /// The tower's shape without its colors, for blueprint-style ghosts.
    ghost: Handle<Mesh>,
    ghost_ok: Handle<StandardMaterial>,
    ghost_problem: Handle<StandardMaterial>,
}

impl FromWorld for TowerAssets {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        let tower = meshes.add(tower_mesh().build());
        let site = meshes.add(site_mesh().build());
        let mut ghost = tower_mesh().build();
        ghost.remove_attribute(Mesh::ATTRIBUTE_COLOR);
        let ghost = meshes.add(ghost);
        // Solid, softly glowing blueprints: see-through ghosts of a mesh made of overlapping
        // boxes show every inner face and turn into a smear.
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let mut blueprint = |color: Color| {
            materials.add(StandardMaterial {
                base_color: color,
                emissive: color.to_linear() * GHOST_GLOW,
                perceptual_roughness: 1.0,
                reflectance: 0.0,
                ..default()
            })
        };
        Self {
            tower,
            site,
            ghost,
            ghost_ok: blueprint(palette::BLUEPRINT),
            ghost_problem: blueprint(palette::BLUEPRINT_PROBLEM),
        }
    }
}

/// Starts a construction site on `cell`.
pub fn spawn_site(
    commands: &mut Commands,
    assets: &TowerAssets,
    voxel: &VoxelMaterial,
    cell: IVec2,
) -> Entity {
    let pop = Tween::pop_in(Vec3::ONE, 0.0);
    let root = commands
        .spawn((
            Transform {
                translation: cell_to_world(cell),
                ..pop.start_transform()
            },
            Visibility::Hidden,
            pop,
        ))
        .id();
    let body = commands
        .spawn((
            Mesh3d(assets.site.clone()),
            MeshMaterial3d(voxel.0.clone()),
            Transform::default(),
            Squash::new(Vec3::ONE),
            ChildOf(root),
        ))
        .id();
    commands.entity(root).insert(BuildSite { cell, body });
    root
}

/// Replaces a finished site with a tower that drops into place.
pub fn finish_site(
    commands: &mut Commands,
    assets: &TowerAssets,
    voxel: &VoxelMaterial,
    site: Entity,
    cell: IVec2,
) {
    commands.entity(site).despawn();
    let drop = Tween::drop_in(cell_to_world(cell), DROP_HEIGHT, DROP_DELAY);
    let root = commands
        .spawn((
            Tower { cell },
            drop.start_transform(),
            Visibility::Hidden,
            drop,
        ))
        .id();
    let body = commands
        .spawn((
            Mesh3d(assets.tower.clone()),
            MeshMaterial3d(voxel.0.clone()),
            Transform::default(),
            Squash::new(Vec3::ONE),
            ChildOf(root),
        ))
        .id();
    commands.entity(root).insert(Landing { body });
}

/// A ghost tower on `cell`, red if the plan says it can't be built. Pops in from nothing.
pub fn ghost_bundle(assets: &TowerAssets, cell: IVec2, problem: bool) -> impl Bundle {
    let pop = Tween::pop_in(Vec3::splat(GHOST_SCALE), 0.0);
    (
        Ghost,
        Mesh3d(assets.ghost.clone()),
        MeshMaterial3d(ghost_material(assets, problem)),
        Transform {
            translation: cell_to_world(cell),
            ..pop.start_transform()
        },
        Visibility::Hidden,
        pop,
        NotShadowCaster,
    )
}

pub fn ghost_material(assets: &TowerAssets, problem: bool) -> Handle<StandardMaterial> {
    if problem {
        assets.ghost_problem.clone()
    } else {
        assets.ghost_ok.clone()
    }
}

/// Squash and a puff of dust when a dropping tower hits the ground.
fn land_towers(
    mut commands: Commands,
    landing: Query<(Entity, &Landing, &GlobalTransform), Without<Tween>>,
    mut bodies: Query<&mut Squash>,
    mut particles: Particles,
) {
    for (entity, landing, global) in &landing {
        commands.entity(entity).remove::<Landing>();
        if let Ok(mut squash) = bodies.get_mut(landing.body) {
            squash.pulse(-0.3);
        }
        particles.burst(
            &mut commands,
            global.translation() + Vec3::Y * 0.05,
            Burst {
                colors: &[palette::DIRT, palette::ROAD, palette::MEADOW_GRASS],
                count: 18,
                size: 0.05,
                spread: 1.6,
                lift: 1.2,
                lifetime: 0.7,
            },
        );
    }
}

/// Ghosts hover gently so they read as "not real yet".
fn bob_ghosts(time: Res<Time>, mut ghosts: Query<&mut Transform, With<Ghost>>) {
    for mut transform in &mut ghosts {
        let phase = transform.translation.x * 1.3 + transform.translation.z * 0.7;
        transform.translation.y = 0.03 + 0.03 * (time.elapsed_secs() * 2.2 + phase).sin();
    }
}

/// A wooden archer tower on a stone base, in voxels around the tile center.
fn tower_mesh() -> VoxelMesh {
    let mut rng = Rng::new(MODEL_SALT);
    let mut mesh = VoxelMesh::default();

    // Stone base, built from a few uneven blocks.
    mesh.cuboid(
        Vec3::new(-3.2, 0.0, -3.2),
        Vec3::new(3.2, 1.0, 3.2),
        shade(palette::HEARTH_STONE, 0.85),
    );
    for (x, z) in [(-3.0, -3.0), (0.0, -3.0), (-3.0, 0.0), (0.0, 0.0)] {
        let top = 1.0 + rng.range(0.5, 0.9);
        let color = vary(palette::HEARTH_STONE, &mut rng, 0.05, 8.0);
        mesh.cuboid(
            Vec3::new(x, 1.0, z),
            Vec3::new(x + 3.0, top, z + 3.0),
            color,
        );
    }

    // Four posts with a band of beams halfway up.
    let post_top = 6.8;
    for (x, z) in [(-2.6, -2.6), (1.8, -2.6), (-2.6, 1.8), (1.8, 1.8)] {
        let bark = vary(palette::BARK, &mut rng, 0.03, 6.0);
        mesh.cuboid(
            Vec3::new(x, 1.4, z),
            Vec3::new(x + 0.8, post_top, z + 0.8),
            bark,
        );
    }
    let beam = shade(palette::PLANK, 0.8);
    mesh.cuboid(Vec3::new(-2.6, 3.8, -2.4), Vec3::new(2.6, 4.3, -2.0), beam);
    mesh.cuboid(Vec3::new(-2.6, 3.8, 2.0), Vec3::new(2.6, 4.3, 2.4), beam);
    mesh.cuboid(Vec3::new(-2.4, 3.8, -2.6), Vec3::new(-2.0, 4.3, 2.6), beam);
    mesh.cuboid(Vec3::new(2.0, 3.8, -2.6), Vec3::new(2.4, 4.3, 2.6), beam);

    // Lookout platform with a plank railing.
    mesh.cuboid(
        Vec3::new(-3.4, post_top, -3.4),
        Vec3::new(3.4, post_top + 0.6, 3.4),
        palette::PLANK,
    );
    let rail_bottom = post_top + 0.6;
    for k in 0..7 {
        let along = -3.4 + k as f32;
        let height = if k % 2 == 0 { 1.1 } else { 0.7 };
        let color = vary(palette::PLANK, &mut rng, 0.04, 6.0);
        for (min, max) in [
            (
                Vec3::new(along, 0.0, -3.4),
                Vec3::new(along + 0.8, height, -2.9),
            ),
            (
                Vec3::new(along, 0.0, 2.9),
                Vec3::new(along + 0.8, height, 3.4),
            ),
            (
                Vec3::new(-3.4, 0.0, along),
                Vec3::new(-2.9, height, along + 0.8),
            ),
            (
                Vec3::new(2.9, 0.0, along),
                Vec3::new(3.4, height, along + 0.8),
            ),
        ] {
            mesh.cuboid(
                min + Vec3::Y * rail_bottom,
                max + Vec3::Y * rail_bottom,
                color,
            );
        }
    }

    // Corner poles holding a stepped roof.
    let roof_bottom = rail_bottom + 2.0;
    for (x, z) in [(-3.2, -3.2), (2.8, -3.2), (-3.2, 2.8), (2.8, 2.8)] {
        mesh.cuboid(
            Vec3::new(x, rail_bottom, z),
            Vec3::new(x + 0.4, roof_bottom, z + 0.4),
            palette::BARK,
        );
    }
    let tiers = [(3.9, 0.6), (2.9, 0.6), (1.9, 0.6), (0.9, 0.6)];
    let mut bottom = roof_bottom;
    for (k, (half, height)) in tiers.into_iter().enumerate() {
        let t = k as f32 / (tiers.len() - 1) as f32;
        let color = vary(
            palette::ROOF.mix(&palette::ROOF_TOP, t),
            &mut rng,
            0.02,
            4.0,
        );
        mesh.cuboid(
            Vec3::new(-half, bottom, -half),
            Vec3::new(half, bottom + height, half),
            color,
        );
        bottom += height;
    }

    // A little flag on top.
    mesh.cuboid(
        Vec3::new(-0.12, bottom, -0.12),
        Vec3::new(0.12, bottom + 2.0, 0.12),
        palette::BARK,
    );
    mesh.cuboid(
        Vec3::new(0.12, bottom + 1.2, -0.08),
        Vec3::new(1.4, bottom + 1.9, 0.08),
        palette::FLAG,
    );
    mesh
}

/// A construction site: foundation stones, corner posts and a stack of planks.
fn site_mesh() -> VoxelMesh {
    let mut rng = Rng::new(MODEL_SALT + 1);
    let mut mesh = VoxelMesh::default();
    mesh.cuboid(
        Vec3::new(-3.2, 0.0, -3.2),
        Vec3::new(3.2, 0.8, 3.2),
        shade(palette::HEARTH_STONE, 0.85),
    );
    for (x, z) in [(-2.6, -2.6), (1.8, -2.6), (-2.6, 1.8), (1.8, 1.8)] {
        let top = rng.range(2.4, 3.6);
        mesh.cuboid(
            Vec3::new(x, 0.8, z),
            Vec3::new(x + 0.8, top, z + 0.8),
            vary(palette::BARK, &mut rng, 0.03, 6.0),
        );
    }
    for k in 0..3 {
        let y = 0.8 + k as f32 * 0.4;
        let shift = rng.range(-0.3, 0.3);
        mesh.cuboid(
            Vec3::new(-1.8 + shift, y, -0.9 + k as f32 * 0.2),
            Vec3::new(1.6 + shift, y + 0.4, 0.5 + k as f32 * 0.2),
            vary(palette::PLANK, &mut rng, 0.04, 6.0),
        );
    }
    mesh
}
