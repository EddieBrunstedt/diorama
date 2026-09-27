//! Building grey-box meshes out of boxes in code.
//!
//! Boxes are merged into one mesh with per-vertex colors, so one shared material covers
//! everything and each box can have its own color.

use crate::rng::Rng;
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

/// How much darker the bottom edge of a box's side faces is than the top edge. A cheap stand-in
/// for ambient occlusion that grounds things.
const SIDE_BOTTOM_SHADE: f32 = 0.7;

/// The shared vertex-colored material for [`BoxMesh`] meshes.
#[derive(Resource)]
pub struct ShapeMaterial(pub Handle<StandardMaterial>);

impl FromWorld for ShapeMaterial {
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

/// Accumulates colored boxes into one mesh.
#[derive(Default)]
pub struct BoxMesh {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl BoxMesh {
    /// A box from `min` to `max`, with side faces darker toward the bottom.
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
                self.positions.push(p.to_array());
                self.normals.push(normal.to_array());
                let color = if normal == Vec3::NEG_Y || (normal != Vec3::Y && p.y <= min.y) {
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

    /// A flat, upward-facing rectangle at height `y`.
    pub fn quad(&mut self, min: Vec2, max: Vec2, y: f32, color: Color) {
        let color = color.to_linear().to_f32_array();
        let base = self.positions.len() as u32;
        for (x, z) in [
            (min.x, min.y),
            (min.x, max.y),
            (max.x, max.y),
            (max.x, min.y),
        ] {
            self.positions.push([x, y, z]);
            self.normals.push([0.0, 1.0, 0.0]);
            self.colors.push(color);
        }
        self.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
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

/// Randomly nudges a color's lightness and hue, so large areas don't look flat.
pub fn vary(color: Color, rng: &mut Rng, lightness: f32, hue: f32) -> Color {
    let mut c = Oklcha::from(color);
    c.lightness = (c.lightness + rng.range(-lightness, lightness)).clamp(0.0, 1.0);
    c.hue = (c.hue + rng.range(-hue, hue)).rem_euclid(360.0);
    c.into()
}

/// Four L-shaped corners framing a 1×1 square centered on the origin, lying on the ground.
/// `outer` is the half size of the frame, `arm` how far each corner reaches along an edge.
pub fn bracket_mesh(outer: f32, arm: f32, thickness: f32, height: f32) -> Mesh {
    let mut mesh = BoxMesh::default();
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let corner = Vec3::new(sx * outer, 0.0, sz * outer);
        let along_x = Vec3::new(-sx * arm, height, -sz * thickness);
        let along_z = Vec3::new(-sx * thickness, height, -sz * arm);
        for along in [along_x, along_z] {
            let other = corner + along;
            mesh.cuboid_graded(
                corner.min(other),
                corner.max(other),
                Color::WHITE,
                Color::WHITE,
            );
        }
    }
    mesh.build()
}
