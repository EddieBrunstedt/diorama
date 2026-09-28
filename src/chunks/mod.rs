//! Claiming chunks: the map grows by one chunk after every wave.
//!
//! When the build phase after a wave begins, a few chunks are offered at the edge of the map
//! ([`offer`]), shown where they'd go ([`ghosts`]). The player claims one by clicking it: it drops
//! onto the map, and a nest in it spawns from the coming wave on (past a few nests, the oldest
//! goes quiet: see [`Grid::live_nests`]). The claim is one step in the undo history
//! ([`crate::build::history::claim_chunk`]), so it can be taken back (and another one claimed)
//! until the wave starts. The wave can't start until one is claimed. While the cursor is on an
//! offered chunk, the path preview shows the routes as they'd be with it claimed
//! ([`preview_claim`]).
//!
//! The templates chunks are made from, and how they're laid down, are plain data in
//! [`templates`].

mod ghosts;
pub mod offer;
pub mod templates;

use crate::build::history::{History, claim_chunk};
use crate::build::{ActiveStroke, BuildSystems, Report, UiHovered, strokes};
use crate::camera::{CameraRig, MainCamera};
use crate::economy::{Wallet, nest_bounty};
use crate::flow::{FlowFields, FlowSystems};
use crate::grid::Grid;
use crate::map::{ChunkChanged, Landmarks};
use crate::pointer::{Pointer, PointerSystems};
use crate::preview::{PreviewSystems, Scenario, WhatIf};
use crate::rng::{Rng, RunSeed};
use crate::tuning::grid::CHUNK_SIZE;
use crate::tuning::juice::OFFER_HOVER_LIFT;
use crate::waves::plan::Run;
use crate::waves::{ClaimFirst, Phase};
use bevy::prelude::*;
use offer::Offer;
use templates::Claim;

pub struct ChunksPlugin;

impl Plugin for ChunksPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkOffer>()
            .init_resource::<OfferHover>()
            .add_message::<ChunkClaimed>()
            .add_plugins(ghosts::GhostsPlugin)
            .add_systems(Startup, seed_offers)
            .add_systems(OnEnter(Phase::Build), make_offer)
            .add_systems(OnEnter(Phase::Wave), close_offer)
            .add_systems(OnExit(Phase::GameOver), close_offer)
            .add_systems(
                Update,
                (hover_offer, claim.after(strokes))
                    .chain()
                    .run_if(in_state(Phase::Build))
                    .in_set(BuildSystems)
                    .after(PointerSystems),
            )
            .add_systems(
                Update,
                preview_claim
                    .after(FlowSystems)
                    .before(PreviewSystems)
                    .run_if(resource_changed::<OfferHover>.or_else(resource_changed::<FlowFields>)),
            )
            .add_systems(Update, (frame_the_map, show_the_offer).after(BuildSystems));
    }
}

/// The chunks offered this build phase, if any.
#[derive(Resource, Default, Debug)]
pub struct ChunkOffer(pub Option<Offer>);

impl ChunkOffer {
    /// Whether a chunk has to be claimed before the wave can start.
    pub fn pending(&self, grid: &Grid) -> bool {
        self.0.as_ref().is_some_and(|offer| offer.pending(grid))
    }
}

/// The offered chunk under the cursor, while one can be claimed.
#[derive(Resource, Default, Debug)]
pub struct OfferHover(pub Option<Claim>);

/// A chunk was claimed (not undone or redone), paying `bounty`, with its middle at `at`.
#[derive(Message, Clone, Copy, Debug)]
pub struct ChunkClaimed {
    pub at: Vec3,
    pub bounty: u32,
}

/// Draws the offers, so the same seed (`DIORAMA_SEED`) makes the same ones.
#[derive(Resource)]
struct OfferRng(Rng);

fn seed_offers(mut commands: Commands, seed: Res<RunSeed>) {
    commands.insert_resource(OfferRng(Rng::new(seed.0 ^ 0xC4A2)));
}

/// After every cleared wave (not before the first), new ground is offered.
fn make_offer(
    run: Option<Res<Run>>,
    grid: Res<Grid>,
    landmarks: Res<Landmarks>,
    rng: Option<ResMut<OfferRng>>,
    mut offer: ResMut<ChunkOffer>,
) {
    let (Some(run), Some(mut rng)) = (run, rng) else {
        return;
    };
    if run.cleared == 0 {
        offer.0 = None;
        return;
    }
    let wave = run.wave.saturating_sub(1);
    offer.0 = Some(Offer::new(
        &grid,
        &landmarks,
        wave,
        nest_bounty(wave),
        &mut rng.0,
    ));
}

/// Starting the wave settles the claim; a new run starts without one.
fn close_offer(mut offer: ResMut<ChunkOffer>, mut hover: ResMut<OfferHover>) {
    offer.0 = None;
    hover.0 = None;
}

