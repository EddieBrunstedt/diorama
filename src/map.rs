//! The map: the hand-authored starting [`Grid`] (with its nests), where the Command Center is
//! ([`Landmarks`]), and the ground it's drawn as, one mesh per chunk. The map grows as chunks are
//! claimed ([`crate::chunks`]): each [`ChunkChanged`] redraws the ground around it, and a claimed
//! chunk drops onto the map with a slam. A new run takes the map back to the starting one.

use crate::build::{BuildSystems, clear_the_map};
use crate::grid::{Chunk, Footprint, Grid, Structure, Terrain};
use crate::juice::{Burst, Particles, Shake, Tween, TweenDone, TweenTarget};
use crate::palette;
use crate::rng::Rng;
use crate::shapes::{BoxMesh, ShapeMaterial, shade, vary};
use crate::tuning::grid::{CHUNK_SIZE, COMMAND_CENTER_SIZE, NEST_SIZE, STARTING_CHUNKS};
use crate::tuning::juice::{
    CHUNK_DROP_HEIGHT, CHUNK_DROP_SECONDS, CHUNK_DUST_PER_SIDE, CHUNK_LAND_SHAKE, CHUNK_SINK_DEPTH,
    CHUNK_SINK_SECONDS, DUST_LIFETIME, DUST_LIFT, DUST_SIZE, DUST_SPREAD,
};
use crate::waves::Phase;
use bevy::prelude::*;

/// Gap between neighboring cells' tiles, and the width of the seam along chunk borders.
const CELL_GAP: f32 = 0.06;
const CHUNK_SEAM: f32 = 0.12;
/// Thickness of the slab the map sits on, so its edge reads as an edge.
const SLAB_DEPTH: f32 = 0.5;
/// Rock stands between these heights: low on its own, tall with rock all around, so ridges rise
/// toward their spine. Each block's height varies by up to `ROCK_JITTER` either way.
const ROCK_HEIGHT: (f32, f32) = (0.45, 1.05);
const ROCK_JITTER: f32 = 0.08;
/// A nest's ground is a low crust of creep.
pub const NEST_HEIGHT: f32 = 0.05;
/// Mud lies flush with the ground, with a low lip of drier mud where it meets the ground and
/// glossy puddles in it (up to this many a cell, this big).
const MUD_Y: f32 = 0.003;
const MUD_LIP_WIDTH: f32 = 0.1;
const MUD_LIP_HEIGHT: f32 = 0.025;
const MUD_PUDDLES: usize = 2;
const MUD_PUDDLE_SIZE: (f32, f32) = (0.18, 0.42);
/// How shiny wet mud is, compared to the rest of the ground (see [`ShapeMaterial`]).
const MUD_ROUGHNESS: f32 = 0.35;
const MUD_REFLECTANCE: f32 = 0.45;
/// Salts for the per-cell random numbers.
const SALT_TILE: u64 = 1;
const SALT_ROCK: u64 = 2;
const SALT_MUD: u64 = 3;

/// The starting map, row by row from the far edge (the top of the screen) down, one character
/// per cell: `.` ground, `#` rock, `~` mud, `N` nest, `C` the Command Center (on ground). It
/// covers `STARTING_CHUNKS`×`STARTING_CHUNKS` chunks around the origin.
///
/// The nest is up and left of the Command Center. A ridge crosses the map between them, with
/// three gates:
/// - right on the straight line, one cell wide between rocks: swarmers slip through, but brutes
///   never fit and can't widen it;
/// - to the west, three cells of mud: slow, and it can't be walled shut, only sealed off behind
///   it at the cost of a long wall;
/// - to the east, two cells of dry ground: the long way round, but wide enough for brutes.
///
/// Every detour from one gate to the next is shorter than breaking a wall, so walling off one
/// gate sends the horde to the next, and only sealing all of them makes it break through.
const STARTING_MAP: [&str; 32] = [
    "..........NN....................",
    "..........NN....................",
    "................................",
    "...##..............##...........",
    "...##..............###...~~~....",
    "...##...................~~~.....",
    "........................~~~.....",
    "................................",
    "....~~~..####...................",
    "####~~~########...####......###.",
    "####~~~########.#########..#####",
    "...~~~~.###......########..#####",
    "...~~~~~...........####....##...",
    ".....~~~........................",
    "................................",
    "...............CC...............",
    "...............CC...............",
    "..........##....................",
    "..........###...................",
    "..........##.......~~~~.........",
    "...................~~~~~........",
    "...................~~~~.........",
    "....##..............~~..........",
    "....##..........................",
    "....###.........................",
    "....##..................###.....",
    "........................###.....",
    ".......~~~~.....##.......#......",
    ".......~~~~.....##..............",
    "........~~......................",
    "................................",
    "................................",
];

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        let (grid, landmarks) = starting_map();
        app.insert_resource(grid)
            .insert_resource(landmarks)
            .init_resource::<ShapeMaterial>()
            .init_resource::<MudMaterial>()
            .add_message::<ChunkChanged>()
            .add_systems(Startup, spawn_ground)
            .add_systems(
                OnExit(Phase::GameOver),
                back_to_the_starting_map.after(clear_the_map),
            )
            .add_systems(Update, follow_chunks.after(BuildSystems))
            .add_observer(land_chunk);
    }
}

