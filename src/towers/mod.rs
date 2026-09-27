//! Towers: construction sites while the keeper builds (towers and the catapult), finished towers
//! dropping into place, see-through ghosts where buildings are planned, and shooting slimes during
//! the wave ([`shooting`], with the choice of target in [`aim`]).
//!
//! Slimes spit goo at towers near the road ([`health`]): each hit sticks a blob of goo on the
//! tower, and a tower out of hit points slumps over, broken, and stops shooting. The keeper's
//! hammer knocks the goo off and sets it straight ([`TowerWork`]).

pub mod aim;
pub mod health;
mod shooting;

use crate::catapult;
use crate::island::cell_to_world;
use crate::juice::{Burst, Particles, Shake, Squash, Tween, Wobble};
use crate::keeper::Building;
use crate::lamps::{self, GlowMaterials, Lamp};
use crate::palette;
use crate::phase::GameState;
use crate::rng::Rng;
use crate::ui::Popup;
use crate::voxel::{VOXEL, VoxelMaterial, VoxelMesh, shade, vary};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use health::TowerHealth;
use shooting::{Archer, Arrow};

/// How high a finished tower drops from.
const DROP_HEIGHT: f32 = 1.4;
/// Seconds between finishing the build and the tower dropping in (the logs fly over first).
const DROP_DELAY: f32 = 0.3;
/// How much ghosts glow, so they read as plans rather than real buildings.
const GHOST_GLOW: f32 = 0.35;
/// Ghosts are a bit smaller than the real tower.
const GHOST_SCALE: f32 = 0.9;

const MODEL_SALT: u64 = 6;
/// The tower's stepped roof (voxels): where it starts, each step's half width, and their height.
const ROOF_BOTTOM: f32 = 9.4;
const ROOF_STEPS: [f32; 4] = [3.9, 2.9, 1.9, 0.9];
const ROOF_STEP_HEIGHT: f32 = 0.6;
/// A broken tower leans over this far (radians) and sags.
const BROKEN_TILT: f32 = 0.24;
const BROKEN_SAG: f32 = 0.86;
/// Seconds between a broken tower's creaks, when it shudders and sheds a few chips.
const CREAK_INTERVAL: f32 = 1.3;
/// Height above the tower's base for popups.
const POPUP_HEIGHT: f32 = 1.2;
/// Where the lantern hangs from the lookout platform (voxels): on the corner facing the camera.
const LANTERN_HOOK: Vec3 = Vec3::new(3.0, 6.8, 3.0);
/// The lantern's glass, below the hook (voxels).
const LANTERN_GLASS: Vec3 = Vec3::new(0.0, -1.25, 0.0);
const LANTERN_GLASS_SIZE: Vec3 = Vec3::new(0.7, 1.0, 0.7);
/// How brightly a lantern lights the tower and the road at full dusk, and how far.
const LANTERN_LUMENS: f32 = 20_000.0;
const LANTERN_RANGE: f32 = 2.4;

pub struct TowersPlugin;

impl Plugin for TowersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VoxelMaterial>()
            .init_resource::<TowerAssets>()
            .add_message::<TowerHit>()
            .add_message::<TowerWork>()
            .add_systems(OnExit(GameState::GameOver), clear_towers)
            .add_systems(
                Update,
                (
                    land_towers,
                    bob_ghosts,
                    shooting::shoot.run_if(in_state(GameState::Wave)),
                    shooting::fly_arrows,
                    (take_spits, do_tower_work, creak_broken_towers).chain(),
                    douse_lanterns,
                ),
            );
    }
}

/// A finished tower. `body` is the child with the model, which squashes, tilts and gets goo on it.
#[derive(Component, Debug)]
pub struct Tower {
    pub cell: IVec2,
    pub body: Entity,
    pub health: TowerHealth,
    /// The lantern's glass and its light, which go out while the tower is broken.
    lantern: (Entity, Entity),
}

/// A building going up. `body` is the child that squashes when hammered.
#[derive(Component, Debug)]
pub struct BuildSite {
    pub cell: IVec2,
    pub body: Entity,
    pub building: Building,
}

/// Goo that landed on a tower (at this world point, spat from `from`).
#[derive(Message, Clone, Copy, Debug)]
pub struct TowerHit {
    pub tower: Entity,
    pub at: Vec3,
}

