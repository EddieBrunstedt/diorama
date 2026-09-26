//! The slime's voxel model: a boxy lilac blob with a darker skirt, a shiny top, big dot eyes and
//! rosy cheeks. Faces +z, in voxel units, standing on y = 0.

use crate::palette;
use crate::voxel::{VoxelMesh, shade};
use bevy::prelude::*;

/// Height of the slime's middle, for aiming arrows at it (voxels).
pub const MIDDLE: f32 = 1.5;

pub fn slime_mesh() -> VoxelMesh {
    let mut mesh = VoxelMesh::default();
    let body = palette::SLIME;
    let light = body.mix(&Color::WHITE, 0.25);

    // A wide skirt where it sags onto the ground, the body, and a smaller rounded top.
    mesh.cuboid(
        Vec3::new(-2.1, 0.0, -2.0),
        Vec3::new(2.1, 0.7, 2.0),
        shade(body, 0.82),
    );
    mesh.cuboid(Vec3::new(-1.9, 0.5, -1.8), Vec3::new(1.9, 2.5, 1.8), body);
    mesh.cuboid(Vec3::new(-1.4, 2.5, -1.3), Vec3::new(1.4, 3.1, 1.3), light);

    // A glossy highlight on the top left.
    let shine = body.mix(&Color::WHITE, 0.75);
    mesh.cuboid(Vec3::new(-1.2, 3.1, -0.4), Vec3::new(-0.4, 3.2, 0.6), shine);
    mesh.cuboid(Vec3::new(-1.95, 1.6, 0.4), Vec3::new(-1.9, 2.2, 1.2), shine);

    // Eyes with a glint, a tiny mouth and blushing cheeks on the front.
    let front = 1.8;
    for x in [-1.0, 0.45] {
        mesh.cuboid(
            Vec3::new(x, 1.3, front),
            Vec3::new(x + 0.55, 2.1, front + 0.08),
            palette::KEEPER_EYES,
        );
        mesh.cuboid(
            Vec3::new(x + 0.08, 1.8, front + 0.08),
            Vec3::new(x + 0.28, 2.0, front + 0.1),
            Color::WHITE,
        );
    }
    mesh.cuboid(
        Vec3::new(-0.25, 1.0, front),
        Vec3::new(0.25, 1.15, front + 0.08),
        palette::KEEPER_EYES,
    );
    for x in [-1.65, 1.05] {
        mesh.cuboid(
            Vec3::new(x, 1.05, front),
            Vec3::new(x + 0.6, 1.3, front + 0.06),
            palette::FLOWERS[2],
        );
    }
    mesh
}