fn hover_offer(
    pointer: Res<Pointer>,
    ui: Res<UiHovered>,
    grid: Res<Grid>,
    offer: Res<ChunkOffer>,
    mut hover: ResMut<OfferHover>,
) {
    let hovered = offer
        .0
        .as_ref()
        .filter(|offer| offer.pending(&grid) && !ui.0)
        .zip(pointer.cell)
        .and_then(|(offer, cell)| offer.choice_at(cell));
    if hover.0 != hovered {
        hover.0 = hovered;
    }
}

/// A left click on an offered chunk claims it.
fn claim(
    buttons: Res<ButtonInput<MouseButton>>,
    active: Res<ActiveStroke>,
    hover: Res<OfferHover>,
    offer: Res<ChunkOffer>,
    mut grid: ResMut<Grid>,
    mut wallet: ResMut<Wallet>,
    mut history: ResMut<History>,
    mut report: Report,
    mut claimed: MessageWriter<ChunkClaimed>,
) {
    if !buttons.just_pressed(MouseButton::Left) || active.0.is_some() {
        return;
    }
    let (Some(choice), Some(offer)) = (hover.0, offer.0.as_ref()) else {
        return;
    };
    let bounty = offer.bounty_for(choice);
    let Some((edit, action)) = claim_chunk(choice, bounty, &mut grid, &mut wallet) else {
        return;
    };
    history.record(edit);
    report.send(vec![action]);
    let at = choice.nest().map_or_else(
        || chunk_area(choice.slot).center(),
        |nest| nest.area().center(),
    );
    claimed.write(ChunkClaimed {
        at: Vec3::new(at.x, 0.0, at.y),
        bounty,
    });
}

/// While the cursor is on an offered chunk, the path preview shows the map as it would be with
/// that chunk claimed: its nest's routes, and how the others' change.
fn preview_claim(
    hover: Res<OfferHover>,
    offer: Res<ChunkOffer>,
    grid: Res<Grid>,
    landmarks: Res<Landmarks>,
    mut what_if: ResMut<WhatIf>,
) {
    // The click that claims a chunk leaves the hover on it for the rest of that frame.
    match hover.0.filter(|_| offer.pending(&grid)) {
        Some(choice) => {
            let mut claimed = grid.clone();
            choice.add_to(&mut claimed);
            let raised = Some((choice.slot, OFFER_HOVER_LIFT));
            what_if.0 = Some(Box::new(Scenario::new(claimed, &landmarks, raised)));
        }
        None => {
            if what_if.0.is_some() {
                what_if.0 = None;
            }
        }
    }
}

/// The ground a chunk slot covers (world x and z).
pub fn chunk_area(slot: IVec2) -> Rect {
    let min = Grid::chunk_origin(slot).as_vec2();
    Rect::from_corners(min, min + Vec2::splat(CHUNK_SIZE as f32))
}

/// Keeps the camera's view of the map up to date as it grows: the map and any chunks on offer.
/// When new ground is offered, the camera glides out to show all of it, even if the player had
/// moved it.
fn frame_the_map(
    grid: Res<Grid>,
    offer: Res<ChunkOffer>,
    mut changes: MessageReader<ChunkChanged>,
    mut rig: ResMut<CameraRig>,
    mut was_pending: Local<bool>,
) {
    let changed = changes.read().count() > 0;
    let pending = offer.pending(&grid);
    if !changed && pending == *was_pending {
        return;
    }
    let fresh = pending && !*was_pending;
    *was_pending = pending;
    if let Some(area) = map_area(&grid, &offer) {
        rig.show(area, fresh);
    }
}

/// The ground the map covers, and the chunks on offer while one has to be claimed.
fn map_area(grid: &Grid, offer: &ChunkOffer) -> Option<Rect> {
    let bounds = grid.bounds()?;
    let mut area = Rect::from_corners(bounds.min.as_vec2(), (bounds.max + IVec2::ONE).as_vec2());
    if let Some(offer) = offer.0.as_ref().filter(|offer| offer.pending(grid)) {
        for choice in &offer.choices {
            area = area.union(chunk_area(choice.slot));
        }
    }
    Some(area)
}

/// Asking to start the wave before claiming a chunk brings the chunks on offer back into view,
/// if the player had moved the camera off them.
fn show_the_offer(
    mut asked: MessageReader<ClaimFirst>,
    grid: Res<Grid>,
    offer: Res<ChunkOffer>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut rig: ResMut<CameraRig>,
) {
    if asked.read().count() == 0 {
        return;
    }
    let Some(choices) = offer.0.as_ref().map(|offer| &offer.choices) else {
        return;
    };
    let (camera, transform) = *camera;
    let Some(view) = camera.logical_viewport_rect() else {
        return;
    };
    let on_screen = |corner: Vec2| {
        camera
            .world_to_viewport(transform, Vec3::new(corner.x, 0.0, corner.y))
            .is_ok_and(|at| view.contains(at))
    };
    let all_in_view = choices.iter().all(|choice| {
        let area = chunk_area(choice.slot);
        [
            area.min,
            area.max,
            Vec2::new(area.min.x, area.max.y),
            Vec2::new(area.max.x, area.min.y),
        ]
        .into_iter()
        .all(on_screen)
    });
    if !all_in_view && let Some(area) = map_area(&grid, &offer) {
        rig.show(area, true);
    }
}