/// The keeper working on the tower on a cell.
#[derive(Message, Clone, Copy, Debug)]
pub enum TowerWork {
    /// A hammer strike while repairing.
    Strike(IVec2),
    /// The repair is done.
    Repaired(IVec2),
}

/// A blob of goo stuck on a tower.
#[derive(Component, Debug)]
struct Goo;

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
    catapult_ghost: Handle<Mesh>,
    goo: Handle<Mesh>,
    ghost_ok: Handle<StandardMaterial>,
    ghost_problem: Handle<StandardMaterial>,
    arrow: Handle<Mesh>,
    lantern: Handle<Mesh>,
    cube: Handle<Mesh>,
    voxel: Handle<StandardMaterial>,
    /// Lantern glass, lit and out.
    lamp: Handle<StandardMaterial>,
    lamp_out: Handle<StandardMaterial>,
    /// Goo glows like the slimes it came from.
    goo_material: Handle<StandardMaterial>,
}

impl FromWorld for TowerAssets {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        let tower = meshes.add(tower_mesh().build());
        let site = meshes.add(site_mesh().build());
        let arrow = meshes.add(shooting::arrow_mesh().build());
        let lantern = meshes.add(lantern_mesh().build());
        let cube = meshes.add(Cuboid::from_length(1.0));
        let mut ghost = tower_mesh().build();
        ghost.remove_attribute(Mesh::ATTRIBUTE_COLOR);
        let ghost = meshes.add(ghost);
        let mut catapult_ghost = catapult::model::ghost_mesh().build();
        catapult_ghost.remove_attribute(Mesh::ATTRIBUTE_COLOR);
        let catapult_ghost = meshes.add(catapult_ghost);
        let goo = meshes.add(goo_mesh().build());
        let voxel = world.resource::<VoxelMaterial>().0.clone();
        let glow = world.get_resource_or_init::<GlowMaterials>();
        let (lamp, lamp_out, goo_material) =
            (glow.lamp.clone(), glow.lamp_out.clone(), glow.slime.clone());
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
            catapult_ghost,
            goo,
            ghost_ok: blueprint(palette::BLUEPRINT),
            ghost_problem: blueprint(palette::BLUEPRINT_PROBLEM),
            arrow,
            lantern,
            cube,
            voxel,
            lamp,
            lamp_out,
            goo_material,
        }
    }
}

/// Starts a construction site for `building` on `cell`.
pub fn spawn_site(
    commands: &mut Commands,
    assets: &TowerAssets,
    voxel: &VoxelMaterial,
    cell: IVec2,
    building: Building,
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
    commands.entity(root).insert(BuildSite {
        cell,
        body,
        building,
    });
    root
}

/// Replaces a finished site with a tower that drops into place.
pub fn finish_tower(
    commands: &mut Commands,
    assets: &TowerAssets,
    voxel: &VoxelMaterial,
    site: Entity,
    cell: IVec2,
) {
    commands.entity(site).despawn();
    let drop = Tween::drop_in(cell_to_world(cell), DROP_HEIGHT, DROP_DELAY);
    let root = commands
        .spawn((drop.start_transform(), Visibility::Hidden, drop))
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
    let hook = commands
        .spawn((
            Mesh3d(assets.lantern.clone()),
            MeshMaterial3d(voxel.0.clone()),
            Transform::from_translation(LANTERN_HOOK * VOXEL),
            NotShadowCaster,
            ChildOf(body),
        ))
        .id();
    let glass = commands
        .spawn((
            Mesh3d(assets.cube.clone()),
            MeshMaterial3d(assets.lamp.clone()),
            Transform::from_translation(LANTERN_GLASS * VOXEL)
                .with_scale(LANTERN_GLASS_SIZE * VOXEL),
            NotShadowCaster,
            ChildOf(hook),
        ))
        .id();
    let phase = (cell.x * 7 + cell.y * 13) as f32;
    let light = commands
        .spawn((
            lamps::lamp(LANTERN_LUMENS, LANTERN_RANGE, phase),
            Transform::from_translation(LANTERN_GLASS * VOXEL),
            ChildOf(hook),
        ))
        .id();
    commands.entity(root).insert((
        Tower {
            cell,
            body,
            health: TowerHealth::default(),
            lantern: (glass, light),
        },
        Landing { body },
        Archer::new(body),
    ));
}

