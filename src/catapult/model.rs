//! The catapult's voxel models: a wheeled wooden frame with a winch and a padded crossbar, a
//! throwing arm with a bucket, and the boulder it throws. Faces +z (the way it throws), in voxel
//! units, standing on y = 0.
//!
//! The arm is its own mesh, built standing straight up from its pivot, so rotating it around x
//! swings it: at rest it lies back, loaded; fired, it swings up against the crossbar.

use crate::palette;
use crate::rng::Rng;
use crate::voxel::{VoxelMesh, shade, vary};
use bevy::prelude::*;

/// The catapult is modeled a bit small and scaled up, so it's the biggest thing on its tile.
pub const SCALE: f32 = 1.2;
/// Where the arm's pivot sits on the frame (voxels).
pub const PIVOT: Vec3 = Vec3::new(0.0, 2.0, 1.2);
/// Arm angles (radians around x; 0 is straight up, negative leans back): lying back loaded, and
/// stopped against the crossbar after a throw.
pub const ARM_REST: f32 = -std::f32::consts::FRAC_PI_2;
pub const ARM_STOP: f32 = 0.28;
/// Where the boulder sits in the bucket, in the arm's space (voxels).
pub const BUCKET: Vec3 = Vec3::new(0.0, 5.3, 0.75);
/// Where the keeper stands to work the catapult: on a step at its left side, by the winch.
pub const SEAT: Vec3 = Vec3::new(-3.3, 1.0, -1.4);

const MODEL_SALT: u64 = 11;

/// The frame: side beams on four chunky wheels, a winch at the back, uprights with a padded
/// crossbar at the front, and a plank step for the keeper.
pub fn base_mesh() -> VoxelMesh {
    let mut mesh = VoxelMesh::default();
    add_base(&mut mesh);
    mesh
}

fn add_base(mesh: &mut VoxelMesh) {
    let mut rng = Rng::new(MODEL_SALT);
    let wood = |rng: &mut Rng| vary(palette::PLANK, rng, 0.04, 6.0);
    let dark = shade(palette::BARK, 0.75);

    // Wheels: a cross of two boxes each, with a metal hub.
    for x in [-2.9, 2.3] {
        for z in [-2.4, 2.1] {
            mesh.cuboid(
                Vec3::new(x, 0.25, z - 0.95),
                Vec3::new(x + 0.6, 1.65, z + 0.95),
                dark,
            );
            mesh.cuboid(
                Vec3::new(x, 0.0, z - 0.7),
                Vec3::new(x + 0.6, 1.9, z + 0.7),
                dark,
            );
            let hub_x = if x < 0.0 { x - 0.15 } else { x + 0.45 };
            mesh.cuboid(
                Vec3::new(hub_x, 0.7, z - 0.25),
                Vec3::new(hub_x + 0.3, 1.2, z + 0.25),
                palette::TOOL_METAL,
            );
        }
    }

    // Side beams and cross beams.
    for x in [-2.3, 1.5] {
        mesh.cuboid(
            Vec3::new(x, 0.9, -3.6),
            Vec3::new(x + 0.8, 1.7, 3.3),
            wood(&mut rng),
        );
    }
    for (z0, z1) in [(-3.6, -2.9), (2.6, 3.3)] {
        mesh.cuboid(
            Vec3::new(-1.5, 1.0, z0),
            Vec3::new(1.5, 1.6, z1),
            wood(&mut rng),
        );
    }

    // The axle the arm swings on, in blocks on the beams.
    for x in [-1.6, 1.0] {
        mesh.cuboid(
            Vec3::new(x, 1.7, PIVOT.z - 0.5),
            Vec3::new(x + 0.6, 2.6, PIVOT.z + 0.5),
            palette::BARK,
        );
    }
    mesh.cuboid(
        Vec3::new(-1.0, PIVOT.y - 0.2, PIVOT.z - 0.2),
        Vec3::new(1.0, PIVOT.y + 0.2, PIVOT.z + 0.2),
        palette::TOOL_METAL,
    );

    // Uprights and the padded crossbar the arm stops against.
    for x in [-2.3, 1.7] {
        mesh.cuboid(
            Vec3::new(x, 1.7, 2.4),
            Vec3::new(x + 0.6, 5.4, 3.0),
            vary(palette::BARK, &mut rng, 0.03, 6.0),
        );
        // A brace down to the beam.
        mesh.cuboid(
            Vec3::new(x, 1.7, 3.0),
            Vec3::new(x + 0.6, 3.0, 3.6),
            palette::BARK,
        );
    }
    mesh.cuboid(
        Vec3::new(-2.4, 5.0, 2.3),
        Vec3::new(2.4, 5.8, 3.1),
        wood(&mut rng),
    );
    mesh.cuboid(
        Vec3::new(-0.9, 4.8, 2.0),
        Vec3::new(0.9, 5.9, 2.4),
        palette::ROPE,
    );

    // The winch at the back: a rope drum with crank handles.
    mesh.cuboid(
        Vec3::new(-1.5, 1.8, -2.9),
        Vec3::new(1.5, 2.6, -2.1),
        palette::ROPE,
    );
    for x in [-2.5, 2.3] {
        mesh.cuboid(
            Vec3::new(x, 2.0, -2.6),
            Vec3::new(x + 0.2, 3.0, -2.4),
            palette::BARK,
        );
    }

    // The keeper's step, on the left.
    mesh.cuboid(
        Vec3::new(-4.3, 0.6, -2.6),
        Vec3::new(-2.3, 1.0, -0.2),
        wood(&mut rng),
    );
    mesh.cuboid(
        Vec3::new(-4.1, 0.0, -2.4),
        Vec3::new(-3.7, 0.6, -2.0),
        palette::BARK,
    );
    mesh.cuboid(
        Vec3::new(-4.1, 0.0, -0.8),
        Vec3::new(-3.7, 0.6, -0.4),
        palette::BARK,
    );
}

