//! The tile offer after each survived wave: a few random tiles, the player picks one and places
//! it on the island's edge, and the next prepare phase starts.
//!
//! Which tiles are offered and where they may go is plain data in [`rules`]. The rest is the ECS
//! side: picking a card (clicking it in the UI, or its number key), a ghost of the tile following
//! the cursor over the free spots on the edge ([`ghost`]), and placing it with a click. The island
//! itself adds the tile and drops it in (`island::PlaceTile`).

mod ghost;
pub mod rules;

use crate::camera::DioramaCamera;
use crate::dev::ScriptedCursor;
use crate::island::{Island, PlaceTile, TileKind, cell_to_world};
use crate::juice::{Burst, Particles};
use crate::keeper::cursor_position;
use crate::palette;
use crate::phase::GameState;
use crate::rng::Rng;
use crate::ui::Popup;
use crate::voxel::TILE_SIZE;
use bevy::picking::hover::HoverMap;
use bevy::picking::pointer::PointerId;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use rules::Blocked;

/// Seconds between placing the tile and the prepare phase starting, to watch it land.
const PLACED_PAUSE: f32 = 1.1;
const PICK_KEYS: [KeyCode; 3] = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3];
pub const PUT_BACK_KEY: KeyCode = KeyCode::Escape;

pub struct OfferPlugin;

impl Plugin for OfferPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TileOffer>()
            .init_resource::<Placement>()
            .init_resource::<ghost::GhostAssets>()
            .insert_resource(OfferRng(Rng::new(seed_from_clock())))
            .add_message::<PickCard>()
            .add_message::<ghost::Rejected>()
            .add_systems(Startup, ghost::spawn_ghost)
            .add_systems(OnEnter(GameState::TileOffer), roll_offer)
            .add_systems(OnExit(GameState::TileOffer), end_offer)
            .add_systems(
                Update,
                (
                    (pick_cards, hover_spot, place_tile, finish_offer)
                        .chain()
                        .run_if(in_state(GameState::TileOffer)),
                    ghost::move_ghost,
                    ghost::show_slots,
                )
                    .chain(),
            );
    }
}

/// The tiles on offer, and how picking and placing one is going.
#[derive(Resource, Default, Debug)]
pub struct TileOffer {
    pub cards: Vec<TileKind>,
    /// The card picked for placing.
    pub picked: Option<usize>,
    /// Seconds since the picked card's tile was placed, once it has been.
    pub placed: Option<f32>,
    /// Goes up with every new offer, so the UI knows to deal new cards.
    pub round: u32,
}

impl TileOffer {
    /// The kind of tile being placed, while one is picked and not placed yet.
    pub fn placing(&self) -> Option<TileKind> {
        if self.placed.is_some() {
            return None;
        }
        self.picked.and_then(|index| self.cards.get(index).copied())
    }
}

/// Where the picked tile would go: the free spot on the edge under the cursor, if any.
#[derive(Resource, Default, Debug)]
pub struct Placement {
    pub hover: Option<Spot>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    pub cell: IVec2,
    pub kind: TileKind,
    pub fits: Result<(), Blocked>,
}

/// Picks a card to place (e.g. it was clicked), or puts the picked one back (`None`).
#[derive(Message, Clone, Copy, Debug)]
pub struct PickCard(pub Option<usize>);

#[derive(Resource)]
struct OfferRng(Rng);

/// A different offer every run, unless `DIORAMA_SEED` fixes it (see [`crate::dev`]).
fn seed_from_clock() -> u64 {
    if let Some(seed) = std::env::var("DIORAMA_SEED")
        .ok()
        .and_then(|seed| seed.parse().ok())
    {
        return seed;
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos() as u64)
}

fn roll_offer(island: Res<Island>, mut offer: ResMut<TileOffer>, mut rng: ResMut<OfferRng>) {
    *offer = TileOffer {
        cards: rules::roll(&island, &mut rng.0).to_vec(),
        picked: None,
        placed: None,
        round: offer.round + 1,
    };
}

fn end_offer(mut offer: ResMut<TileOffer>, mut placement: ResMut<Placement>) {
    offer.picked = None;
    offer.placed = None;
    placement.hover = None;
}

