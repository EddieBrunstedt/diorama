//! Grey-box tower meshes: a concrete plinth with a hazard band (like the walls, so everything
//! built reads as one outpost), a fixed base, and a turret that swings around a pivot. Turrets
//! face -Z, like every model here.

use crate::grid::Kind;
use crate::palette;
use crate::shapes::{BoxMesh, shade};
use bevy::prelude::*;

/// A tower's parts, in its own space (its cell's middle on the ground is the origin).
pub struct Layout {
    /// Where the turret turns, above the middle of the cell.
    pub pivot: f32,
    /// The barrel's tip in the turret's space (for flashes, tracers and shells).
    pub muzzle: Vec3,
    /// The top of the tallest part, for picking.
    pub height: f32,
}

pub fn layout(kind: Kind) -> Layout {
    match kind {
        // Walls have no turret; they're drawn by `structures`.
        Kind::Wall => Layout {
            pivot: 0.0,
            muzzle: Vec3::ZERO,
            height: 0.0,
        },
        Kind::Autocannon => Layout {
            pivot: 0.72,
            muzzle: Vec3::new(0.0, 0.16, -0.74),
            height: 0.72 + 0.41,
        },
        Kind::Flamer => Layout {
            pivot: 0.58,
            muzzle: Vec3::new(0.0, 0.14, -0.58),
            height: 0.58 + 0.32,
        },
        Kind::Mortar => Layout {
            pivot: 0.3,
            muzzle: MORTAR_TUBE.0 + MORTAR_TUBE.1 * MORTAR_TUBE.2,
            height: 0.3 + 0.74,
        },
        Kind::ArtilleryCommand => Layout {
            pivot: ARTILLERY_PIVOT,
            muzzle: ROCKET_RACK.0 + ROCKET_RACK.1 * ROCKET_RACK.2,
            height: ARTILLERY_ANTENNA_TOP,
        },
    }
}

/// The Artillery Command's turret turns on top of its bunker, and its antenna (on the bunker)
/// is the tallest part.
const ARTILLERY_PIVOT: f32 = 0.5;
const ARTILLERY_ANTENNA_TOP: f32 = 1.18;

/// The Artillery Command's rack of launch tubes: where its middle starts, which way the tubes
/// point (up and forward) and how long they are.
const ROCKET_RACK: (Vec3, Vec3, f32) = (
    Vec3::new(0.0, 0.14, 0.1),
    Vec3::new(0.0, 0.819, -0.574),
    0.44,
);

/// The mortar's tube: where it starts, which way it points (up and forward) and how long it is.
const MORTAR_TUBE: (Vec3, Vec3, f32) = (
    Vec3::new(0.0, 0.08, 0.14),
    Vec3::new(0.0, 0.819, -0.574),
    0.74,
);

/// The concrete plinth every tower stands on.
fn plinth(mesh: &mut BoxMesh) {
    let half = 0.44;
    mesh.cuboid_graded(
        Vec3::new(-half, 0.0, -half),
        Vec3::new(half, 0.18, half),
        shade(palette::CONCRETE, 0.85),
        shade(palette::CONCRETE, 0.45),
    );
    mesh.cuboid(
        Vec3::new(-half + 0.02, 0.18, -half + 0.02),
        Vec3::new(half - 0.02, 0.23, half - 0.02),
        palette::HAZARD,
    );
}

