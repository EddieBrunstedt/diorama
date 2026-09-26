//! Voxel models for the keeper: a little gnome with a pointy red hat and a backpack for logs.
//!
//! Everything faces +z and is in voxel units. The body's origin is on the ground between the feet;
//! the arm's origin is its shoulder joint, so rotating it swings the tool.

use crate::palette;
use crate::voxel::{VoxelMesh, shade};
use bevy::prelude::*;

/// Where the tool arm's shoulder sits on the body (voxels).
pub const SHOULDER: Vec3 = Vec3::new(1.15, 2.1, 0.0);
/// Where the feet sit, left and right (voxels).
pub const FEET: [Vec3; 2] = [Vec3::new(-0.5, 0.0, 0.0), Vec3::new(0.5, 0.0, 0.0)];
/// Where the nth log sits on the backpack (voxels): stacked high behind the head.
pub fn log_slot(index: usize) -> Vec3 {
    let wobble = if index.is_multiple_of(2) { -0.12 } else { 0.12 };
    Vec3::new(wobble, 2.45 + index as f32 * 0.5, -1.2)
}

pub fn body_mesh() -> VoxelMesh {
    let mut mesh = VoxelMesh::default();
    let tunic = palette::KEEPER_TUNIC;
    let skin = palette::KEEPER_SKIN;

    // Tunic with a flared hem and a belt.
    mesh.cuboid(
        Vec3::new(-1.0, 0.7, -0.75),
        Vec3::new(1.0, 2.3, 0.75),
        tunic,
    );
    mesh.cuboid(
        Vec3::new(-1.1, 0.6, -0.85),
        Vec3::new(1.1, 1.0, 0.85),
        shade(tunic, 0.9),
    );
    mesh.cuboid(
        Vec3::new(-1.05, 1.3, -0.8),
        Vec3::new(1.05, 1.55, 0.8),
        palette::KEEPER_BELT,
    );
    mesh.cuboid(
        Vec3::new(-0.2, 1.28, 0.8),
        Vec3::new(0.2, 1.57, 0.86),
        palette::FLAG,
    );

    // Left arm hanging at the side.
    mesh.cuboid(
        Vec3::new(-1.38, 1.25, -0.3),
        Vec3::new(-1.0, 2.2, 0.3),
        tunic,
    );
    mesh.cuboid(
        Vec3::new(-1.36, 0.95, -0.25),
        Vec3::new(-1.02, 1.25, 0.25),
        skin,
    );

    // Big head with a bushy beard, a round nose and two dot eyes.
    mesh.cuboid(Vec3::new(-0.85, 2.3, -0.8), Vec3::new(0.85, 3.7, 0.8), skin);
    mesh.cuboid(
        Vec3::new(-0.85, 1.9, 0.45),
        Vec3::new(0.85, 2.95, 1.0),
        palette::KEEPER_BEARD,
    );
    mesh.cuboid(
        Vec3::new(-0.5, 1.55, 0.55),
        Vec3::new(0.5, 1.9, 0.95),
        palette::KEEPER_BEARD,
    );
    mesh.cuboid(
        Vec3::new(-0.25, 2.8, 0.8),
        Vec3::new(0.25, 3.2, 1.2),
        palette::KEEPER_NOSE,
    );
    for x in [-0.55, 0.3] {
        mesh.cuboid(
            Vec3::new(x, 3.15, 0.8),
            Vec3::new(x + 0.25, 3.42, 0.84),
            palette::KEEPER_EYES,
        );
    }

    // Pointy hat, drooping backward at the tip.
    let hat = palette::KEEPER_HAT;
    mesh.cuboid(
        Vec3::new(-1.0, 3.6, -0.95),
        Vec3::new(1.0, 4.0, 0.95),
        shade(hat, 0.9),
    );
    mesh.cuboid(
        Vec3::new(-0.75, 4.0, -0.75),
        Vec3::new(0.75, 4.7, 0.65),
        hat,
    );
    mesh.cuboid(Vec3::new(-0.5, 4.7, -0.6), Vec3::new(0.5, 5.3, 0.35), hat);
    mesh.cuboid(
        Vec3::new(-0.28, 5.3, -0.55),
        Vec3::new(0.28, 5.8, 0.05),
        hat,
    );
    mesh.cuboid(
        Vec3::new(-0.15, 5.6, -0.95),
        Vec3::new(0.15, 5.9, -0.45),
        hat,
    );

    // Backpack with a flap.
    mesh.cuboid(
        Vec3::new(-0.85, 1.0, -1.45),
        Vec3::new(0.85, 2.35, -0.75),
        palette::BACKPACK,
    );
    mesh.cuboid(
        Vec3::new(-0.9, 2.05, -1.5),
        Vec3::new(0.9, 2.45, -0.7),
        shade(palette::BACKPACK, 0.8),
    );
    mesh
}

/// One boot, centered under its leg.
pub fn foot_mesh() -> VoxelMesh {
    let mut mesh = VoxelMesh::default();
    mesh.cuboid(
        Vec3::new(-0.35, 0.0, -0.4),
        Vec3::new(0.35, 0.7, 0.6),
        palette::KEEPER_BOOTS,
    );
    mesh
}

/// The right arm, hanging down from the shoulder, with the hand at the bottom.
pub fn arm_mesh() -> VoxelMesh {
    let mut mesh = VoxelMesh::default();
    mesh.cuboid(
        Vec3::new(-0.2, -0.95, -0.3),
        Vec3::new(0.2, 0.1, 0.3),
        palette::KEEPER_TUNIC,
    );
    mesh.cuboid(
        Vec3::new(-0.18, -1.25, -0.25),
        Vec3::new(0.18, -0.95, 0.25),
        palette::KEEPER_SKIN,
    );
    mesh
}

/// An axe held in the hand, pointing forward.
pub fn axe_mesh() -> VoxelMesh {
    let mut mesh = VoxelMesh::default();
    mesh.cuboid(
        Vec3::new(-0.1, -1.2, -0.2),
        Vec3::new(0.1, -1.0, 1.8),
        palette::TOOL_HANDLE,
    );
    mesh.cuboid(
        Vec3::new(-0.08, -1.3, 1.2),
        Vec3::new(0.08, -0.45, 1.75),
        palette::TOOL_METAL,
    );
    mesh.cuboid(
        Vec3::new(-0.06, -0.55, 1.15),
        Vec3::new(0.06, -0.35, 1.8),
        shade(palette::TOOL_METAL, 1.15),
    );
    mesh
}

/// A mallet held in the hand, pointing forward.
pub fn hammer_mesh() -> VoxelMesh {
    let mut mesh = VoxelMesh::default();
    mesh.cuboid(
        Vec3::new(-0.1, -1.2, -0.2),
        Vec3::new(0.1, -1.0, 1.4),
        palette::TOOL_HANDLE,
    );
    mesh.cuboid(
        Vec3::new(-0.3, -1.45, 1.1),
        Vec3::new(0.3, -0.75, 1.65),
        shade(palette::BARK, 1.1),
    );
    mesh
}

/// A log lying crosswise, with light cut ends.
pub fn log_mesh() -> VoxelMesh {
    let mut mesh = VoxelMesh::default();
    mesh.cuboid(
        Vec3::new(-0.9, 0.0, -0.25),
        Vec3::new(0.9, 0.5, 0.25),
        palette::BARK,
    );
    for (from, to) in [(-0.96, -0.9), (0.9, 0.96)] {
        mesh.cuboid(
            Vec3::new(from, 0.08, -0.17),
            Vec3::new(to, 0.42, 0.17),
            palette::LOG_END,
        );
    }
    mesh
}
