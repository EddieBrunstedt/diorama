//! On-screen text: the money in the top-left corner ([`money`]), the Command Center's health, the
//! phase and the size of the horde at the top, a controls hint for the phase under the money, the
//! wave preview and the game-over screen ([`wave`]), and a small HP bar over the wall, tower or
//! Command Center under the cursor, with what selling or repairing it would pay or cost (in the
//! build phase). Undo and redo dim when there's nothing to undo or redo. Labels name the chunks
//! offered after a wave ([`offer`]).

mod money;
mod offer;
mod wave;

use crate::build::history::History;
use crate::camera::MainCamera;
use crate::chunks::ChunkOffer;
use crate::economy::{
    Wallet, affordable_command_center_repair, affordable_repair, command_center_repair_cost,
    sell_value,
};
use crate::grid::{Grid, Structure, cell_center};
use crate::horde::{Brute, BruteQueue, SpawnQueue, Swarmer};
use crate::palette;
use crate::pointer::{Pointer, PointerSystems, column_top};
use crate::waves::{CurrentWave, Phase};
use bevy::prelude::*;

const FONT_SIZE: f32 = 15.0;
/// The hint sits under the money counter, leaving the bottom of the screen to the build panel.
const HINT_TOP: f32 = 64.0;
/// How faint a hint line is while it can't do anything.
const DIMMED_ALPHA: f32 = 0.3;
/// The health bar's size, and the fraction of HP below which it turns red.
const BAR_SIZE: Vec2 = Vec2::new(260.0, 10.0);
const LOW_HEALTH: f32 = 0.3;
/// A hovered wall's HP bar: its size (logical pixels), and how far above the wall's top it floats
/// (world units).
const WALL_BAR_SIZE: Vec2 = Vec2::new(40.0, 5.0);
const WALL_BAR_LIFT: f32 = 0.35;
const WALL_BAR_FONT_SIZE: f32 = 12.0;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            money::MoneyPlugin,
            wave::WaveHudPlugin,
            offer::OfferHudPlugin,
        ))
        .add_systems(Startup, (spawn_hint, spawn_status, spawn_wall_bar))
        .add_systems(
            Update,
            (show_hint, show_status, show_wall_bar.after(PointerSystems)),
        );
    }
}

#[derive(Component)]
struct HealthFill;

#[derive(Component)]
struct HealthText;

#[derive(Component)]
struct PhaseText;

#[derive(Component)]
struct SwarmerCount;

fn spawn_status(mut commands: Commands) {
    let text = |value: &str| {
        (
            Text::new(value),
            TextFont::from_font_size(FONT_SIZE),
            TextColor(palette::HINT_TEXT),
            Pickable::IGNORE,
        )
    };
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(4.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|status| {
            status.spawn((HealthText, text("")));
            status
                .spawn((
                    Node {
                        width: Val::Px(BAR_SIZE.x),
                        height: Val::Px(BAR_SIZE.y),
                        ..default()
                    },
                    BackgroundColor(palette::HEALTH_EMPTY),
                    Pickable::IGNORE,
                ))
                .with_child((
                    HealthFill,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(palette::HEALTH),
                    Pickable::IGNORE,
                ));
            status.spawn((PhaseText, text("")));
            status.spawn((SwarmerCount, text("")));
        });
}