/// Where the Command Center is. (The nests are in the [`Grid`], since claiming chunks adds them.)
#[derive(Resource, Clone, Debug)]
pub struct Landmarks {
    pub command_center: Footprint,
}

/// A chunk was added to the map (`claimed`) or taken off it.
#[derive(Message, Clone, Copy, Debug)]
pub struct ChunkChanged {
    pub at: IVec2,
    pub claimed: bool,
}

/// The hand-authored starting map ([`STARTING_MAP`]) and its landmarks.
pub fn starting_map() -> (Grid, Landmarks) {
    read_map(&STARTING_MAP, -STARTING_CHUNKS / 2 * CHUNK_SIZE)
}

/// The terrain a map drawing's character stands for: `.` ground, `#` rock, `~` mud, `N` nest.
/// (`C`, the Command Center, stands on ground, but only a whole map can have one.)
pub fn terrain_of(char: char) -> Option<Terrain> {
    match char {
        '.' => Some(Terrain::Ground),
        '#' => Some(Terrain::Rock),
        '~' => Some(Terrain::Mud),
        'N' => Some(Terrain::Nest),
        _ => None,
    }
}

/// Reads a map drawn as rows of characters (see [`STARTING_MAP`]), whose first character is cell
/// `(origin, origin)`. Every chunk it touches is added, with ground wherever the drawing leaves
/// it out.
///
/// Panics if the drawing is malformed: an unknown character, no Command Center, or a Command
/// Center or nest that isn't a whole square of the right size.
pub fn read_map(rows: &[&str], origin: i32) -> (Grid, Landmarks) {
    let mut grid = Grid::default();
    let mut command_center = Vec::new();
    let mut nest_cells = Vec::new();
    for (y, row) in rows.iter().enumerate() {
        for (x, char) in row.chars().enumerate() {
            let cell = IVec2::splat(origin) + IVec2::new(x as i32, y as i32);
            let chunk = Grid::chunk_of(cell);
            if grid.cell(cell).is_none() {
                grid.insert_chunk(chunk, Chunk::new(Terrain::Ground));
            }
            let terrain = match char {
                'C' => {
                    command_center.push(cell);
                    Terrain::Ground
                }
                other => terrain_of(other)
                    .unwrap_or_else(|| panic!("unknown map character {other:?} at {cell}")),
            };
            if terrain == Terrain::Nest {
                nest_cells.push(cell);
            }
            grid.set_terrain(cell, terrain);
        }
    }

    let min = command_center
        .iter()
        .copied()
        .reduce(IVec2::min)
        .expect("the map has no Command Center");
    let command_center_footprint = square(&command_center, min, COMMAND_CENTER_SIZE);
    assert_eq!(
        command_center.len() as i32,
        COMMAND_CENTER_SIZE * COMMAND_CENTER_SIZE,
        "one Command Center"
    );
    for cell in command_center_footprint.cells() {
        grid.set_structure(cell, Some(Structure::CommandCenter));
    }

    grid.nests = find_nests(&nest_cells);
    let landmarks = Landmarks {
        command_center: command_center_footprint,
    };
    (grid, landmarks)
}