/// The part that doesn't turn.
pub fn base_mesh(kind: Kind) -> Mesh {
    let mut mesh = BoxMesh::default();
    plinth(&mut mesh);
    let steel = palette::STEEL;
    match kind {
        Kind::Wall => {}
        Kind::Autocannon => {
            // A tall armored pedestal.
            mesh.cuboid(
                Vec3::new(-0.26, 0.23, -0.26),
                Vec3::new(0.26, 0.4, 0.26),
                shade(steel, 0.8),
            );
            mesh.cuboid(
                Vec3::new(-0.18, 0.4, -0.18),
                Vec3::new(0.18, 0.72, 0.18),
                steel,
            );
        }
        Kind::Flamer => {
            // A squat drum with a hazard ring.
            mesh.cuboid(
                Vec3::new(-0.32, 0.23, -0.32),
                Vec3::new(0.32, 0.46, 0.32),
                shade(palette::GUNMETAL, 1.2),
            );
            mesh.cuboid(
                Vec3::new(-0.33, 0.4, -0.33),
                Vec3::new(0.33, 0.45, 0.33),
                palette::HAZARD,
            );
            mesh.cuboid(
                Vec3::new(-0.2, 0.46, -0.2),
                Vec3::new(0.2, 0.58, 0.2),
                steel,
            );
        }
        Kind::Mortar => {
            // A ring of sandbags around a floor plate.
            mesh.cuboid(
                Vec3::new(-0.34, 0.23, -0.34),
                Vec3::new(0.34, 0.3, 0.34),
                shade(steel, 0.7),
            );
            let bags = [
                (-0.3, -0.3, 0.2, 0.1),
                (0.0, -0.36, 0.2, 0.08),
                (0.3, -0.3, 0.2, 0.1),
                (0.36, 0.0, 0.08, 0.2),
                (0.3, 0.3, 0.2, 0.1),
                (0.0, 0.36, 0.2, 0.08),
                (-0.3, 0.3, 0.2, 0.1),
                (-0.36, 0.0, 0.08, 0.2),
            ];
            for (i, (x, z, w, d)) in bags.into_iter().enumerate() {
                let color = shade(palette::SANDBAG, if i % 2 == 0 { 1.0 } else { 0.85 });
                mesh.cuboid(
                    Vec3::new(x - w / 2.0 - 0.04, 0.23, z - d / 2.0 - 0.04),
                    Vec3::new(x + w / 2.0 + 0.04, 0.42, z + d / 2.0 + 0.04),
                    color,
                );
            }
        }
        Kind::ArtilleryCommand => {
            // A squat bunker with a slit of lit windows and a hazard edge on its roof.
            let half = 0.36;
            mesh.cuboid_graded(
                Vec3::new(-half, 0.23, -half),
                Vec3::new(half, ARTILLERY_PIVOT - 0.04, half),
                shade(palette::CONCRETE, 0.95),
                shade(palette::CONCRETE, 0.6),
            );
            mesh.cuboid(
                Vec3::new(-half - 0.01, 0.34, half - 0.2),
                Vec3::new(half + 0.01, 0.39, half - 0.06),
                palette::BASE_LIGHT,
            );
            mesh.cuboid(
                Vec3::new(-half + 0.06, 0.34, half - 0.01),
                Vec3::new(half - 0.06, 0.39, half + 0.01),
                palette::BASE_LIGHT,
            );
            mesh.cuboid(
                Vec3::new(-half, ARTILLERY_PIVOT - 0.04, -half),
                Vec3::new(half, ARTILLERY_PIVOT, half),
                palette::HAZARD,
            );
            // An antenna in the back corner, with a light on top.
            let mast = Vec2::new(0.27, 0.27);
            mesh.cuboid(
                Vec3::new(mast.x - 0.025, ARTILLERY_PIVOT, mast.y - 0.025),
                Vec3::new(mast.x + 0.025, ARTILLERY_ANTENNA_TOP - 0.07, mast.y + 0.025),
                steel,
            );
            mesh.cuboid(
                Vec3::new(mast.x - 0.05, ARTILLERY_ANTENNA_TOP - 0.07, mast.y - 0.05),
                Vec3::new(mast.x + 0.05, ARTILLERY_ANTENNA_TOP, mast.y + 0.05),
                palette::BASE_LIGHT,
            );
        }
    }
    mesh.build()
}

