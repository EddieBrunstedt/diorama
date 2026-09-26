//! Voxel trees on forest tiles. Each tree is its own entity so it can be chopped later.

use crate::palette;
use crate::rng::Rng;
use crate::tuning::TREES_PER_FOREST_TILE;
use crate::voxel::{VoxelMesh, shade, vary};
use bevy::prelude::*;

const SPOT_SALT: u64 = 2;
const MODEL_SALT: u64 = 3;

/// Where trees may stand on a forest tile (voxel units from the tile center).
const SPOTS: [Vec2; 5] = [
    Vec2::new(-2.3, -2.3),
    Vec2::new(2.3, -2.3),
    Vec2::new(-2.3, 2.3),
    Vec2::new(2.3, 2.3),
    Vec2::new(0.0, 0.0),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TreeStyle {
    Pine,
    Round,
    Autumn,
}

#[derive(Clone, Copy, Debug)]
pub struct TreeSpot {
    /// Position on the tile in voxel units, relative to the tile center.
    pub position: Vec2,
    pub style: TreeStyle,
    /// Rotation in quarter turns, so voxels stay aligned to the grid.
    pub quarter_turns: u8,
    /// Uniform scale. Varies a little so trees don't look copy-pasted.
    pub scale: f32,
}

/// The trees on a forest tile. Deterministic per cell.
pub fn tree_spots(cell: IVec2) -> Vec<TreeSpot> {
    let mut rng = Rng::for_cell(cell, SPOT_SALT);
    let mut spots = SPOTS.to_vec();
    (0..TREES_PER_FOREST_TILE.min(SPOTS.len()))
        .map(|_| {
            let spot = spots.swap_remove(rng.index(spots.len()));
            let style = match rng.f32() {
                x if x < 0.5 => TreeStyle::Pine,
                x if x < 0.82 => TreeStyle::Round,
                _ => TreeStyle::Autumn,
            };
            TreeSpot {
                position: spot + Vec2::new(rng.range(-0.5, 0.5), rng.range(-0.5, 0.5)),
                style,
                quarter_turns: rng.index(4) as u8,
                scale: rng.range(0.85, 1.12),
            }
        })
        .collect()
}

/// A tree model with its base at the origin.
pub fn tree_mesh(cell: IVec2, index: usize, style: TreeStyle) -> VoxelMesh {
    let mut rng = Rng::for_cell(cell, MODEL_SALT + index as u64 * 7919);
    let mut mesh = VoxelMesh::default();
    let bark = vary(palette::BARK, &mut rng, 0.03, 6.0);
    match style {
        TreeStyle::Pine => {
            mesh.cuboid(Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 2.0, 0.5), bark);
            // (half width, bottom, top) per tier, widest at the bottom.
            let tiers = [
                (2.5, 1.5, 3.0),
                (2.0, 3.0, 4.5),
                (1.5, 4.5, 5.8),
                (0.5, 5.8, 6.8),
            ];
            for (k, (half, bottom, top)) in tiers.into_iter().enumerate() {
                let t = k as f32 / (tiers.len() - 1) as f32;
                let color = vary(
                    palette::PINE.mix(&palette::PINE_TIP, t),
                    &mut rng,
                    0.02,
                    5.0,
                );
                mesh.cuboid(
                    Vec3::new(-half, bottom, -half),
                    Vec3::new(half, top, half),
                    color,
                );
            }
        }
        TreeStyle::Round | TreeStyle::Autumn => {
            let (leaves, hue) = match style {
                TreeStyle::Autumn => (palette::LEAVES_AUTUMN, 14.0),
                _ => (palette::LEAVES, 6.0),
            };
            mesh.cuboid(Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 2.6, 0.5), bark);
            // A stepped crown: wide and dark below, narrower and lighter on top.
            let crown = vary(leaves, &mut rng, 0.03, hue);
            mesh.cuboid(
                Vec3::new(-2.2, 2.2, -2.2),
                Vec3::new(2.2, 4.4, 2.2),
                shade(crown, 0.85),
            );
            mesh.cuboid(Vec3::new(-1.6, 4.4, -1.6), Vec3::new(1.6, 5.8, 1.6), crown);
            // Lumps poking out of the crown's sides.
            for _ in 0..3 + rng.index(3) {
                let side = rng.pick(&[-1.0, 1.0]);
                let across = rng.range(-1.2, 1.2);
                let (x, z) = if rng.chance(0.5) {
                    (side * 2.0, across)
                } else {
                    (across, side * 2.0)
                };
                let center = Vec3::new(x, rng.range(2.8, 4.6), z);
                let half = Vec3::new(
                    rng.range(0.6, 0.9),
                    rng.range(0.5, 0.8),
                    rng.range(0.6, 0.9),
                );
                let color = vary(leaves, &mut rng, 0.06, hue);
                mesh.cuboid(center - half, center + half, color);
            }
        }
    }
    mesh
}

/// What's left after chopping: a short trunk with a light cut face on top.
pub fn stump_mesh() -> VoxelMesh {
    let mut mesh = VoxelMesh::default();
    mesh.cuboid(
        Vec3::new(-0.6, 0.0, -0.6),
        Vec3::new(0.6, 0.7, 0.6),
        palette::BARK,
    );
    mesh.cuboid(
        Vec3::new(-0.45, 0.7, -0.45),
        Vec3::new(0.45, 0.8, 0.45),
        palette::LOG_END,
    );
    // A couple of roots poking out.
    mesh.cuboid(
        Vec3::new(0.6, 0.0, -0.2),
        Vec3::new(1.0, 0.3, 0.2),
        palette::BARK,
    );
    mesh.cuboid(
        Vec3::new(-0.2, 0.0, -1.0),
        Vec3::new(0.2, 0.25, -0.6),
        palette::BARK,
    );
    mesh
}