/// Picks cards with the number keys or from the UI; Esc or a right click puts the card back.
fn pick_cards(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut picks: MessageReader<PickCard>,
    mut offer: ResMut<TileOffer>,
) {
    let mut wanted = picks.read().last().map(|pick| pick.0);
    for (index, key) in PICK_KEYS.iter().enumerate() {
        if keys.just_pressed(*key) {
            wanted = Some(Some(index));
        }
    }
    if keys.just_pressed(PUT_BACK_KEY) || mouse.just_pressed(MouseButton::Right) {
        wanted = Some(None);
    }
    let Some(wanted) = wanted.filter(|_| offer.placed.is_none()) else {
        return;
    };
    let wanted = wanted.filter(|&index| index < offer.cards.len());
    // Picking the picked card again puts it back.
    let picked = if wanted == offer.picked { None } else { wanted };
    offer.picked = picked;
}

/// Finds the free spot on the island's edge under the cursor.
fn hover_spot(
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<DioramaCamera>>,
    scripted: Option<Res<ScriptedCursor>>,
    hover: Res<HoverMap>,
    ui_nodes: Query<(), With<Node>>,
    island: Res<Island>,
    offer: Res<TileOffer>,
    mut placement: ResMut<Placement>,
) {
    let over_ui = hover
        .get(&PointerId::Mouse)
        .is_some_and(|hits| hits.keys().any(|&entity| ui_nodes.contains(entity)));
    let (camera, camera_transform) = *camera;
    let point = cursor_position(&window, scripted.as_deref())
        .filter(|_| !over_ui)
        .and_then(|cursor| camera.viewport_to_world(camera_transform, cursor).ok())
        .and_then(|ray| ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)));
    let spot = offer.placing().zip(point).and_then(|(kind, point)| {
        let cell = (point.xz() / TILE_SIZE).round().as_ivec2();
        let fits = rules::check(&island, cell, kind);
        // Only free spots on the edge count; elsewhere there's nothing to show.
        let on_edge = !matches!(fits, Err(Blocked::Taken | Blocked::Adrift));
        on_edge.then_some(Spot { cell, kind, fits })
    });
    if placement.hover != spot {
        placement.hover = spot;
    }
}

/// A click on a free spot places the tile; on a spot where it doesn't fit, says why.
fn place_tile(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    placement: Res<Placement>,
    mut offer: ResMut<TileOffer>,
    mut place: MessageWriter<PlaceTile>,
    mut popups: MessageWriter<Popup>,
    mut rejected: MessageWriter<ghost::Rejected>,
    mut particles: Particles,
) {
    if !mouse.just_pressed(MouseButton::Left) || offer.placed.is_some() {
        return;
    }
    let Some(spot) = placement.hover else {
        return;
    };
    let at = cell_to_world(spot.cell);
    if let Err(blocked) = spot.fits {
        popups.write(Popup::problem(at + Vec3::Y * 0.4, blocked.message()));
        rejected.write(ghost::Rejected);
        return;
    }
    place.write(PlaceTile {
        cell: spot.cell,
        kind: spot.kind,
    });
    offer.placed = Some(0.0);
    // The ghost bursts into sparkles as the real tile takes its place.
    particles.burst(
        &mut commands,
        at + Vec3::Y * ghost::HOVER_HEIGHT,
        Burst {
            colors: &[palette::UI_PAPER, palette::FLAG, palette::CLOUD],
            count: 14,
            size: 0.05,
            spread: 1.4,
            lift: 1.6,
            lifetime: 0.6,
        },
    );
}

fn finish_offer(
    time: Res<Time>,
    mut offer: ResMut<TileOffer>,
    mut next: ResMut<NextState<GameState>>,
) {
    let Some(since) = &mut offer.placed else {
        return;
    };
    *since += time.delta_secs();
    if *since >= PLACED_PAUSE {
        next.set(GameState::Prepare);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placing_needs_a_picked_card_that_is_not_placed_yet() {
        let mut offer = TileOffer {
            cards: vec![TileKind::Forest, TileKind::Road, TileKind::Meadow],
            ..default()
        };
        assert_eq!(offer.placing(), None);
        offer.picked = Some(1);
        assert_eq!(offer.placing(), Some(TileKind::Road));
        offer.placed = Some(0.0);
        assert_eq!(offer.placing(), None);
    }
}
