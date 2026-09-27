//! The on-screen UI: the order queue and its timeline, the prepare clock and the bell, the wood
//! and hearth counters, the build choice, numbered markers over order targets, tower health over
//! hurt towers, a label at the cursor, popups and banners, the tile offer, and the game over
//! screen.

mod build_panel;
mod clock;
mod game_over;
mod hud;
mod icons;
mod offer_panel;
mod queue_panel;
mod timeline;
mod tower_health;

use crate::island::TileKind;
use crate::keeper::{Building, OrderId, OrderKind, PlanSystems, PlannedOrder, Problem};
use crate::palette;
use crate::phase::GameState;
use bevy::prelude::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Popup>()
            .add_message::<FlashOrder>()
            .add_message::<OrderFinished>()
            .init_resource::<icons::Icons>()
            .add_systems(
                Startup,
                (
                    hud::spawn_hud,
                    queue_panel::spawn_panel,
                    clock::spawn_clock,
                    game_over::spawn_game_over,
                    offer_panel::spawn_offer_panel,
                    build_panel::spawn_build_panel,
                ),
            )
            .add_systems(OnEnter(GameState::Prepare), clock::announce_prepare)
            .add_systems(OnEnter(GameState::Wave), clock::announce_wave)
            .add_systems(OnEnter(GameState::TileOffer), clock::announce_offer)
            .add_systems(
                Update,
                (
                    queue_panel::sync_cards,
                    queue_panel::animate_cards,
                    queue_panel::update_tooltip,
                    queue_panel::update_wave_divider,
                    queue_panel::update_late_note,
                    queue_panel::update_header_hint,
                    timeline::update_track,
                    clock::update_clock,
                    clock::update_bell,
                    clock::animate_banners,
                    hud::update_wood_counter,
                    hud::spawn_popups,
                    hud::animate_popups,
                    hud::update_markers,
                    hud::update_cursor_label,
                )
                    .chain()
                    .after(PlanSystems),
            )
            .add_systems(
                Update,
                (
                    clock::announce_cleared,
                    hud::update_hearth_counter,
                    build_panel::update_build_panel,
                    tower_health::update_tower_bars,
                    game_over::update_game_over,
                    (
                        offer_panel::update_offer_panel,
                        offer_panel::animate_offer_cards,
                    )
                        .chain(),
                )
                    .after(PlanSystems),
            );
    }
}

/// Text that pops up over a spot on the island and floats away.
#[derive(Message, Clone, Debug)]
pub struct Popup {
    pub at: Vec3,
    pub text: String,
    pub color: Color,
}

impl Popup {
    pub fn new(at: Vec3, text: impl Into<String>, color: Color) -> Self {
        Self {
            at,
            text: text.into(),
            color,
        }
    }

    pub fn problem(at: Vec3, text: impl Into<String>) -> Self {
        Self::new(at, text, palette::PROBLEM)
    }
}

/// Draws attention to an order's card (e.g. when its target is clicked again).
#[derive(Message, Clone, Copy, Debug)]
pub struct FlashOrder(pub OrderId);

/// An order was carried out (as opposed to removed), so its card leaves with a flourish.
#[derive(Message, Clone, Copy, Debug)]
pub struct OrderFinished(pub OrderId);

pub fn order_color(kind: &OrderKind) -> Color {
    match kind {
        OrderKind::Chop(_) => palette::ORDER_CHOP,
        OrderKind::Build(..) => palette::ORDER_BUILD,
        OrderKind::Repair(_) => palette::ORDER_REPAIR,
        OrderKind::Man(_) => palette::ORDER_CATAPULT,
        OrderKind::Walk(_) => palette::ORDER_WALK,
    }
}

pub fn order_title(kind: &OrderKind) -> &'static str {
    match kind {
        OrderKind::Chop(_) => "Chop tree",
        OrderKind::Build(_, Building::Tower) => "Build tower",
        OrderKind::Build(_, Building::Catapult) => "Build catapult",
        OrderKind::Repair(_) => "Repair tower",
        OrderKind::Man(_) => "Man catapult",
        OrderKind::Walk(_) => "Walk here",
    }
}

pub fn building_name(building: Building) -> &'static str {
    match building {
        Building::Tower => "Tower",
        Building::Catapult => "Catapult",
    }
}