/// The footprint of a square of `size` cells starting at `min`, checking that `cells` holds all of
/// it.
fn square(cells: &[IVec2], min: IVec2, size: i32) -> Footprint {
    let footprint = Footprint { min, size };
    assert!(
        footprint.cells().all(|cell| cells.contains(&cell)),
        "{footprint:?} isn't a whole {size}×{size} square"
    );
    footprint
}

/// The nests that `cells` (the nest cells of a drawing, in reading order) make up, each starting
/// at its lowest cell.
///
/// Panics if they don't make up whole, separate nests.
pub fn find_nests(cells: &[IVec2]) -> Vec<Footprint> {
    let mut nests: Vec<Footprint> = Vec::new();
    for &cell in cells {
        if nests.iter().any(|nest| nest.cells().any(|c| c == cell)) {
            continue;
        }
        nests.push(square(cells, cell, NEST_SIZE));
    }
    assert_eq!(
        nests.len() as i32 * NEST_SIZE * NEST_SIZE,
        cells.len() as i32,
        "nests overlap"
    );
    nests
}

/// How tall the terrain on a cell stands, for putting things on top of it (0 outside the map).
pub fn terrain_height(grid: &Grid, cell: IVec2) -> f32 {
    let Some(data) = grid.cell(cell) else {
        return 0.0;
    };
    match data.terrain {
        Terrain::Ground | Terrain::Mud => 0.0,
        Terrain::Nest => NEST_HEIGHT,
        Terrain::Rock => {
            // Taller the more rock is around it, so a ridge rises toward its spine.
            let rock_around = AROUND
                .iter()
                .filter(|&&step| {
                    grid.cell(cell + step)
                        .is_some_and(|data| data.terrain == Terrain::Rock)
                })
                .count();
            let (low, high) = ROCK_HEIGHT;
            let height = low + (high - low) * rock_around as f32 / AROUND.len() as f32;
            height + Rng::for_cell(cell, SALT_ROCK).range(-ROCK_JITTER, ROCK_JITTER)
        }
    }
}

/// The eight cells around a cell.
const AROUND: [IVec2; 8] = [
    IVec2::new(-1, -1),
    IVec2::new(0, -1),
    IVec2::new(1, -1),
    IVec2::new(-1, 0),
    IVec2::new(1, 0),
    IVec2::new(-1, 1),
    IVec2::new(0, 1),
    IVec2::new(1, 1),
];

/// The material wet mud is drawn with: the vertex colors of [`ShapeMaterial`], but glossy.
#[derive(Resource)]
pub struct MudMaterial(pub Handle<StandardMaterial>);

impl FromWorld for MudMaterial {
    fn from_world(world: &mut World) -> Self {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        Self(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: MUD_ROUGHNESS,
            reflectance: MUD_REFLECTANCE,
            ..default()
        }))
    }
}

/// A chunk's ground (its tiles and rock, or its mud): redrawn when the chunks around it change.
#[derive(Component)]
struct Ground {
    chunk: IVec2,
}

/// The ground of a newly claimed chunk, dropping onto the map. It slams down when it lands.
#[derive(Component)]
struct Landing;

/// What ground is drawn with.
#[derive(bevy::ecs::system::SystemParam)]
struct GroundAssets<'w> {
    material: Res<'w, ShapeMaterial>,
    mud: Res<'w, MudMaterial>,
    meshes: ResMut<'w, Assets<Mesh>>,
}

impl GroundAssets<'_> {
    /// Spawns the ground of the chunk at `at`, returning its entities (the ground, and its mud if
    /// it has any).
    fn spawn(&mut self, commands: &mut Commands, grid: &Grid, at: IVec2) -> Vec<Entity> {
        let origin = Grid::chunk_origin(at);
        let transform = Transform::from_xyz(origin.x as f32, 0.0, origin.y as f32);
        let mut entities = vec![
            commands
                .spawn((
                    Ground { chunk: at },
                    Mesh3d(self.meshes.add(chunk_mesh(grid, at))),
                    MeshMaterial3d(self.material.0.clone()),
                    transform,
                ))
                .id(),
        ];
        if let Some(mesh) = mud_mesh(grid, at) {
            entities.push(
                commands
                    .spawn((
                        Ground { chunk: at },
                        Mesh3d(self.meshes.add(mesh)),
                        MeshMaterial3d(self.mud.0.clone()),
                        transform,
                    ))
                    .id(),
            );
        }
        entities
    }
}