/// The part that turns, around the pivot.
pub fn turret_mesh(kind: Kind) -> Mesh {
    let mut mesh = BoxMesh::default();
    let (gun, steel, light) = (palette::GUNMETAL, palette::STEEL, palette::BASE_LIGHT);
    let mut part = |min: [f32; 3], max: [f32; 3], color: Color| {
        mesh.cuboid(Vec3::from(min), Vec3::from(max), color);
    };
    match kind {
        Kind::Wall => {}
        Kind::Autocannon => {
            part([-0.24, 0.0, -0.22], [0.24, 0.3, 0.26], gun);
            part([-0.2, 0.3, -0.16], [0.2, 0.36, 0.22], shade(steel, 1.2));
            // A sensor light, and an ammo box on the side.
            part([0.08, 0.36, -0.12], [0.16, 0.41, -0.04], light);
            part([0.24, 0.04, -0.06], [0.33, 0.22, 0.18], palette::HAZARD);
            // Twin barrels with muzzle brakes.
            for x in [-0.09, 0.09] {
                part(
                    [x - 0.035, 0.12, -0.66],
                    [x + 0.035, 0.2, -0.2],
                    shade(gun, 0.8),
                );
                part(
                    [x - 0.05, 0.1, -0.74],
                    [x + 0.05, 0.22, -0.64],
                    shade(gun, 0.6),
                );
            }
        }
        Kind::Flamer => {
            part([-0.2, 0.0, -0.18], [0.2, 0.24, 0.18], gun);
            // Two fuel tanks on its back.
            for x in [-0.12, 0.12] {
                part([x - 0.08, 0.02, 0.16], [x + 0.08, 0.3, 0.4], palette::FUEL);
                part(
                    [x - 0.085, 0.2, 0.15],
                    [x + 0.085, 0.24, 0.41],
                    palette::HAZARD,
                );
            }
            // A thick nozzle with a pilot light under its tip.
            part([-0.07, 0.07, -0.5], [0.07, 0.19, -0.16], shade(gun, 0.8));
            part([-0.09, 0.05, -0.58], [0.09, 0.22, -0.48], shade(gun, 0.55));
            part(
                [-0.03, 0.02, -0.56],
                [0.03, 0.05, -0.5],
                palette::PILOT_LIGHT,
            );
        }
        Kind::Mortar => {
            part([-0.2, 0.0, -0.2], [0.2, 0.06, 0.2], steel);
            // Bipod legs.
            for x in [-0.12, 0.12] {
                part(
                    [x - 0.02, 0.0, -0.22],
                    [x + 0.02, 0.36, -0.18],
                    shade(steel, 0.8),
                );
            }
            // The tube, stepped along its slant, with a hazard ring near the muzzle.
            let (start, direction, length) = MORTAR_TUBE;
            let steps = 18;
            for i in 0..=steps {
                let t = i as f32 / steps as f32;
                let at = start + direction * length * t;
                let color = if (0.78..0.88).contains(&t) {
                    palette::HAZARD
                } else {
                    shade(gun, 0.8 + 0.2 * t)
                };
                let half = Vec3::new(0.09, 0.06, 0.06);
                part((at - half).to_array(), (at + half).to_array(), color);
            }
        }
        Kind::ArtilleryCommand => {
            // A turntable and a cradle holding a rack of four launch tubes.
            part([-0.24, 0.0, -0.24], [0.24, 0.06, 0.24], steel);
            part([-0.16, 0.06, -0.02], [0.16, 0.2, 0.24], shade(gun, 1.2));
            let (start, direction, length) = ROCKET_RACK;
            // Across the rack: sideways, and square to the tubes in the upright plane.
            let across = Vec3::new(0.0, -direction.z, direction.y);
            let steps = 12;
            for (side, row) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let offset = Vec3::X * side * 0.075 + across * row * 0.07;
                for i in 0..=steps {
                    let t = i as f32 / steps as f32;
                    let at = start + offset + direction * length * t;
                    let color = if (0.7..0.8).contains(&t) {
                        palette::HAZARD
                    } else if i == steps {
                        shade(gun, 0.4)
                    } else {
                        shade(gun, 0.9 + 0.2 * t)
                    };
                    let half = Vec3::splat(0.05);
                    part((at - half).to_array(), (at + half).to_array(), color);
                }
            }
        }
    }
    mesh.build()
}

/// A shell in flight: a small dark slug with a bright nose.
pub fn shell_mesh() -> Mesh {
    let mut mesh = BoxMesh::default();
    mesh.cuboid(
        Vec3::new(-0.06, -0.06, -0.1),
        Vec3::new(0.06, 0.06, 0.1),
        palette::GUNMETAL,
    );
    mesh.cuboid(
        Vec3::new(-0.04, -0.04, -0.14),
        Vec3::new(0.04, 0.04, -0.1),
        palette::HAZARD,
    );
    mesh.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mortar_tube_points_up_and_forward() {
        let (_, direction, _) = MORTAR_TUBE;
        assert!((direction.length() - 1.0).abs() < 1e-3);
        let muzzle = layout(Kind::Mortar).muzzle;
        assert!(muzzle.y > 0.5 && muzzle.z < 0.0);
        for kind in Kind::ALL {
            let layout = layout(kind);
            assert!(
                layout.pivot + layout.muzzle.y <= layout.height + 0.01,
                "{kind:?}"
            );
        }
    }
}