/// The wood change shown on a card, e.g. "+1" or "-3".
pub fn wood_label(kind: &OrderKind) -> String {
    match kind.wood_change() {
        0 => String::new(),
        change => format!("{change:+}"),
    }
}

/// Explains a planned order's problem, given the wood in the backpack just before it.
pub fn problem_text(planned: &PlannedOrder, wood_before: u32) -> Option<String> {
    Some(match planned.problem? {
        Problem::NotEnoughWood => {
            let needs = -planned.order.kind.wood_change();
            format!("Not enough wood: needs {needs}, will have {wood_before}")
        }
        Problem::BackpackFull => "Backpack will be full".to_string(),
        Problem::Unreachable => Problem::Unreachable.message().to_string(),
        Problem::WaveOn => "No building during the wave: waits for the next prepare phase".into(),
        Problem::NoWave => "Waits for the wave: he gets in when the slimes come".into(),
    })
}

/// The color that flags a problem: red for orders that will be skipped, slime lilac for orders
/// waiting for their phase (builds during the wave, the catapult outside it).
pub fn problem_color(problem: Problem) -> Color {
    if problem.holds() {
        palette::WAVE_HOLD
    } else {
        palette::PROBLEM
    }
}

/// A short duration, e.g. "2.5s", or "12s" once it's ten seconds or more.
pub fn format_seconds(seconds: f32) -> String {
    let tenths = (seconds.max(0.0) * 10.0).round() / 10.0;
    if tenths < 10.0 {
        format!("{tenths:.1}s")
    } else {
        format!("{:.0}s", seconds.round())
    }
}

/// How long an order takes, e.g. "1.4s walk + 2.0s chop".
pub fn duration_text(planned: &PlannedOrder) -> String {
    let walk = format!("{} walk", format_seconds(planned.walk_seconds));
    let work = match planned.order.kind {
        OrderKind::Chop(_) => "chop",
        OrderKind::Build(..) => "build",
        OrderKind::Repair(_) => "repair",
        OrderKind::Man(_) => return format!("{walk}, then in until you call him out"),
        OrderKind::Walk(_) => return walk,
    };
    format!("{walk} + {} {work}", format_seconds(planned.work_seconds))
}

pub fn tile_name(kind: TileKind) -> &'static str {
    match kind {
        TileKind::Forest => "Forest",
        TileKind::Meadow => "Meadow",
        TileKind::Road => "Road",
        TileKind::Hearth => "Hearth",
    }
}

/// The color that stands for a kind of tile, e.g. on its card's border.
pub fn tile_color(kind: TileKind) -> Color {
    match kind {
        TileKind::Forest => palette::PINE,
        TileKind::Meadow => crate::voxel::shade(palette::MEADOW_GRASS, 0.8),
        TileKind::Road => crate::voxel::shade(palette::ROAD, 0.8),
        TileKind::Hearth => palette::HEARTH_STONE,
    }
}

/// Rounded paper panel styling shared by the HUD.
fn paper(radius: f32) -> (BackgroundColor, BorderColor, BoxShadow) {
    (
        BackgroundColor(palette::UI_PAPER.with_alpha(0.92)),
        BorderColor::all(palette::UI_PAPER_DARK),
        BoxShadow::new(
            Color::BLACK.with_alpha(0.18),
            Val::Px(0.0),
            Val::Px(3.0),
            Val::Px(0.0),
            Val::Px(radius / 2.0),
        ),
    )
}

fn font(size: f32) -> TextFont {
    TextFont::from_font_size(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_are_short() {
        assert_eq!(format_seconds(2.0), "2.0s");
        assert_eq!(format_seconds(0.26), "0.3s");
        assert_eq!(format_seconds(9.96), "10s");
        assert_eq!(format_seconds(12.4), "12s");
    }

    #[test]
    fn wood_labels_are_signed() {
        assert_eq!(wood_label(&OrderKind::tower(IVec2::ZERO)), "-3");
        assert_eq!(
            wood_label(&OrderKind::Build(IVec2::ZERO, Building::Catapult)),
            format!("-{}", crate::tuning::CATAPULT_WOOD_COST)
        );
        assert_eq!(wood_label(&OrderKind::Repair(IVec2::ZERO)), "");
        assert_eq!(wood_label(&OrderKind::Walk(Vec2::ZERO)), "");
    }
}