fn spawn_ground(mut commands: Commands, grid: Res<Grid>, mut assets: GroundAssets) {
    for (at, _) in grid.chunks() {
        assets.spawn(&mut commands, &grid, at);
    }
}

/// Redraws the ground around every chunk that was claimed or taken back (rock heights and mud
/// lips depend on the cells next door). A claimed chunk drops in, and one taken back sinks away.
fn follow_chunks(
    mut commands: Commands,
    mut changes: MessageReader<ChunkChanged>,
    grid: Res<Grid>,
    ground: Query<(Entity, &Ground, &Transform)>,
    mut assets: GroundAssets,
) {
    let changes: Vec<ChunkChanged> = changes.read().copied().collect();
    if changes.is_empty() {
        return;
    }
    let mut redraw: Vec<IVec2> = Vec::new();
    for change in &changes {
        for y in -1..=1 {
            for x in -1..=1 {
                let at = change.at + IVec2::new(x, y);
                if !redraw.contains(&at) {
                    redraw.push(at);
                }
            }
        }
    }
    let claimed = |at: IVec2| {
        changes
            .iter()
            .rev()
            .find(|change| change.at == at)
            .map(|change| change.claimed)
    };
    for (entity, view, transform) in &ground {
        if !redraw.contains(&view.chunk) {
            continue;
        }
        if claimed(view.chunk) == Some(false) {
            let from = transform.translation;
            commands.entity(entity).insert(
                Tween::new(
                    TweenTarget::Translation {
                        from,
                        to: from - Vec3::Y * CHUNK_SINK_DEPTH,
                    },
                    EaseFunction::QuadraticIn,
                    0.0,
                    CHUNK_SINK_SECONDS,
                )
                .then_despawn(),
            );
        } else {
            commands.entity(entity).despawn();
        }
    }
    for at in redraw {
        if grid.chunk(at).is_none() {
            continue;
        }
        let entities = assets.spawn(&mut commands, &grid, at);
        if claimed(at) == Some(true) {
            let origin = Grid::chunk_origin(at);
            let to = Vec3::new(origin.x as f32, 0.0, origin.y as f32);
            let drop = Tween::new(
                TweenTarget::Translation {
                    from: to + Vec3::Y * CHUNK_DROP_HEIGHT,
                    to,
                },
                EaseFunction::QuadraticIn,
                0.0,
                CHUNK_DROP_SECONDS,
            );
            for (index, entity) in entities.into_iter().enumerate() {
                let mut entity = commands.entity(entity);
                entity.insert((drop.clone(), drop.start_transform()));
                if index == 0 {
                    entity.insert(Landing);
                }
            }
        }
    }
}

/// A claimed chunk hits the map: dust all along its edges and a big shake.
fn land_chunk(
    done: On<TweenDone>,
    mut commands: Commands,
    landing: Query<&Ground, With<Landing>>,
    mut particles: Particles,
    mut shake: ResMut<Shake>,
) {
    let Ok(ground) = landing.get(done.entity) else {
        return;
    };
    commands.entity(done.entity).remove::<Landing>();
    shake.add(CHUNK_LAND_SHAKE);
    let origin = Grid::chunk_origin(ground.chunk).as_vec2();
    let size = CHUNK_SIZE as f32;
    let corners = [
        origin,
        origin + Vec2::new(size, 0.0),
        origin + Vec2::splat(size),
        origin + Vec2::new(0.0, size),
    ];
    for side in 0..4 {
        let (from, to) = (corners[side], corners[(side + 1) % 4]);
        for step in 0..CHUNK_DUST_PER_SIDE {
            let at = from.lerp(to, (step as f32 + 0.5) / CHUNK_DUST_PER_SIDE as f32);
            particles.burst(
                &mut commands,
                Vec3::new(at.x, 0.05, at.y),
                0.8,
                Burst {
                    colors: &palette::DUST,
                    count: 5,
                    size: DUST_SIZE * 1.6,
                    spread: DUST_SPREAD,
                    lift: DUST_LIFT,
                    lifetime: DUST_LIFETIME * 1.4,
                },
            );
        }
    }
}