/// A ghost of `building` on `cell`, red if the plan says it can't be built. Pops in from nothing.
pub fn ghost_bundle(
    assets: &TowerAssets,
    cell: IVec2,
    building: Building,
    problem: bool,
) -> impl Bundle {
    let (mesh, scale) = match building {
        Building::Tower => (assets.ghost.clone(), GHOST_SCALE),
        Building::Catapult => (
            assets.catapult_ghost.clone(),
            GHOST_SCALE * catapult::model::SCALE,
        ),
    };
    let pop = Tween::pop_in(Vec3::splat(scale), 0.0);
    (
        Ghost,
        Mesh3d(mesh),
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

/// Where goo spat from the direction `toward` (world x/z, from the tower) sticks on a tower, in
/// the tower's own space: mostly splattered anywhere on the stepped roof, where it shows from
/// above, otherwise somewhere solid on the side facing the slime (the lookout's railing, a corner
/// post, the stone base).
pub fn goo_spot(toward: Vec2, rng: &mut Rng) -> Vec3 {
    let (normal, across) = if toward.x.abs() > toward.y.abs() {
        (Vec3::X * toward.x.signum(), Vec3::Z)
    } else {
        (Vec3::Z * toward.y.signum(), Vec3::X)
    };
    let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
    if rng.chance(0.6) {
        let (x, z) = (rng.range(-3.2, 3.2), rng.range(-3.2, 3.2));
        let steps = ROOF_STEPS
            .iter()
            .filter(|&&half| x.abs().max(z.abs()) <= half)
            .count();
        let top = ROOF_BOTTOM + ROOF_STEP_HEIGHT * steps as f32;
        return Vec3::new(x, top + 0.3, z) * VOXEL;
    }
    // (distance out from the middle, height, where across the side), in voxels.
    let (out, height, lateral) = match rng.index(3) {
        0 => (3.5, rng.range(7.0, 8.0), rng.range(-2.8, 2.8)),
        1 => (2.7, rng.range(1.8, 6.2), 2.2 * side),
        _ => (3.3, rng.range(0.4, 1.3), rng.range(-2.4, 2.4)),
    };
    (normal * out + across * lateral + Vec3::Y * height) * VOXEL
}

/// Goo lands on a tower: it sticks, the tower shudders, and if that was the last hit point it
/// slumps over, broken.
fn take_spits(
    mut commands: Commands,
    mut hits: MessageReader<TowerHit>,
    mut towers: Query<(&mut Tower, &GlobalTransform)>,
    mut bodies: Query<(
        &mut Squash,
        &Transform,
        &GlobalTransform,
        Option<&mut Wobble>,
    )>,
    assets: Res<TowerAssets>,
    mut shake: ResMut<Shake>,
    mut particles: Particles,
    mut popups: MessageWriter<Popup>,
) {
    for hit in hits.read() {
        let Ok((mut tower, global)) = towers.get_mut(hit.tower) else {
            continue;
        };
        let broke = tower.health.damage(crate::tuning::SPIT_DAMAGE);
        let Ok((mut squash, transform, body_global, wobble)) = bodies.get_mut(tower.body) else {
            continue;
        };
        let local = body_global.affine().inverse().transform_point3(hit.at);
        commands.spawn((
            Goo,
            Mesh3d(assets.goo.clone()),
            MeshMaterial3d(assets.goo_material.clone()),
            Transform::from_translation(local),
            Visibility::Hidden,
            Tween::pop_in(Vec3::ONE, 0.0),
            ChildOf(tower.body),
        ));
        particles.burst(
            &mut commands,
            hit.at,
            Burst {
                colors: &[palette::GOO, palette::SLIME],
                count: 6,
                size: 0.04,
                spread: 0.8,
                lift: 1.0,
                lifetime: 0.5,
            },
        );
        let away = (global.translation() - hit.at)
            .with_y(0.0)
            .normalize_or(Vec3::X);
        let axis = Vec3::Y.cross(away);
        if broke {
            // It slumps over away from the goo, sagging.
            squash.rest = Vec3::new(1.0, BROKEN_SAG, 1.0);
            squash.pulse(-0.3);
            let tilt = Quat::from_axis_angle(axis, BROKEN_TILT);
            let mut slumped = *transform;
            slumped.rotation = tilt;
            let mut shudder = Wobble::new(None, &slumped, axis, 0.12);
            shudder.rest = tilt;
            commands.entity(tower.body).insert(shudder);
            shake.add(0.3);
            particles.burst(
                &mut commands,
                global.translation() + Vec3::Y * 0.7,
                Burst {
                    colors: &[palette::PLANK, palette::ROOF, palette::HEARTH_STONE],
                    count: 16,
                    size: 0.05,
                    spread: 1.4,
                    lift: 1.8,
                    lifetime: 0.8,
                },
            );
            popups.write(Popup::problem(
                global.translation() + Vec3::Y * POPUP_HEIGHT,
                "Tower broken!",
            ));
        } else {
            squash.pulse(-0.12);
            let mut shudder = Wobble::new(wobble.as_deref(), transform, axis, 0.06);
            if let Some(existing) = wobble {
                shudder.rest = existing.rest;
            }
            commands.entity(tower.body).insert(shudder);
        }
    }
}

/// The keeper's hammer knocks goo off a tower, and a finished repair sets it straight.
fn do_tower_work(
    mut commands: Commands,
    mut work: MessageReader<TowerWork>,
    mut towers: Query<(&mut Tower, &GlobalTransform)>,
    mut bodies: Query<(&mut Squash, &Transform, Option<&Children>)>,
    goo: Query<&GlobalTransform, With<Goo>>,
    mut particles: Particles,
    mut popups: MessageWriter<Popup>,
) {
    for work in work.read() {
        let cell = match *work {
            TowerWork::Strike(cell) | TowerWork::Repaired(cell) => cell,
        };
        let Some((mut tower, global)) = towers.iter_mut().find(|(tower, _)| tower.cell == cell)
        else {
            continue;
        };
        let Ok((mut squash, transform, children)) = bodies.get_mut(tower.body) else {
            continue;
        };
        let blobs: Vec<(Entity, Vec3)> = children
            .into_iter()
            .flatten()
            .filter_map(|&child| Some((child, goo.get(child).ok()?.translation())))
            .collect();
        let knocked_off = match *work {
            TowerWork::Strike(_) => {
                squash.pulse(-0.14);
                // Each strike knocks off a blob of goo.
                blobs.into_iter().take(1).collect::<Vec<_>>()
            }
            TowerWork::Repaired(_) => {
                tower.health.repair();
                squash.rest = Vec3::ONE;
                squash.pulse(0.3);
                let upright = Transform {
                    rotation: Quat::IDENTITY,
                    ..*transform
                };
                let mut settle = Wobble::new(None, &upright, Vec3::X, 0.1);
                settle.rest = Quat::IDENTITY;
                commands.entity(tower.body).insert(settle);
                particles.burst(
                    &mut commands,
                    global.translation() + Vec3::Y * 0.9,
                    Burst {
                        colors: &[palette::FLAG, palette::UI_PAPER, palette::ORDER_REPAIR],
                        count: 14,
                        size: 0.04,
                        spread: 1.2,
                        lift: 2.2,
                        lifetime: 0.8,
                    },
                );
                popups.write(Popup::new(
                    global.translation() + Vec3::Y * POPUP_HEIGHT,
                    "Repaired!",
                    palette::ORDER_REPAIR,
                ));
                blobs
            }
        };
        for (blob, at) in knocked_off {
            commands.entity(blob).despawn();
            particles.burst(
                &mut commands,
                at,
                Burst {
                    colors: &[palette::GOO, palette::SLIME],
                    count: 5,
                    size: 0.04,
                    spread: 1.0,
                    lift: 1.4,
                    lifetime: 0.5,
                },
            );
        }
    }
}

/// Broken towers creak now and then, shuddering and shedding chips, so they stand out.
fn creak_broken_towers(
    mut commands: Commands,
    time: Res<Time>,
    towers: Query<(&Tower, &GlobalTransform)>,
    bodies: Query<(&Transform, Option<&Wobble>)>,
    mut particles: Particles,
    mut since: Local<f32>,
) {
    *since += time.delta_secs();
    if *since < CREAK_INTERVAL {
        return;
    }
    *since = 0.0;
    for (tower, global) in &towers {
        if !tower.health.is_broken() {
            continue;
        }
        if let Ok((transform, wobble)) = bodies.get(tower.body) {
            let mut creak = Wobble::new(wobble, transform, Vec3::Y, 0.05);
            if let Some(existing) = wobble {
                creak.rest = existing.rest;
            }
            commands.entity(tower.body).insert(creak);
        }
        particles.burst(
            &mut commands,
            global.translation() + Vec3::Y * 0.9,
            Burst {
                colors: &[palette::PLANK, palette::ROOF, palette::GOO],
                count: 3,
                size: 0.035,
                spread: 0.6,
                lift: 0.6,
                lifetime: 0.7,
            },
        );
    }
}

/// A broken tower's lantern goes out, and comes back on when it's repaired.
fn douse_lanterns(
    towers: Query<&Tower>,
    mut glasses: Query<&mut MeshMaterial3d<StandardMaterial>>,
    mut lamps: Query<&mut Lamp>,
    assets: Res<TowerAssets>,
) {
    for tower in &towers {
        let (glass, light) = tower.lantern;
        let lit = !tower.health.is_broken();
        if let Ok(mut lamp) = lamps.get_mut(light) {
            lamp.lit = lit;
        }
        if let Ok(mut material) = glasses.get_mut(glass) {
            let want = if lit { &assets.lamp } else { &assets.lamp_out };
            if material.0 != *want {
                material.0 = want.clone();
            }
        }
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

/// Clears the old run's towers, building sites and arrows.
fn clear_towers(
    mut commands: Commands,
    towers: Query<Entity, Or<(With<Tower>, With<BuildSite>, With<Arrow>)>>,
) {
    for entity in &towers {
        commands.entity(entity).despawn();
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
    let roof_bottom = ROOF_BOTTOM;
    for (x, z) in [(-3.2, -3.2), (2.8, -3.2), (-3.2, 2.8), (2.8, 2.8)] {
        mesh.cuboid(
            Vec3::new(x, rail_bottom, z),
            Vec3::new(x + 0.4, roof_bottom, z + 0.4),
            palette::BARK,
        );
    }
    let tiers = ROOF_STEPS.map(|half| (half, ROOF_STEP_HEIGHT));
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

/// A lantern's rope, cap and base, in voxels below its hook. The glass between them glows.
fn lantern_mesh() -> VoxelMesh {
    let mut mesh = VoxelMesh::default();
    let iron = shade(palette::KEEPER_EYES, 1.4);
    mesh.cuboid(
        Vec3::new(-0.08, -0.6, -0.08),
        Vec3::new(0.08, 0.0, 0.08),
        palette::ROPE,
    );
    mesh.cuboid(
        Vec3::new(-0.5, -0.75, -0.5),
        Vec3::new(0.5, -0.6, 0.5),
        iron,
    );
    mesh.cuboid(
        Vec3::new(-0.25, -0.85, -0.25),
        Vec3::new(0.25, -0.75, 0.25),
        iron,
    );
    mesh.cuboid(
        Vec3::new(-0.45, -1.9, -0.45),
        Vec3::new(0.45, -1.75, 0.45),
        iron,
    );
    mesh
}

/// A blob of goo, dripping.
fn goo_mesh() -> VoxelMesh {
    let mut mesh = VoxelMesh::default();
    let shine = palette::GOO.mix(&Color::WHITE, 0.5);
    mesh.cuboid(
        Vec3::new(-0.7, -0.5, -0.7),
        Vec3::new(0.7, 0.5, 0.7),
        palette::GOO,
    );
    mesh.cuboid(
        Vec3::new(-0.3, -1.2, -0.3),
        Vec3::new(0.3, -0.5, 0.3),
        palette::GOO,
    );
    mesh.cuboid(
        Vec3::new(-0.45, 0.3, -0.45),
        Vec3::new(0.0, 0.6, 0.0),
        shine,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goo_sticks_on_the_tower() {
        let mut rng = Rng::new(1);
        let roof_top = ROOF_BOTTOM + ROOF_STEP_HEIGHT * ROOF_STEPS.len() as f32;
        for _ in 0..500 {
            let spot = goo_spot(Vec2::new(-1.0, 0.3), &mut rng) / VOXEL;
            assert!(spot.x.abs() <= 3.6 && spot.z.abs() <= 3.6, "{spot:?}");
            assert!(spot.y > 0.0 && spot.y <= roof_top + 0.5, "{spot:?}");
            if spot.y < ROOF_BOTTOM {
                assert!(spot.x < 0.0, "on the side facing the slime: {spot:?}");
            }
        }
    }
}
