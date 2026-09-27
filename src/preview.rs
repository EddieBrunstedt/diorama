//! The live path preview: a line of arrows crawling from each nest to the Command Center along
//! the way the horde will go. It follows the flow field, so it changes the moment a wall is
//! placed, removed, undone or redone, and flares up to show it did. Where the way goes through a
//! wall (the horde would rather break it than walk around), the arrows turn red and climb over it.

use crate::flow::{FlowField, FlowSystems};
use crate::grid::{Grid, cell_at};
use crate::map::Landmarks;
use crate::palette;
use crate::pointer::column_top;
use crate::tuning::juice::{PREVIEW_CRAWL, PREVIEW_FLARE_SECONDS, PREVIEW_SPACING};
use bevy::{
    asset::RenderAssetUsages,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

/// Height of the arrows above whatever they cross.
const LIFT: f32 = 0.06;
/// Arrows shrink in over this distance from the ends of a route.
const FADE_LENGTH: f32 = 0.8;
/// How much bigger the arrows get when the route changes.
const FLARE: f32 = 0.7;
/// Rounding passes over the route's corners.
const SMOOTHING: usize = 2;
const ALPHA: f32 = 0.85;
/// Arrows going through a wall are this much bigger, so the breach point stands out.
const BREACH_SCALE: f32 = 1.5;

pub struct PreviewPlugin;

impl Plugin for PreviewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PreviewAssets>()
            .init_resource::<Routes>()
            .add_systems(
                Update,
                (
                    trace_routes.run_if(resource_changed::<FlowField>),
                    show_arrows,
                )
                    .chain()
                    .after(FlowSystems),
            );
    }
}

/// A path along the ground, measured along its length.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Polyline {
    points: Vec<Vec2>,
    /// How far along the line each point is.
    distances: Vec<f32>,
}

impl Polyline {
    pub fn new(points: Vec<Vec2>) -> Self {
        let mut distances = Vec::with_capacity(points.len());
        let mut total = 0.0;
        for (i, point) in points.iter().enumerate() {
            if i > 0 {
                total += point.distance(points[i - 1]);
            }
            distances.push(total);
        }
        Self { points, distances }
    }

    pub fn length(&self) -> f32 {
        self.distances.last().copied().unwrap_or(0.0)
    }

    /// The point `distance` along the line (clamped to its ends), and the way the line heads
    /// there.
    pub fn sample(&self, distance: f32) -> Option<(Vec2, Vec2)> {
        if self.points.len() < 2 {
            return None;
        }
        let distance = distance.clamp(0.0, self.length());
        // The segment ending at the first point past `distance`.
        let end = self
            .distances
            .partition_point(|&d| d <= distance)
            .clamp(1, self.points.len() - 1);
        let (a, b) = (self.points[end - 1], self.points[end]);
        let span = self.distances[end] - self.distances[end - 1];
        let t = if span > 0.0 {
            (distance - self.distances[end - 1]) / span
        } else {
            0.0
        };
        Some((a.lerp(b, t), (b - a).normalize_or_zero()))
    }
}

/// Rounds a path's corners by cutting each one off (Chaikin's method), keeping both ends.
pub fn smooth(points: &[Vec2], passes: usize) -> Vec<Vec2> {
    let mut points = points.to_vec();
    for _ in 0..passes {
        if points.len() < 3 {
            break;
        }
        let mut rounder = vec![points[0]];
        for pair in points.windows(2) {
            rounder.push(pair[0].lerp(pair[1], 0.25));
            rounder.push(pair[0].lerp(pair[1], 0.75));
        }
        rounder.push(*points.last().unwrap());
        // The cuts next to the ends only shorten the first and last segments.
        rounder.remove(1);
        rounder.remove(rounder.len() - 2);
        points = rounder;
    }
    points
}

/// The route from each nest, and when they last changed (real seconds).
#[derive(Resource, Default)]
struct Routes {
    lines: Vec<Polyline>,
    changed_at: Option<f32>,
}

fn trace_routes(
    time: Res<Time<Real>>,
    field: Res<FlowField>,
    landmarks: Res<Landmarks>,
    mut routes: ResMut<Routes>,
) {
    let lines: Vec<Polyline> = landmarks
        .nests
        .iter()
        .map(|nest| {
            let start = nest.center().xz();
            let cells = field.route(cell_at(nest.center()));
            let mut points = vec![start];
            points.extend(cells.iter().skip(1).map(|cell| cell.as_vec2() + 0.5));
            Polyline::new(smooth(&points, SMOOTHING))
        })
        .collect();
    if lines != routes.lines {
        // The very first routes aren't a change worth flaring for.
        let first = routes.lines.is_empty();
        routes.lines = lines;
        routes.changed_at = (!first).then(|| time.elapsed_secs());
    }
}

#[derive(Resource)]
struct PreviewAssets {
    arrow: Handle<Mesh>,
    normal: Handle<StandardMaterial>,
    breach: Handle<StandardMaterial>,
}

impl FromWorld for PreviewAssets {
    fn from_world(world: &mut World) -> Self {
        let arrow = world.resource_mut::<Assets<Mesh>>().add(arrow_mesh());
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let mut material = |color: Color| {
            materials.add(StandardMaterial {
                base_color: color.with_alpha(ALPHA),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })
        };
        Self {
            arrow,
            normal: material(palette::PATH_PREVIEW),
            breach: material(palette::PATH_BREACH),
        }
    }
}