/// A new run starts on the starting map: every claimed chunk is taken off, with its nest. (What
/// was built on them is cleared first, like everything else built.)
fn back_to_the_starting_map(mut grid: ResMut<Grid>, mut changed: MessageWriter<ChunkChanged>) {
    let (start, _) = starting_map();
    let claimed: Vec<IVec2> = grid
        .chunks()
        .map(|(at, _)| at)
        .filter(|&at| start.chunk(at).is_none())
        .collect();
    for at in claimed {
        grid.remove_chunk(at);
        changed.write(ChunkChanged { at, claimed: false });
    }
    grid.nests = start.nests;
}

/// A chunk's ground in chunk-local coordinates: a slab showing through as grid lines, a tile per
/// cell, and rock blocks. Mud is drawn separately ([`mud_mesh`]), since it's glossy.
pub fn chunk_mesh(grid: &Grid, at: IVec2) -> Mesh {
    let size = CHUNK_SIZE as f32;
    let origin = Grid::chunk_origin(at);
    let mut mesh = BoxMesh::default();
    mesh.cuboid_graded(
        Vec3::new(0.0, -SLAB_DEPTH, 0.0),
        Vec3::new(size, 0.0, size),
        palette::CHUNK_SEAM,
        shade(palette::GROUND_GAP, 0.5),
    );
    let seam = CHUNK_SEAM / 2.0;
    mesh.quad(
        Vec2::splat(seam),
        Vec2::splat(size - seam),
        0.001,
        palette::GROUND_GAP,
    );
    for y in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let local = IVec2::new(x, y);
            let cell = origin + local;
            let Some(data) = grid.cell(cell) else {
                continue;
            };
            // Cells on the chunk's border leave room for the seam.
            let gap = |edge: bool| if edge { seam } else { CELL_GAP / 2.0 };
            let min = local.as_vec2() + Vec2::new(gap(x == 0), gap(y == 0));
            let max = (local + IVec2::ONE).as_vec2()
                - Vec2::new(gap(x == CHUNK_SIZE - 1), gap(y == CHUNK_SIZE - 1));
            let mut rng = Rng::for_cell(cell, SALT_TILE);
            let (low, high) = (local.as_vec2(), (local + IVec2::ONE).as_vec2());
            match data.terrain {
                Terrain::Ground => {
                    let color = vary(palette::GROUND, &mut rng, 0.012, 4.0);
                    mesh.quad(min, max, 0.002, color);
                }
                Terrain::Mud => {}
                Terrain::Nest => {
                    // A crust of creep over the whole cell, so a nest reads as one patch.
                    let color = vary(palette::CREEP, &mut rng, 0.02, 8.0);
                    mesh.cuboid(
                        Vec3::new(low.x, 0.0, low.y),
                        Vec3::new(high.x, NEST_HEIGHT, high.y),
                        color,
                    );
                }
                Terrain::Rock => {
                    // Rock fills the whole cell, so neighboring rocks merge into one outcrop,
                    // with a lump on top so it reads as natural rather than built.
                    let color = vary(palette::ROCK, &mut rng, 0.03, 6.0);
                    let height = terrain_height(grid, cell);
                    mesh.cuboid(
                        Vec3::new(low.x, 0.0, low.y),
                        Vec3::new(high.x, height, high.y),
                        color,
                    );
                    let size = rng.range(0.35, 0.6);
                    let at =
                        low + Vec2::new(rng.range(0.1, 0.9 - size), rng.range(0.1, 0.9 - size));
                    mesh.cuboid(
                        Vec3::new(at.x, height, at.y),
                        Vec3::new(at.x + size, height + rng.range(0.1, 0.25), at.y + size),
                        shade(color, 1.15),
                    );
                }
            }
        }
    }
    mesh.build()
}