fn show_status(
    grid: Res<Grid>,
    phase: Res<State<Phase>>,
    wave: Res<CurrentWave>,
    queue: Res<SpawnQueue>,
    brute_queue: Res<BruteQueue>,
    swarmers: Query<(), With<Swarmer>>,
    brutes: Query<(), With<Brute>>,
    mut fill: Single<(&mut Node, &mut BackgroundColor), With<HealthFill>>,
    mut texts: ParamSet<(
        Single<&mut Text, With<HealthText>>,
        Single<&mut Text, With<PhaseText>>,
        Single<(&mut Text, &mut Node), (With<SwarmerCount>, Without<HealthFill>)>,
    )>,
) {
    let health = grid.command_center;
    let fraction = health.hp as f32 / health.max.max(1) as f32;
    let (node, color) = &mut *fill;
    node.width = Val::Percent(fraction * 100.0);
    color.0 = if fraction < LOW_HEALTH {
        palette::HEALTH_LOW
    } else {
        palette::HEALTH
    };
    let label = format!("Command Center  {} / {}", health.hp, health.max);
    let mut health_text = texts.p0();
    if health_text.0 != label {
        health_text.0 = label;
    }
    let label = match phase.get() {
        Phase::Build => format!("Build phase  -  wave {} next", wave.0.number),
        Phase::Wave => format!("Wave {}", wave.0.number),
        Phase::GameOver => "Overrun".to_string(),
    };
    let mut phase_text = texts.p1();
    if phase_text.0 != label {
        phase_text.0 = label;
    }
    // The horde's size, while there is one.
    let mut count_text = texts.p2();
    let (count_text, count_node) = &mut *count_text;
    let display = if *phase.get() == Phase::Build {
        Display::None
    } else {
        Display::Flex
    };
    if count_node.display != display {
        count_node.display = display;
    }
    let count = |name: &str, alive: usize, waiting: u32| {
        if waiting > 0 {
            format!("{name}  {alive}  (+{waiting} coming)")
        } else {
            format!("{name}  {alive}")
        }
    };
    let count = format!(
        "{}      {}",
        count("Swarmers", swarmers.iter().len(), queue.waiting),
        count("Brutes", brutes.iter().len(), brute_queue.waiting),
    );
    if count_text.0 != count {
        count_text.0 = count;
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum HintLine {
    /// Only in this phase.
    In(Phase),
    Always,
    Undo,
    Redo,
    /// While a chunk has to be claimed.
    Claim,
}

fn spawn_hint(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(14.0),
                top: Val::Px(HINT_TOP),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|hint| {
            let build = HintLine::In(Phase::Build);
            for (line, text) in [
                (HintLine::Claim, "Left click a marked chunk: claim it"),
                (build, "1-5: pick what to build"),
                (build, "Left click: build, or repair (drag for a line)"),
                (build, "Right click: sell (drag for a line)"),
                (build, "Space: start the wave"),
                (HintLine::Undo, "Ctrl+Z: undo"),
                (HintLine::Redo, "Ctrl+Shift+Z: redo"),
                (
                    HintLine::In(Phase::Wave),
                    "1-2: aim an ability, left click: fire",
                ),
                (HintLine::In(Phase::Wave), "Esc / right click: stop aiming"),
                (HintLine::In(Phase::Wave), "Tab: game speed"),
                (HintLine::In(Phase::GameOver), "R: restart"),
                (HintLine::Always, "WASD / middle drag: pan    Scroll: zoom"),
            ] {
                hint.spawn((
                    line,
                    Text::new(text),
                    TextFont::from_font_size(FONT_SIZE),
                    TextColor(palette::HINT_TEXT),
                    Pickable::IGNORE,
                ));
            }
        });
}

/// Shows the lines for the phase, and dims undo and redo when there's nothing to undo or redo.
fn show_hint(
    history: Res<History>,
    phase: Res<State<Phase>>,
    grid: Res<Grid>,
    offer: Res<ChunkOffer>,
    mut lines: Query<(&HintLine, &mut TextColor, &mut Node)>,
) {
    if !history.is_changed() && !phase.is_changed() && !offer.is_changed() && !grid.is_changed() {
        return;
    }
    let building = *phase.get() == Phase::Build;
    let claiming = building && offer.pending(&grid);
    for (line, mut color, mut node) in &mut lines {
        let (shown, usable) = match *line {
            HintLine::In(only) => (only == *phase.get(), true),
            HintLine::Always => (true, true),
            HintLine::Undo => (building, history.can_undo()),
            HintLine::Redo => (building, history.can_redo()),
            HintLine::Claim => (claiming, true),
        };
        let display = if shown { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        let alpha = palette::HINT_TEXT.alpha() * if usable { 1.0 } else { DIMMED_ALPHA };
        color.set_if_neq(TextColor(palette::HINT_TEXT.with_alpha(alpha)));
    }
}

#[derive(Component)]
struct WallBar;

#[derive(Component)]
struct WallBarFill;

#[derive(Component)]
struct WallBarText;

/// What selling or repairing the hovered structure would pay or cost.
#[derive(Component)]
struct WallBarPrices;

fn spawn_wall_bar(mut commands: Commands) {
    commands
        .spawn((
            WallBar,
            Node {
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(1.0),
                ..default()
            },
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .with_children(|bar| {
            bar.spawn((
                WallBarPrices,
                Text::new(""),
                TextFont::from_font_size(WALL_BAR_FONT_SIZE),
                TextColor(palette::MONEY),
                Pickable::IGNORE,
            ));
            bar.spawn((
                WallBarText,
                Text::new(""),
                TextFont::from_font_size(WALL_BAR_FONT_SIZE),
                TextColor(palette::HINT_TEXT),
                Pickable::IGNORE,
            ));
            bar.spawn((
                Node {
                    width: Val::Px(WALL_BAR_SIZE.x),
                    height: Val::Px(WALL_BAR_SIZE.y),
                    padding: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(palette::WALL_BAR_BACK),
                Pickable::IGNORE,
            ))
            .with_child((
                WallBarFill,
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(palette::HEALTH),
                Pickable::IGNORE,
            ));
        });
}

/// Floats an HP bar over the wall, tower or Command Center under the cursor, with what it sells
/// for and what a repair would cost in the build phase.
fn show_wall_bar(
    pointer: Res<Pointer>,
    phase: Res<State<Phase>>,
    grid: Res<Grid>,
    wallet: Res<Wallet>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut bar: Single<(&mut Node, &mut Visibility, &ComputedNode), With<WallBar>>,
    mut fill: Single<(&mut Node, &mut BackgroundColor), (With<WallBarFill>, Without<WallBar>)>,
    mut text: Single<&mut Text, (With<WallBarText>, Without<WallBarPrices>)>,
    mut prices: Single<&mut Text, (With<WallBarPrices>, Without<WallBarText>)>,
) {
    let (node, visibility, computed) = &mut *bar;
    let (camera, transform) = *camera;
    // Only what the horde can break has HP to show.
    let above = pointer.cell.and_then(|cell| {
        let structure = grid.structure(cell)?;
        let health = match structure {
            Structure::CommandCenter => (grid.command_center.hp, grid.command_center.max),
            structure => structure.health()?,
        };
        let top = cell_center(cell) + Vec3::Y * (column_top(&grid, cell) + WALL_BAR_LIFT);
        let at = camera.world_to_viewport(transform, top).ok()?;
        Some((at, structure, health))
    });
    let Some((at, structure, (hp, max))) = above else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);
    // Centered over the wall, sitting on the point above it.
    let size = computed.size() * computed.inverse_scale_factor();
    node.left = Val::Px(at.x - size.x / 2.0);
    node.top = Val::Px(at.y - size.y);
    let fraction = hp as f32 / max.max(1) as f32;
    let (fill_node, color) = &mut *fill;
    fill_node.width = Val::Percent(fraction * 100.0);
    color.0 = if fraction < LOW_HEALTH {
        palette::HEALTH_LOW
    } else {
        palette::HEALTH
    };
    let name = structure
        .kind()
        .map_or("Command Center", |kind| kind.name());
    let label = format!("{name}  {hp} / {max}");
    if text.0 != label {
        text.0 = label;
    }
    // Prices only matter while there's building to do.
    let command_center = structure == Structure::CommandCenter;
    let mut label = String::new();
    if *phase.get() == Phase::Build {
        let (full, affordable) = if command_center {
            (
                command_center_repair_cost(hp, max),
                affordable_command_center_repair(grid.command_center, wallet.money()),
            )
        } else {
            label = format!("sell +{}", sell_value(structure));
            (
                crate::economy::repair_cost(structure.kind().unwrap_or_default(), hp, max),
                affordable_repair(structure, wallet.money()),
            )
        };
        if hp < max {
            let repair = match affordable {
                Some((to, cost)) if to < max => format!("repair -{cost} (to {to})"),
                Some((_, cost)) => format!("repair -{cost}"),
                None => format!("repair -{full}"),
            };
            label = format!("{label}   {repair}").trim().to_string();
        }
    }
    if prices.0 != label {
        prices.0 = label;
    }
}