/// A flat chevron lying on the ground, pointing down -Z.
fn arrow_mesh() -> Mesh {
    let (reach, width, thickness) = (0.2, 0.2, 0.11);
    let tip = Vec2::new(0.0, -reach);
    // Each arm as a quad, wound the same way as `BoxMesh::quad` so it faces up.
    let left = [
        tip,
        Vec2::new(-width, 0.0),
        Vec2::new(-width, thickness),
        tip + Vec2::new(0.0, thickness),
    ];
    let right = [
        tip,
        tip + Vec2::new(0.0, thickness),
        Vec2::new(width, thickness),
        Vec2::new(width, 0.0),
    ];
    let positions: Vec<[f32; 3]> = left
        .iter()
        .chain(&right)
        .map(|p| [p.x, 0.0, p.y + reach / 2.0])
        .collect();
    let normals = vec![[0.0, 1.0, 0.0]; positions.len()];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]))
}

#[derive(Component)]
struct Arrow;

/// Arrow entities, reused from frame to frame.
#[derive(Default)]
struct ArrowPool(Vec<Entity>);

fn show_arrows(
    mut commands: Commands,
    time: Res<Time<Real>>,
    routes: Res<Routes>,
    field: Res<FlowField>,
    grid: Res<Grid>,
    landmarks: Res<Landmarks>,
    assets: Res<PreviewAssets>,
    mut pool: Local<ArrowPool>,
    mut arrows: Query<
        (
            &mut Transform,
            &mut Visibility,
            &mut MeshMaterial3d<StandardMaterial>,
        ),
        With<Arrow>,
    >,
) {
    let now = time.elapsed_secs();
    let crawl = (now * PREVIEW_CRAWL).rem_euclid(PREVIEW_SPACING);
    let flare = routes.changed_at.map_or(0.0, |at| {
        (1.0 - (now - at) / PREVIEW_FLARE_SECONDS).max(0.0)
    });
    let base = landmarks.command_center.area();

    let mut wanted = Vec::new();
    for line in &routes.lines {
        let length = line.length();
        let mut along = crawl;
        while along < length {
            if let Some((at, heading)) = line.sample(along) {
                let cell = at.floor().as_ivec2();
                // Hidden under the Command Center, and shrinking in at both ends.
                let fade = (along / FADE_LENGTH)
                    .min((length - along) / FADE_LENGTH)
                    .clamp(0.0, 1.0);
                if !base.contains(at) {
                    let breach = !field.is_clear(cell);
                    let height = column_top(&grid, cell) + LIFT;
                    let mut scale = fade * (1.0 + FLARE * flare * flare);
                    if breach {
                        scale *= BREACH_SCALE;
                    }
                    let transform = Transform::from_xyz(at.x, height, at.y)
                        .with_rotation(Quat::from_rotation_y((-heading.x).atan2(-heading.y)))
                        .with_scale(Vec3::splat(scale));
                    wanted.push((transform, breach));
                }
            }
            along += PREVIEW_SPACING;
        }
    }

    while pool.0.len() < wanted.len() {
        let arrow = commands
            .spawn((
                Arrow,
                Mesh3d(assets.arrow.clone()),
                MeshMaterial3d(assets.normal.clone()),
                Transform::default(),
                Visibility::Hidden,
                NotShadowCaster,
            ))
            .id();
        pool.0.push(arrow);
    }
    for (i, &entity) in pool.0.iter().enumerate() {
        // Arrows spawned this frame show up next frame.
        let Ok((mut transform, mut visibility, mut material)) = arrows.get_mut(entity) else {
            continue;
        };
        let Some(&(placed, breach)) = wanted.get(i) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        *transform = placed;
        let look = if breach {
            &assets.breach
        } else {
            &assets.normal
        };
        if material.0 != *look {
            material.0 = look.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polylines_sample_along_their_length() {
        let line = Polyline::new(vec![Vec2::ZERO, Vec2::new(2.0, 0.0), Vec2::new(2.0, 3.0)]);
        assert_eq!(line.length(), 5.0);
        assert_eq!(line.sample(1.0), Some((Vec2::new(1.0, 0.0), Vec2::X)));
        assert_eq!(line.sample(3.5), Some((Vec2::new(2.0, 1.5), Vec2::Y)));
        assert_eq!(line.sample(9.0), Some((Vec2::new(2.0, 3.0), Vec2::Y)));
        assert_eq!(line.sample(-1.0), Some((Vec2::ZERO, Vec2::X)));
        assert_eq!(Polyline::new(vec![Vec2::ONE]).sample(0.0), None);
    }

    #[test]
    fn smoothing_rounds_corners_and_keeps_the_ends() {
        let corner = [Vec2::ZERO, Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0)];
        let smooth = smooth(&corner, 2);
        assert_eq!(smooth.first(), Some(&Vec2::ZERO));
        assert_eq!(smooth.last(), Some(&Vec2::new(4.0, 4.0)));
        assert!(!smooth.contains(&Vec2::new(4.0, 0.0)), "the corner is cut");
        // It's shorter than going all the way into the corner, but not by much.
        let length = Polyline::new(smooth).length();
        assert!(length < 8.0 && length > 7.0, "{length}");
        // Straight lines stay straight.
        let straight = [Vec2::ZERO, Vec2::X, Vec2::new(2.0, 0.0)];
        assert!(smooth_is_straight(&super::smooth(&straight, 2)));
    }

    fn smooth_is_straight(points: &[Vec2]) -> bool {
        points.iter().all(|point| point.y == 0.0)
    }
}
