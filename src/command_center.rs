//! The Command Center: the heart of the outpost and what the horde is after. It stands in the
//! middle of the map from the start, as a structure on its cells (see [`Landmarks`]).
//!
//! A swarmer that reaches it bursts on it and deals damage: the building flinches and flashes,
//! and gibs fly. When its HP runs out it flashes hard and refills, until milestone 5 makes that
//! the end of the run.

use crate::horde::HordeSystems;
use crate::juice::{Burst, Particles, Shake, Squash};
use crate::map::Landmarks;
use crate::palette;
use crate::shapes::{BoxMesh, shade};
use crate::tuning::command_center::MAX_HP;
use crate::tuning::juice::*;
use bevy::prelude::*;

/// How tall it stands (to the top of its antenna).
pub const HEIGHT: f32 = 1.5;
/// How bright the hit flash glows at its peak.
const FLASH_BRIGHTNESS: f32 = 2.5;

pub struct CommandCenterPlugin;

impl Plugin for CommandCenterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandCenterHealth>()
            .add_message::<CommandCenterHit>()
            .add_systems(Startup, spawn_command_center)
            .add_systems(Update, (take_hits, flash).chain().after(HordeSystems));
    }
}

/// Something reached the Command Center at `at` and hit it.
#[derive(Message, Clone, Copy, Debug)]
pub struct CommandCenterHit {
    pub at: Vec3,
    pub damage: u32,
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct CommandCenterHealth {
    pub hp: u32,
    pub max: u32,
}

impl Default for CommandCenterHealth {
    fn default() -> Self {
        Self {
            hp: MAX_HP,
            max: MAX_HP,
        }
    }
}

impl CommandCenterHealth {
    /// Takes `damage`. Returns true if that used up the last of its HP.
    pub fn damage(&mut self, damage: u32) -> bool {
        let before = self.hp;
        self.hp = self.hp.saturating_sub(damage);
        before > 0 && self.hp == 0
    }
}

#[derive(Component)]
struct CommandCenterView {
    /// Its own material, so it can flash without lighting up everything else.
    material: Handle<StandardMaterial>,
    /// Seconds of flash left, and how strong it started.
    flash: f32,
    strength: f32,
}

fn spawn_command_center(
    mut commands: Commands,
    landmarks: Res<Landmarks>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let footprint = landmarks.command_center;
    let material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.8,
        reflectance: 0.25,
        ..default()
    });
    commands.spawn((
        CommandCenterView {
            material: material.clone(),
            flash: 0.0,
            strength: 0.0,
        },
        Mesh3d(meshes.add(command_center_mesh(footprint.size as f32))),
        MeshMaterial3d(material),
        Transform::from_translation(footprint.center()),
        Squash::new(Vec3::ONE),
    ));
}

