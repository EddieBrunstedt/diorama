//! Nests as the player sees them: a dark pit ringed with chitin spikes and eggs, sitting in its
//! crust of creep (drawn with the ground). It heaves when swarmers come out.

use super::{HordeSystems, SwarmersEmerged};
use crate::juice::Squash;
use crate::map::{Landmarks, NEST_HEIGHT};
use crate::palette;
use crate::rng::Rng;
use crate::shapes::{BoxMesh, ShapeMaterial, shade, vary};
use crate::tuning::juice::{NEST_PULSE, NEST_PULSE_INTERVAL};
use bevy::prelude::*;

/// Spikes around the rim, and how tall they stand.
const SPIKES: usize = 14;
const SPIKE_HEIGHT: (f32, f32) = (0.25, 0.6);
/// Eggs around the pit.
const EGGS: usize = 7;
/// How much of the nest the pit covers (a fraction of its size).
const PIT: f32 = 0.55;

pub struct NestPlugin;

impl Plugin for NestPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_nests)
            .add_systems(Update, heave.after(HordeSystems));
    }
}

#[derive(Component)]
struct NestView {
    index: usize,
    /// When it last heaved (game seconds).
    heaved: f32,
}

fn spawn_nests(
    mut commands: Commands,
    landmarks: Res<Landmarks>,
    material: Res<ShapeMaterial>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (index, nest) in landmarks.nests.iter().enumerate() {
        let mesh = meshes.add(nest_mesh(nest.size as f32, index as u64));
        commands.spawn((
            NestView {
                index,
                heaved: f32::NEG_INFINITY,
            },
            Mesh3d(mesh),
            MeshMaterial3d(material.0.clone()),
            Transform::from_translation(nest.center() + Vec3::Y * NEST_HEIGHT),
            Squash::new(Vec3::ONE),
        ));
    }
}

/// A nest `size` cells across, centered on the origin.
fn nest_mesh(size: f32, seed: u64) -> Mesh {
    let mut rng = Rng::new(seed ^ 0x4E57);
    let mut mesh = BoxMesh::default();
    let half = size / 2.0;
    let pit = half * PIT;
    mesh.quad(
        Vec2::splat(-pit),
        Vec2::splat(pit),
        0.004,
        palette::NEST_PIT,
    );
    // A low lip of chitin around the pit.
    let lip = 0.12;
    for (min, max) in [
        (
            Vec2::new(-pit - lip, -pit - lip),
            Vec2::new(pit + lip, -pit),
        ),
        (Vec2::new(-pit - lip, pit), Vec2::new(pit + lip, pit + lip)),
        (Vec2::new(-pit - lip, -pit), Vec2::new(-pit, pit)),
        (Vec2::new(pit, -pit), Vec2::new(pit + lip, pit)),
    ] {
        mesh.cuboid(
            Vec3::new(min.x, 0.0, min.y),
            Vec3::new(max.x, 0.08, max.y),
            palette::CHITIN_DARK,
        );
    }
    // Spikes around the rim, each a thick base with a thinner tip.
    for i in 0..SPIKES {
        let angle = (i as f32 + rng.range(-0.3, 0.3)) / SPIKES as f32 * std::f32::consts::TAU;
        // Out toward the square's edge.
        let out = Vec2::from_angle(angle);
        let reach = (half - 0.18) / out.x.abs().max(out.y.abs());
        let at = out * reach;
        let height = rng.range(SPIKE_HEIGHT.0, SPIKE_HEIGHT.1);
        let color = vary(palette::CHITIN, &mut rng, 0.04, 10.0);
        let (base, tip) = (0.09, 0.045);
        mesh.cuboid_graded(
            Vec3::new(at.x - base, 0.0, at.y - base),
            Vec3::new(at.x + base, height * 0.55, at.y + base),
            shade(color, 0.7),
            palette::CHITIN_DARK,
        );
        mesh.cuboid(
            Vec3::new(at.x - tip, height * 0.55, at.y - tip),
            Vec3::new(at.x + tip, height, at.y + tip),
            color,
        );
    }
    // Eggs glowing acid green between the spikes and the pit.
    for i in 0..EGGS {
        let angle = (i as f32 + rng.range(0.0, 0.8)) / EGGS as f32 * std::f32::consts::TAU;
        let at = Vec2::from_angle(angle) * rng.range(pit + 0.2, half - 0.3);
        let r = rng.range(0.05, 0.08);
        mesh.cuboid(
            Vec3::new(at.x - r, 0.0, at.y - r),
            Vec3::new(at.x + r, r * 2.2, at.y + r),
            palette::ACID,
        );
    }
    mesh.build()
}

/// A nest heaves as swarmers come out of it, at most so often.
fn heave(
    time: Res<Time>,
    mut emerged: MessageReader<SwarmersEmerged>,
    mut nests: Query<(&mut NestView, &mut Squash)>,
) {
    let now = time.elapsed_secs();
    for SwarmersEmerged { nest } in emerged.read() {
        for (mut view, mut squash) in &mut nests {
            if view.index == *nest && now - view.heaved >= NEST_PULSE_INTERVAL {
                view.heaved = now;
                squash.pulse(NEST_PULSE);
            }
        }
    }
}
