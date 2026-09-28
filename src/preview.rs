//! The live path and breach preview: a line of arrows crawling from each nest to the Command
//! Center along the way each enemy class will go, swarmers in magenta and brutes in bigger, amber
//! arrows. It follows the flow fields, so it changes the moment a wall is placed, removed, undone
//! or redone, and flares up to show it did. Where a way goes through a wall (the horde would
//! rather break it than walk around), the arrows turn red and climb over it, and a pulsing red
//! marker sits on each wall that class will break, with a pip in the class's color.
//!
//! It can also show a map that isn't real yet ([`WhatIf`]): while the cursor is on an offered
//! chunk, the routes and breaches as they'd be with that chunk claimed. Whatever the claim would
//! change stands out, and the rest fades.

use crate::flow::{Class, FlowFields, FlowSystems};
use crate::grid::{Grid, cell_center};
use crate::map::Landmarks;
use crate::palette;
use crate::pointer::column_top;
use crate::shapes::{BoxMesh, bracket_mesh};
use crate::tuning::juice::{
    BREACH_PULSE_RATE, PREVIEW_BRUTE_SPACING, PREVIEW_CRAWL, PREVIEW_FLARE_SECONDS, PREVIEW_SPACING,
};
use bevy::{
    asset::RenderAssetUsages,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

/// Height of the arrows above whatever they cross, and of the breach markers.
const LIFT: f32 = 0.06;
const MARKER_LIFT: f32 = 0.03;
/// Arrows shrink in over this distance from the ends of a route.
const FADE_LENGTH: f32 = 0.8;
/// How much bigger the arrows get when the route changes.
const FLARE: f32 = 0.7;
/// Rounding passes over the route's corners.
const SMOOTHING: usize = 2;
const ALPHA: f32 = 0.85;
/// What a what-if wouldn't change is drawn this faint.
const FADED_ALPHA: f32 = 0.22;
/// Arrows going through a wall are this much bigger, so the breach point stands out.
const BREACH_SCALE: f32 = 1.5;
/// Brutes' arrows are this much bigger than swarmers'.
const BRUTE_ARROW_SCALE: f32 = 1.8;
/// Breach markers: corner brackets a little bigger than the hover highlight's, how much they
/// swell when they pulse, and how bright they glow.
const MARKER: (f32, f32, f32, f32) = (0.56, 0.22, 0.08, 0.05);
const MARKER_PULSE: f32 = 0.08;
const MARKER_BRIGHTNESS: f32 = 1.8;
/// The class pips on a marker: half their size, and where they sit on the wall's top.
const PIP_SIZE: [f32; 2] = [0.07, 0.11];
const PIP_AT: [Vec2; 2] = [Vec2::new(-0.2, 0.3), Vec2::new(0.16, 0.3)];

pub struct PreviewPlugin;

impl Plugin for PreviewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PreviewAssets>()
            .init_resource::<Routes>()
            .init_resource::<WhatIf>()
            .add_systems(
                Update,
                (
                    trace_routes
                        .run_if(resource_changed::<FlowFields>.or_else(resource_changed::<WhatIf>)),
                    (show_arrows, show_breaches),
                )
                    .chain()
                    .in_set(PreviewSystems)
                    .after(FlowSystems),
            );
    }
}

/// Draws the path preview. Whatever sets [`WhatIf`] runs before.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PreviewSystems;

/// A map the preview shows instead of the real one, while it's set: how the map would be after
/// an edit the player is weighing up (see [`crate::chunks`]).
#[derive(Resource, Default)]
pub struct WhatIf(pub Option<Box<Scenario>>);

impl WhatIf {
    /// The map the preview shows: the one in the scenario, or the real one.
    pub fn grid<'a>(&'a self, real: &'a Grid) -> &'a Grid {
        self.0.as_ref().map_or(real, |scenario| &scenario.grid)
    }
}

/// The map as it would be, and its flow fields.
pub struct Scenario {
    pub grid: Grid,
    pub fields: FlowFields,
    /// A chunk drawn lifted (an offered chunk under the cursor), and how high.
    pub raised: Option<(IVec2, f32)>,
}

impl Scenario {
    pub fn new(grid: Grid, landmarks: &Landmarks, raised: Option<(IVec2, f32)>) -> Self {
        let fields = FlowFields::new(&grid, landmarks);
        Self {
            grid,
            fields,
            raised,
        }
    }