/// A squat bunker `size` cells across, centered on the origin: a concrete plinth with a hazard
/// edge, a steel block with lit windows, and an antenna with a light on top.
fn command_center_mesh(size: f32) -> Mesh {
    let mut mesh = BoxMesh::default();
    let half = size / 2.0 - 0.05;
    let cube = |mesh: &mut BoxMesh, half: Vec2, low: f32, high: f32, color: Color| {
        mesh.cuboid(
            Vec3::new(-half.x, low, -half.y),
            Vec3::new(half.x, high, half.y),
            color,
        );
    };
    cube(&mut mesh, Vec2::splat(half), 0.0, 0.2, palette::CONCRETE);
    cube(&mut mesh, Vec2::splat(half), 0.2, 0.26, palette::HAZARD);
    // Corner pylons.
    for (x, z) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let at = Vec2::new(x, z) * (half - 0.14);
        mesh.cuboid(
            Vec3::new(at.x - 0.14, 0.26, at.y - 0.14),
            Vec3::new(at.x + 0.14, 0.55, at.y + 0.14),
            shade(palette::CONCRETE, 0.9),
        );
    }
    let block = half * 0.74;
    cube(&mut mesh, Vec2::splat(block), 0.26, 0.95, palette::STEEL);
    // A band of lit windows around the block.
    let windows = block + 0.015;
    cube(
        &mut mesh,
        Vec2::new(windows, block * 0.7),
        0.62,
        0.72,
        palette::BASE_LIGHT,
    );
    cube(
        &mut mesh,
        Vec2::new(block * 0.7, windows),
        0.62,
        0.72,
        palette::BASE_LIGHT,
    );
    let top = half * 0.46;
    cube(
        &mut mesh,
        Vec2::splat(top),
        0.95,
        1.2,
        shade(palette::STEEL, 1.25),
    );
    // Antenna and its light.
    let mast = Vec2::new(0.22, -0.18);
    mesh.cuboid(
        Vec3::new(mast.x - 0.03, 1.2, mast.y - 0.03),
        Vec3::new(mast.x + 0.03, HEIGHT - 0.08, mast.y + 0.03),
        palette::STEEL,
    );
    mesh.cuboid_graded(
        Vec3::new(mast.x - 0.06, HEIGHT - 0.08, mast.y - 0.06),
        Vec3::new(mast.x + 0.06, HEIGHT, mast.y + 0.06),
        palette::BASE_LIGHT,
        palette::BASE_LIGHT,
    );
    mesh.build()
}

fn take_hits(
    mut commands: Commands,
    mut hits: MessageReader<CommandCenterHit>,
    mut health: ResMut<CommandCenterHealth>,
    mut view: Single<(&mut CommandCenterView, &mut Squash)>,
    mut particles: Particles,
    mut shake: ResMut<Shake>,
) {
    let mut count = 0;
    let mut fell = false;
    for hit in hits.read() {
        fell |= health.damage(hit.damage);
        // A few bursts a frame is plenty to read, even when a whole pile arrives at once.
        if count < MAX_GIB_BURSTS {
            particles.burst(
                &mut commands,
                hit.at + Vec3::Y * 0.15,
                0.05,
                Burst {
                    colors: &palette::GIBS,
                    count: GIB_COUNT,
                    size: GIB_SIZE,
                    spread: GIB_SPREAD,
                    lift: GIB_LIFT,
                    lifetime: GIB_LIFETIME,
                },
            );
        }
        count += 1;
    }
    if count == 0 {
        return;
    }
    let (view, squash) = &mut *view;
    squash.pulse(BASE_HIT_SQUASH);
    shake.add_capped(BASE_HIT_SHAKE, BASE_HIT_SHAKE_MAX);
    if fell {
        // Until milestone 5 brings game over, it just shows it and starts over.
        info!("The Command Center fell. Its HP refills until there's a game over.");
        health.hp = health.max;
        shake.add(BASE_FALL_SHAKE);
        view.flash = BASE_FALL_FLASH_SECONDS;
        view.strength = 3.0;
    } else if view.flash <= BASE_HIT_FLASH_SECONDS {
        view.flash = BASE_HIT_FLASH_SECONDS;
        view.strength = 1.0;
    }
}

/// Glows red for a moment after a hit.
fn flash(
    time: Res<Time>,
    mut view: Single<&mut CommandCenterView>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if view.flash <= 0.0 {
        return;
    }
    view.flash = (view.flash - time.delta_secs()).max(0.0);
    let duration = if view.strength > 1.0 {
        BASE_FALL_FLASH_SECONDS
    } else {
        BASE_HIT_FLASH_SECONDS
    };
    let glow = view.flash / duration * view.strength * FLASH_BRIGHTNESS;
    if let Some(mut material) = materials.get_mut(&view.material) {
        material.emissive = (palette::BASE_HIT.to_linear() * glow).with_alpha(1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_runs_out_once() {
        let mut health = CommandCenterHealth { hp: 3, max: 3 };
        assert!(!health.damage(2));
        assert!(health.damage(2), "the hit that takes the last HP");
        assert_eq!(health.hp, 0);
        assert!(!health.damage(1), "already out");
    }
}