/// A chunk's mud in chunk-local coordinates, if it has any: a sheet over each mud cell (covering
/// the gaps between cells, so a patch reads as one), a lip of drier mud along the edges that
/// meet other ground, and puddles.
pub fn mud_mesh(grid: &Grid, at: IVec2) -> Option<Mesh> {
    let origin = Grid::chunk_origin(at);
    let is_mud = |cell: IVec2| {
        grid.cell(cell)
            .is_some_and(|data| data.terrain == Terrain::Mud)
    };
    let mut mesh = BoxMesh::default();
    let mut any = false;
    for y in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let local = IVec2::new(x, y);
            let cell = origin + local;
            if !is_mud(cell) {
                continue;
            }
            any = true;
            let mut rng = Rng::for_cell(cell, SALT_MUD);
            let (low, high) = (local.as_vec2(), (local + IVec2::ONE).as_vec2());
            mesh.quad(low, high, MUD_Y, vary(palette::MUD, &mut rng, 0.01, 5.0));
            // The lip, along each side that doesn't lead into more mud.
            let lip = vary(palette::MUD_LIP, &mut rng, 0.015, 5.0);
            let w = MUD_LIP_WIDTH;
            for (step, min, max) in [
                (IVec2::NEG_X, low, Vec2::new(low.x + w, high.y)),
                (IVec2::X, Vec2::new(high.x - w, low.y), high),
                (IVec2::NEG_Y, low, Vec2::new(high.x, low.y + w)),
                (IVec2::Y, Vec2::new(low.x, high.y - w), high),
            ] {
                if !is_mud(cell + step) {
                    mesh.cuboid(
                        Vec3::new(min.x, 0.0, min.y),
                        Vec3::new(max.x, MUD_LIP_HEIGHT, max.y),
                        lip,
                    );
                }
            }
            for _ in 0..rng.index(MUD_PUDDLES + 1) {
                let (small, big) = MUD_PUDDLE_SIZE;
                let size = Vec2::new(rng.range(small, big), rng.range(small, big));
                let room = Vec2::ONE - size - 2.0 * w;
                let min = low + Vec2::splat(w) + Vec2::new(rng.f32(), rng.f32()) * room;
                let color = vary(palette::MUD_PUDDLE, &mut rng, 0.01, 5.0);
                mesh.quad(min, min + size, MUD_Y + 0.001, color);
            }
        }
    }
    any.then(|| mesh.build())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::{Class, FlowField, FlowFields};

    #[test]
    fn the_starting_map_holds_its_landmarks() {
        let (grid, landmarks) = starting_map();
        let bounds = grid.bounds().unwrap();
        let cells = STARTING_CHUNKS * CHUNK_SIZE;
        assert_eq!(bounds.size() + IVec2::ONE, IVec2::splat(cells));
        assert_eq!(bounds.min, IVec2::splat(-cells / 2));
        assert!(STARTING_MAP.iter().all(|row| row.len() == cells as usize));
        let on_map = |footprint: Footprint| {
            footprint
                .cells()
                .all(|cell| bounds.contains(cell) && grid.cell(cell).is_some())
        };
        assert!(on_map(landmarks.command_center));
        assert_eq!(landmarks.command_center.center(), Vec3::ZERO);
        for cell in landmarks.command_center.cells() {
            assert_eq!(grid.structure(cell), Some(Structure::CommandCenter));
        }
        assert_eq!(grid.nests.len(), 1);
        let fields = FlowFields::new(&grid, &landmarks);
        for &nest in &grid.nests {
            assert!(on_map(nest));
            for cell in nest.cells() {
                assert_eq!(grid.cell(cell).unwrap().terrain, Terrain::Nest);
                assert!(grid.check_build(cell).is_err());
                assert!(
                    fields.swarmer.distance(cell).is_finite(),
                    "{cell} can't reach the base"
                );
            }
            // Brutes come out of the nest's middle, and can reach the base from there too.
            let brutes = &fields.brute;
            let middle = brutes.node_at(nest.center().xz());
            assert!(brutes.distance(middle).is_finite(), "brutes can't leave");
        }
    }

    #[test]
    fn reading_a_map() {
        let rows = ["NN.~", "NN#.", "..CC", "..CC"];
        let (grid, landmarks) = read_map(&rows, -2);
        assert_eq!(
            landmarks.command_center,
            Footprint {
                min: IVec2::new(0, 0),
                size: 2
            }
        );
        assert_eq!(
            grid.nests,
            vec![Footprint {
                min: IVec2::new(-2, -2),
                size: 2
            }]
        );
        let terrain = |x, y| grid.cell(IVec2::new(x, y)).unwrap().terrain;
        assert_eq!(terrain(1, -2), Terrain::Mud);
        assert_eq!(terrain(0, -1), Terrain::Rock);
        assert_eq!(terrain(1, -1), Terrain::Ground);
        assert_eq!(terrain(0, 0), Terrain::Ground);
        assert_eq!(
            grid.structure(IVec2::new(1, 1)),
            Some(Structure::CommandCenter)
        );
        // The chunks it touches are all there, ground where the drawing leaves off.
        assert_eq!(terrain(8, 8), Terrain::Ground);
        assert_eq!(terrain(-16, -16), Terrain::Ground);
    }

    #[test]
    #[should_panic(expected = "isn't a whole")]
    fn a_broken_nest_is_refused() {
        read_map(&["N...", "....", ".CC.", ".CC."], 0);
    }

    #[test]
    fn ridges_rise_toward_their_spine() {
        let (grid, _) = read_map(&["#..CC", "...CC", "..###", "..###", "..###"], 0);
        let lone = terrain_height(&grid, IVec2::new(0, 0));
        let edge = terrain_height(&grid, IVec2::new(2, 2));
        let spine = terrain_height(&grid, IVec2::new(3, 3));
        assert!(lone < edge && edge < spine, "{lone} {edge} {spine}");
        assert!(spine <= ROCK_HEIGHT.1 + ROCK_JITTER);
        assert_eq!(terrain_height(&grid, IVec2::new(1, 1)), 0.0);
        assert_eq!(terrain_height(&grid, IVec2::new(99, 0)), 0.0, "off the map");
    }

    /// The gates in the ridge on the starting map, as the cells a line of walls closes each
    /// with (the mud gate can't be walled, so it's sealed off behind, below the ridge).
    fn gates() -> [Vec<IVec2>; 3] {
        let a = vec![IVec2::new(-1, -6)];
        let b = (-16..=-8)
            .map(|x| IVec2::new(x, -2))
            .chain([IVec2::new(-8, -3), IVec2::new(-8, -4)])
            .collect();
        let c = vec![IVec2::new(9, -5), IVec2::new(10, -5)];
        [a, b, c]
    }

    /// Where a route from the nest crosses the ridge (its first cell at y = -6).
    fn crossing(field: &FlowField, grid: &Grid) -> IVec2 {
        let start = field.node_at(grid.nests[0].center().xz());
        *field
            .route(start)
            .iter()
            .find(|node| node.y == -6)
            .expect("the route crosses the ridge")
    }

    #[test]
    fn walling_off_a_gate_sends_the_horde_to_the_next() {
        let (mut grid, landmarks) = starting_map();
        let [a, b, c] = gates();
        for cells in [&a, &b, &c] {
            for &cell in cells {
                assert_eq!(grid.check_build(cell), Ok(()), "{cell}");
            }
        }
        let swarmers =
            |grid: &Grid| FlowField::new(grid, Class::Swarmer, landmarks.command_center.cells());
        // Open: straight through the narrow gate.
        assert_eq!(crossing(&swarmers(&grid), &grid), a[0]);
        let wall = |grid: &mut Grid, cells: &[IVec2]| {
            for &cell in cells {
                grid.set_structure(cell, Some(Structure::WALL));
            }
        };
        // Walled: through the mud.
        wall(&mut grid, &a);
        assert!((-12..=-10).contains(&crossing(&swarmers(&grid), &grid).x));
        // The mud sealed off too: the long way round, through the east gate.
        wall(&mut grid, &b);
        assert!((9..=10).contains(&crossing(&swarmers(&grid), &grid).x));
        // All three: it breaks through, at the narrow gate.
        wall(&mut grid, &c);
        let field = swarmers(&grid);
        let start = field.node_at(grid.nests[0].center().xz());
        assert_eq!(field.breaches(start, &grid), a);
    }

    #[test]
    fn brutes_never_take_the_narrow_gate() {
        let (grid, landmarks) = starting_map();
        let brutes = FlowField::new(&grid, Class::Brute, landmarks.command_center.cells());
        let x = crossing(&brutes, &grid).x;
        assert!(
            (-12..=-11).contains(&x) || x == 9,
            "brutes cross the ridge at {x}"
        );
    }
}
