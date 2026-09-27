//! Building voxel models out of cuboids in code.
//!
//! Models are authored in voxel units (one tile is [`VOXELS_PER_TILE`] voxels wide) and merged into
//! a single mesh with per-vertex colors, so one shared material covers everything and every voxel
//! can have its own slightly varied color.

use crate::rng::Rng;
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

/// Voxels along one side of a grid tile.
pub const VOXELS_PER_TILE: f32 = 10.0;
/// World units per voxel.
pub const VOXEL: f32 = 0.1;
/// World units per grid tile.
pub const TILE_SIZE: f32 = VOXELS_PER_TILE * VOXEL;

/// How much darker the bottom edge of a cuboid's side faces is than the top edge.
/// A cheap stand-in for ambient occlusion that makes stacked voxels read clearly.
const SIDE_BOTTOM_SHADE: f32 = 0.82;

/// The shared vertex-colored material used by all voxel meshes.
#[derive(Resource)]
pub struct VoxelMaterial(pub Handle<StandardMaterial>);

impl FromWorld for VoxelMaterial {
    fn from_world(world: &mut World) -> Self {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        Self(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.9,
            reflectance: 0.2,
            ..default()
        }))
    }
}

/// How much brighter than their colors markers are drawn.
const MARKER_BRIGHTNESS: f32 = 1.6;

/// A vertex-colored material that ignores the lighting, for markers that must stay clear in every
/// mood (route dots, aim dots). The vertex colors still carry the faces' baked shading.
#[derive(Resource)]
pub struct MarkerMaterial(pub Handle<StandardMaterial>);

impl FromWorld for MarkerMaterial {
    fn from_world(world: &mut World) -> Self {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        Self(materials.add(StandardMaterial {
            // A little brighter than white, so markers stay bright after tonemapping.
            base_color:
                LinearRgba::rgb(MARKER_BRIGHTNESS, MARKER_BRIGHTNESS, MARKER_BRIGHTNESS).into(),
            unlit: true,
            fog_enabled: false,
            ..default()
        }))
    }
}

/// Accumulates colored cuboids (in voxel units) into one mesh.
#[derive(Default)]
pub struct VoxelMesh {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl VoxelMesh {
    /// A box from `min` to `max`, with side faces slightly darker toward the bottom.
    pub fn cuboid(&mut self, min: Vec3, max: Vec3, color: Color) {
        self.cuboid_graded(min, max, color, shade(color, SIDE_BOTTOM_SHADE));
    }

    /// A box whose side faces blend from `top` (at `max.y`) to `bottom` (at `min.y`).
    pub fn cuboid_graded(&mut self, min: Vec3, max: Vec3, top: Color, bottom: Color) {
        let top = top.to_linear().to_f32_array();
        let bottom = bottom.to_linear().to_f32_array();
        let size = max - min;
        let (dx, dy, dz) = (Vec3::X * size.x, Vec3::Y * size.y, Vec3::Z * size.z);
        // Each face: corner, two edges with `u × v` pointing outward (counter-clockwise front).
        let faces = [
            (Vec3::new(min.x, max.y, min.z), dz, dx, Vec3::Y),
            (min, dx, dz, Vec3::NEG_Y),
            (Vec3::new(max.x, min.y, min.z), dy, dz, Vec3::X),
            (min, dz, dy, Vec3::NEG_X),
            (Vec3::new(min.x, min.y, max.z), dx, dy, Vec3::Z),
            (min, dy, dx, Vec3::NEG_Z),
        ];
        for (corner, u, v, normal) in faces {
            let base = self.positions.len() as u32;
            for p in [corner, corner + u, corner + u + v, corner + v] {
                self.positions.push((p * VOXEL).to_array());
                self.normals.push(normal.to_array());
                let color = if normal == Vec3::Y {
                    top
                } else if normal == Vec3::NEG_Y || p.y <= min.y {
                    bottom
                } else {
                    top
                };
                self.colors.push(color);
            }
            self.indices
                .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    pub fn build(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}

/// Multiplies a color's brightness (in linear space) by `factor`.
pub fn shade(color: Color, factor: f32) -> Color {
    let c = color.to_linear();
    Color::linear_rgba(c.red * factor, c.green * factor, c.blue * factor, c.alpha)
}

/// Randomly nudges a color's lightness and hue, for a hand-painted look.
pub fn vary(color: Color, rng: &mut Rng, lightness: f32, hue: f32) -> Color {
    let mut c = Oklcha::from(color);
    c.lightness = (c.lightness + rng.range(-lightness, lightness)).clamp(0.0, 1.0);
    c.hue = (c.hue + rng.range(-hue, hue)).rem_euclid(360.0);
    c.into()
}

/// Four L-shaped corners around a tile, in voxels.
pub fn bracket_mesh() -> VoxelMesh {
    let mut mesh = VoxelMesh::default();
    const OUTER: f32 = 5.2;
    const ARM: f32 = 2.2;
    const THICK: f32 = 0.7;
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let corner = Vec3::new(sx * OUTER, 0.0, sz * OUTER);
        let inward_x = Vec3::new(-sx * ARM, 0.5, -sz * THICK);
        let inward_z = Vec3::new(-sx * THICK, 0.5, -sz * ARM);
        for inward in [inward_x, inward_z] {
            let other = corner + inward;
            mesh.cuboid(corner.min(other), corner.max(other), Color::WHITE);
        }
    }
    mesh
}