/// The arm's boxes, standing straight up from the pivot at the origin: a beam with iron bands,
/// and a bucket opening toward +z (up, when the arm lies back).
fn arm_boxes() -> Vec<(Vec3, Vec3, Color)> {
    let beam = shade(palette::PLANK, 0.9);
    let bucket = palette::BARK;
    vec![
        (Vec3::new(-0.4, -0.7, -0.4), Vec3::new(0.4, 4.6, 0.4), beam),
        (
            Vec3::new(-0.5, 1.4, -0.5),
            Vec3::new(0.5, 1.8, 0.5),
            palette::TOOL_METAL,
        ),
        (
            Vec3::new(-0.5, 3.3, -0.5),
            Vec3::new(0.5, 3.7, 0.5),
            palette::TOOL_METAL,
        ),
        // The bucket: a back plate, two sides and two ends.
        (Vec3::new(-1.1, 4.3, -0.4), Vec3::new(1.1, 6.3, 0.1), bucket),
        (Vec3::new(-1.1, 4.3, 0.1), Vec3::new(-0.8, 6.3, 1.1), bucket),
        (Vec3::new(0.8, 4.3, 0.1), Vec3::new(1.1, 6.3, 1.1), bucket),
        (Vec3::new(-0.8, 4.3, 0.1), Vec3::new(0.8, 4.6, 1.0), bucket),
        (Vec3::new(-0.8, 6.0, 0.1), Vec3::new(0.8, 6.3, 1.0), bucket),
    ]
}

pub fn arm_mesh() -> VoxelMesh {
    let mut mesh = VoxelMesh::default();
    for (min, max, color) in arm_boxes() {
        mesh.cuboid(min, max, color);
    }
    mesh
}

/// A roughly round lump of stone, centered on the origin.
pub fn boulder_mesh() -> VoxelMesh {
    let mut rng = Rng::new(MODEL_SALT + 1);
    let mut mesh = VoxelMesh::default();
    let stone = |rng: &mut Rng| vary(palette::BOULDER, rng, 0.05, 8.0);
    mesh.cuboid(Vec3::splat(-0.6), Vec3::splat(0.6), stone(&mut rng));
    mesh.cuboid(
        Vec3::new(-0.8, -0.4, -0.4),
        Vec3::new(0.8, 0.4, 0.4),
        stone(&mut rng),
    );
    mesh.cuboid(
        Vec3::new(-0.4, -0.8, -0.4),
        Vec3::new(0.4, 0.8, 0.4),
        stone(&mut rng),
    );
    mesh.cuboid(
        Vec3::new(-0.4, -0.4, -0.8),
        Vec3::new(0.4, 0.4, 0.8),
        stone(&mut rng),
    );
    mesh.cuboid(
        Vec3::new(-0.5, 0.35, -0.2),
        Vec3::new(-0.1, 0.65, 0.2),
        palette::PEBBLE.mix(&Color::WHITE, 0.3),
    );
    mesh
}

/// The whole catapult with its arm lying back, as one mesh, for blueprint ghosts.
pub fn ghost_mesh() -> VoxelMesh {
    let mut mesh = VoxelMesh::default();
    add_base(&mut mesh);
    // Lying back (a quarter turn around x): the arm's (x, y, z) becomes (x, z, -y).
    for (min, max, color) in arm_boxes() {
        let a = PIVOT + Vec3::new(min.x, min.z, -min.y);
        let b = PIVOT + Vec3::new(max.x, max.z, -max.y);
        mesh.cuboid(a.min(b), a.max(b), color);
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ghost_arm_lies_where_the_real_one_rests() {
        // The bucket end of the arm, lying back, is behind the pivot and above the ground.
        let rest = Quat::from_rotation_x(ARM_REST);
        let bucket = PIVOT + rest * BUCKET;
        assert!(bucket.z < PIVOT.z - 4.0);
        assert!(bucket.y > 0.5);
        let tip = Vec3::new(0.0, 6.3, 0.0);
        let baked = PIVOT + Vec3::new(tip.x, tip.z, -tip.y);
        assert!((PIVOT + rest * tip).distance(baked) < 1e-4);
    }
}