    /// How high the ground of `cell` is lifted.
    fn lift(&self, cell: IVec2) -> f32 {
        match self.raised {
            Some((chunk, lift)) if Grid::chunk_of(cell) == chunk => lift,
            _ => 0.0,
        }
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

/// The way one class goes from one nest.
#[derive(Clone, Debug, PartialEq)]
struct Route {
    class: Class,
    line: Polyline,
    /// Shown faded: a [`WhatIf`] wouldn't change it.
    faded: bool,
}

/// A wall some class will break through, and which classes.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Breach {
    cell: IVec2,
    /// Per class, in [`CLASSES`] order.
    by: [bool; 2],
    /// Shown faded: a [`WhatIf`] wouldn't change it.
    faded: bool,
}

const CLASSES: [Class; 2] = [Class::Swarmer, Class::Brute];

/// Each class's route from each nest, the walls they break, and when they last changed (real
/// seconds).
#[derive(Resource, Default)]
struct Routes {
    routes: Vec<Route>,
    breaches: Vec<Breach>,
    changed_at: Option<f32>,
}

fn trace_routes(
    time: Res<Time<Real>>,
    fields: Res<FlowFields>,
    grid: Res<Grid>,
    what_if: Res<WhatIf>,
    mut routes: ResMut<Routes>,
) {
    let (mut lines, mut breaches) = trace(&grid, &fields);
    if let Some(scenario) = &what_if.0 {
        let (real_lines, real_breaches) = (lines, breaches);
        (lines, breaches) = trace(&scenario.grid, &scenario.fields);
        fade_the_same(&mut lines, &real_lines, &mut breaches, &real_breaches);
    }
    if lines != routes.routes || breaches != routes.breaches {
        // The very first routes aren't a change worth flaring for.
        let first = routes.routes.is_empty();
        routes.routes = lines;
        routes.breaches = breaches;
        routes.changed_at = (!first).then(|| time.elapsed_secs());
    }
}

/// Each class's route from each live nest, and the walls they break.
fn trace(grid: &Grid, fields: &FlowFields) -> (Vec<Route>, Vec<Breach>) {
    let mut lines = Vec::new();
    let mut breaches: Vec<Breach> = Vec::new();
    for (index, &class) in CLASSES.iter().enumerate() {
        let field = fields.get(class);
        for nest in grid.live_nests() {
            let start = nest.center().xz();
            let from = field.node_at(start);
            let mut points = vec![start];
            points.extend(
                field
                    .route(from)
                    .iter()
                    .skip(1)
                    .map(|&node| field.node_center(node)),
            );
            lines.push(Route {
                class,
                line: Polyline::new(smooth(&points, SMOOTHING)),
                faded: false,
            });
            for cell in field.breaches(from, grid) {
                match breaches.iter_mut().find(|breach| breach.cell == cell) {
                    Some(breach) => breach.by[index] = true,
                    None => {
                        let mut by = [false; 2];
                        by[index] = true;
                        breaches.push(Breach {
                            cell,
                            by,
                            faded: false,
                        });
                    }
                }
            }
        }
    }
    (lines, breaches)
}

/// Fades the routes and breaches of a what-if that are just as they are on the real map, so what
/// would change stands out.
fn fade_the_same(
    lines: &mut [Route],
    real_lines: &[Route],
    breaches: &mut [Breach],
    real_breaches: &[Breach],
) {
    for route in lines {
        route.faded = real_lines.contains(route);
    }
    for breach in breaches {
        breach.faded = real_breaches.contains(breach);
    }
}

/// How big the arrows are right after the routes change (1 when they settle).
fn flare(routes: &Routes, now: f32) -> f32 {
    let flare = routes.changed_at.map_or(0.0, |at| {
        (1.0 - (now - at) / PREVIEW_FLARE_SECONDS).max(0.0)
    });
    1.0 + FLARE * flare * flare
}

#[derive(Resource)]
struct PreviewAssets {
    arrow: Handle<Mesh>,
    /// Arrow materials per class, in [`CLASSES`] order, and through a wall; then the same faded.
    arrows: [Handle<StandardMaterial>; 2],
    breach: Handle<StandardMaterial>,
    arrows_faded: [Handle<StandardMaterial>; 2],
    breach_faded: Handle<StandardMaterial>,
    marker: Handle<Mesh>,
    marker_material: Handle<StandardMaterial>,
    marker_faded: Handle<StandardMaterial>,
    pip: Handle<Mesh>,
    pips: [Handle<StandardMaterial>; 2],
}

impl FromWorld for PreviewAssets {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        let arrow = meshes.add(arrow_mesh());
        let (outer, arm, thickness, height) = MARKER;
        let marker = meshes.add(bracket_mesh(outer, arm, thickness, height));
        let mut pip = BoxMesh::default();
        pip.cuboid(
            Vec3::new(-1.0, 0.0, -1.0),
            Vec3::new(1.0, 0.04, 1.0),
            Color::WHITE,
        );
        let pip = meshes.add(pip.build());
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let mut see_through = |color: Color, alpha: f32| {
            materials.add(StandardMaterial {
                base_color: color.with_alpha(alpha),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })
        };
        let [arrows, arrows_faded] = [ALPHA, FADED_ALPHA].map(|alpha| {
            [
                see_through(palette::PATH_PREVIEW, alpha),
                see_through(palette::PATH_BRUTE, alpha),
            ]
        });
        let breach = see_through(palette::PATH_BREACH, ALPHA);
        let breach_faded = see_through(palette::PATH_BREACH, FADED_ALPHA);
        let marker_faded = see_through(palette::PATH_BREACH, FADED_ALPHA);
        let mut glowing = |color: Color, brightness: f32| {
            materials.add(StandardMaterial {
                base_color: (color.to_linear() * brightness).with_alpha(1.0).into(),
                unlit: true,
                ..default()
            })
        };
        let marker_material = glowing(palette::PATH_BREACH, MARKER_BRIGHTNESS);
        let pips = [
            glowing(palette::PATH_PREVIEW, 1.2),
            glowing(palette::PATH_BRUTE, 1.2),
        ];
        Self {
            arrow,
            arrows,
            breach,
            arrows_faded,
            breach_faded,
            marker,
            marker_material,
            marker_faded,
            pip,
            pips,
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
    fields: Res<FlowFields>,
    grid: Res<Grid>,
    what_if: Res<WhatIf>,
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
    let flare = flare(&routes, now);
    let base = landmarks.command_center.area();
    let scenario = what_if.0.as_deref();
    let (grid, fields) = scenario.map_or((&*grid, &*fields), |scenario| {
        (&scenario.grid, &scenario.fields)
    });

    let mut wanted = Vec::new();
    for &Route {
        class,
        ref line,
        faded,
    } in &routes.routes
    {
        let (spacing, size, index) = match class {
            Class::Swarmer => (PREVIEW_SPACING, 1.0, 0),
            Class::Brute => (PREVIEW_BRUTE_SPACING, BRUTE_ARROW_SCALE, 1),
        };
        let field = fields.get(class);
        let length = line.length();
        let mut along = (now * PREVIEW_CRAWL).rem_euclid(spacing);
        while along < length {
            if let Some((at, heading)) = line.sample(along) {
                // Hidden under the Command Center, and shrinking in at both ends.
                let fade = (along / FADE_LENGTH)
                    .min((length - along) / FADE_LENGTH)
                    .clamp(0.0, 1.0);
                if !base.contains(at) {
                    let breach = !field.is_clear(field.node_at(at));
                    let cell = at.floor().as_ivec2();
                    let lift = scenario.map_or(0.0, |scenario| scenario.lift(cell));
                    let height = column_top(grid, cell) + lift + LIFT;
                    // What a what-if wouldn't change doesn't flare either.
                    let mut scale = fade * size * if faded { 1.0 } else { flare };
                    if breach {
                        scale *= BREACH_SCALE;
                    }
                    let transform = Transform::from_xyz(at.x, height, at.y)
                        .with_rotation(Quat::from_rotation_y((-heading.x).atan2(-heading.y)))
                        .with_scale(Vec3::splat(scale));
                    let look = match (breach, faded) {
                        (true, false) => &assets.breach,
                        (false, false) => &assets.arrows[index],
                        (true, true) => &assets.breach_faded,
                        (false, true) => &assets.arrows_faded[index],
                    };
                    wanted.push((transform, look));
                }
            }
            along += spacing;
        }
    }

    while pool.0.len() < wanted.len() {
        let arrow = commands
            .spawn((
                Arrow,
                Mesh3d(assets.arrow.clone()),
                MeshMaterial3d(assets.arrows[0].clone()),
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
        let Some(&(placed, look)) = wanted.get(i) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        *transform = placed;
        if material.0 != *look {
            material.0 = look.clone();
        }
    }
}

/// A breach marker: corner brackets on a wall, with a pip per class as children.
#[derive(Component)]
struct Marker {
    pips: [Entity; 2],
}

/// Marker entities, reused from frame to frame.
#[derive(Default)]
struct MarkerPool(Vec<Entity>);

fn show_breaches(
    mut commands: Commands,
    time: Res<Time<Real>>,
    routes: Res<Routes>,
    grid: Res<Grid>,
    what_if: Res<WhatIf>,
    assets: Res<PreviewAssets>,
    mut pool: Local<MarkerPool>,
    mut markers: Query<(
        &Marker,
        &mut Transform,
        &mut Visibility,
        &mut MeshMaterial3d<StandardMaterial>,
    )>,
    mut pips: Query<&mut Visibility, Without<Marker>>,
) {
    let now = time.elapsed_secs();
    let pulse = 1.0 + MARKER_PULSE * (now * BREACH_PULSE_RATE * std::f32::consts::TAU).sin();
    let flare = flare(&routes, now);
    let grid = what_if.grid(&grid);

    while pool.0.len() < routes.breaches.len() {
        let pip_entities = [0, 1].map(|class| {
            let size = PIP_SIZE[class];
            commands
                .spawn((
                    Mesh3d(assets.pip.clone()),
                    MeshMaterial3d(assets.pips[class].clone()),
                    Transform::from_xyz(PIP_AT[class].x, 0.0, PIP_AT[class].y)
                        .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_4))
                        .with_scale(Vec3::splat(size)),
                    NotShadowCaster,
                ))
                .id()
        });
        let marker = commands
            .spawn((
                Marker { pips: pip_entities },
                Mesh3d(assets.marker.clone()),
                MeshMaterial3d(assets.marker_material.clone()),
                Transform::default(),
                Visibility::Hidden,
                NotShadowCaster,
            ))
            .add_children(&pip_entities)
            .id();
        pool.0.push(marker);
    }
    for (i, &entity) in pool.0.iter().enumerate() {
        // Markers spawned this frame show up next frame.
        let Ok((marker, mut transform, mut visibility, mut material)) = markers.get_mut(entity)
        else {
            continue;
        };
        let Some(breach) = routes.breaches.get(i) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        let top = column_top(grid, breach.cell) + MARKER_LIFT;
        // One a what-if wouldn't change fades, and stops pulsing.
        let scale = if breach.faded { 1.0 } else { pulse * flare };
        *transform = Transform::from_translation(cell_center(breach.cell) + Vec3::Y * top)
            .with_scale(Vec3::new(scale, 1.0, scale));
        let look = if breach.faded {
            &assets.marker_faded
        } else {
            &assets.marker_material
        };
        if material.0 != *look {
            material.0 = look.clone();
        }
        for (class, &pip) in marker.pips.iter().enumerate() {
            if let Ok(mut visibility) = pips.get_mut(pip) {
                visibility.set_if_neq(if breach.by[class] && !breach.faded {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
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

    #[test]
    fn a_what_if_fades_what_it_wouldnt_change() {
        use crate::chunks::offer::Offer;
        use crate::map::starting_map;
        use crate::rng::Rng;
        let (grid, landmarks) = starting_map();
        let (real_lines, real_breaches) = trace(&grid, &FlowFields::new(&grid, &landmarks));
        assert_eq!(real_lines.len(), 2, "each class from the starting nest");
        // The same map again: nothing changes, so everything fades.
        let scenario = Scenario::new(grid.clone(), &landmarks, None);
        let (mut lines, mut breaches) = trace(&scenario.grid, &scenario.fields);
        fade_the_same(&mut lines, &real_lines, &mut breaches, &real_breaches);
        assert!(lines.iter().all(|route| route.faded));
        // With a nest claimed, its routes stand out.
        let offer = Offer::new(&grid, &landmarks, 1, 0, &mut Rng::new(1));
        let choice = *offer
            .choices
            .iter()
            .find(|choice| choice.nest().is_some())
            .unwrap();
        let mut claimed = grid.clone();
        choice.add_to(&mut claimed);
        let scenario = Scenario::new(claimed, &landmarks, Some((choice.slot, 0.25)));
        let (mut lines, mut breaches) = trace(&scenario.grid, &scenario.fields);
        fade_the_same(&mut lines, &real_lines, &mut breaches, &real_breaches);
        assert_eq!(lines.len(), 4);
        let start = choice.nest().unwrap().center().xz();
        for route in &lines {
            let from_new_nest = route.line.sample(0.0).unwrap().0 == start;
            assert!(!from_new_nest || !route.faded, "{route:?}");
        }
        // Its ground is drawn lifted.
        let inside = Grid::chunk_origin(choice.slot) + IVec2::new(3, 3);
        assert_eq!(scenario.lift(inside), 0.25);
        assert_eq!(scenario.lift(IVec2::ZERO), 0.0);
    }
}
