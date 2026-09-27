//! Game flow: the timed prepare phase, ringing the bell to start the wave early, the wave, the
//! tile offer, and game over.
//!
//! `Intro` (the island assembles and the keeper lands) → `Prepare` (timed) → `Wave` (until every
//! slime is gone) → `TileOffer` (pick a tile and place it; see [`crate::offer`]) → `Prepare` → ...
//! until the hearth goes out: `GameOver`, then back to `Intro` for a new run.
//!
//! Starting over: every plugin tears down its part of the old run on `OnExit(GameOver)` and
//! spawns the new one on `OnEnter(Intro)`, which also runs when the game starts.

use crate::hearth::HearthHealth;
use crate::juice::Tween;
use crate::keeper::{Keeper, PlanSystems};
use crate::slimes::WaveRun;
use crate::tuning::{BELL_COINS_PER_SECOND, PREPARE_SECONDS};
use bevy::prelude::*;

pub const BELL_KEY: KeyCode = KeyCode::KeyB;
pub const RESTART_KEY: KeyCode = KeyCode::KeyR;
/// Seconds between the last slime going and the wave ending, to enjoy the moment.
const CLEARED_PAUSE: f32 = 1.6;

pub struct PhasePlugin;

impl Plugin for PhasePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameState>()
            .init_resource::<PrepareClock>()
            .init_resource::<Waves>()
            .init_resource::<Cleared>()
            .add_message::<RingBell>()
            .add_message::<WaveCleared>()
            .add_message::<Restart>()
            .add_systems(OnEnter(GameState::Prepare), start_prepare)
            .add_systems(OnEnter(GameState::Wave), start_wave)
            .add_systems(OnEnter(GameState::TileOffer), start_tile_offer)
            .add_systems(OnExit(GameState::GameOver), reset_run)
            .add_systems(
                Update,
                (
                    end_intro.run_if(in_state(GameState::Intro)),
                    (tick_prepare, ring_bell)
                        .chain()
                        .run_if(in_state(GameState::Prepare)),
                    end_wave.run_if(in_state(GameState::Wave)),
                    restart.run_if(in_state(GameState::GameOver)),
                )
                    .before(PlanSystems),
            );
    }
}

#[derive(States, Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GameState {
    /// The island assembles and the keeper drops in. Orders can be given, the clock waits.
    #[default]
    Intro,
    /// The timed build-up before a wave.
    Prepare,
    /// Slimes come down the road until they're all gone.
    Wave,
    /// The wave was survived: pick a new tile and place it. The keeper waits, the clock hasn't
    /// started yet.
    TileOffer,
    /// The hearth went out. Shows the score until the player starts over.
    GameOver,
}

/// Counts down the prepare phase.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct PrepareClock {
    pub total: f32,
    pub left: f32,
}

impl Default for PrepareClock {
    fn default() -> Self {
        Self::new(PREPARE_SECONDS)
    }
}

impl PrepareClock {
    pub fn new(total: f32) -> Self {
        Self { total, left: total }
    }

    /// Advances the clock. Returns whether time ran out on this tick.
    pub fn tick(&mut self, seconds: f32) -> bool {
        let was_running = self.left > 0.0;
        self.left = (self.left - seconds).max(0.0);
        was_running && self.left == 0.0
    }

    pub fn elapsed(&self) -> f32 {
        self.total - self.left
    }
}

/// Seconds until the wave starts, while there's a deadline to plan against. During the tile
/// offer that's the whole prepare phase coming up.
pub fn wave_deadline(state: GameState, clock: &PrepareClock) -> Option<f32> {
    match state {
        GameState::Intro | GameState::Prepare | GameState::TileOffer => Some(clock.left),
        GameState::Wave | GameState::GameOver => None,
    }
}

/// Whether the keeper may start building. Not during the wave: builds wait for the next
/// prepare phase. The tile offer plans for the prepare phase coming up.
pub fn building_allowed(state: GameState) -> bool {
    matches!(
        state,
        GameState::Intro | GameState::Prepare | GameState::TileOffer
    )
}

/// Whether the keeper carries out orders. He waits while a tile is picked, so taking time over
/// the offer doesn't buy him extra time.
pub fn keeper_working(state: GameState) -> bool {
    !matches!(state, GameState::TileOffer | GameState::GameOver)
}

/// Where the game goes once a wave is survived.
pub fn after_wave() -> GameState {
    GameState::TileOffer
}

/// Bonus coin for ringing the bell with this much prepare time left.
pub fn bell_bonus(seconds_left: f32) -> u32 {
    (seconds_left.max(0.0) * BELL_COINS_PER_SECOND).floor() as u32
}

/// How the run is going.
#[derive(Resource, Clone, Debug, Default)]
pub struct Waves {
    /// Waves survived: the score.
    pub completed: u32,
    /// Coin earned so far. Nothing to spend it on yet.
    pub coins: u32,
    /// Bonus coin for ringing the bell early for the current wave, if it was rung.
    pub bell_bonus: Option<u32>,
}

impl Waves {
    /// The wave that's on (during a wave) or coming up next (while preparing).
    pub fn current(&self) -> u32 {
        self.completed + 1
    }
}

/// Asks to ring the bell and start the wave early (e.g. the bell button was clicked).
#[derive(Message, Clone, Copy, Debug)]
pub struct RingBell;

/// The last slime of a wave is gone. Carries the wave's number.
#[derive(Message, Clone, Copy, Debug)]
pub struct WaveCleared(pub u32);

/// Asks to start a new run after game over (e.g. the "Play again" button was clicked).
#[derive(Message, Clone, Copy, Debug)]
pub struct Restart;

/// Seconds since the current wave was cleared, if it has been.
#[derive(Resource, Default)]
struct Cleared(Option<f32>);

fn end_intro(
    keeper: Query<(), (With<Keeper>, Without<Tween>)>,
    mut next: ResMut<NextState<GameState>>,
) {
    if !keeper.is_empty() {
        next.set(GameState::Prepare);
    }
}

fn start_prepare(mut clock: ResMut<PrepareClock>, mut waves: ResMut<Waves>) {
    *clock = PrepareClock::default();
    waves.bell_bonus = None;
}

fn tick_prepare(
    time: Res<Time>,
    mut clock: ResMut<PrepareClock>,
    mut next: ResMut<NextState<GameState>>,
) {
    if clock.tick(time.delta_secs()) {
        next.set(GameState::Wave);
    }
}

fn ring_bell(
    keys: Res<ButtonInput<KeyCode>>,
    mut rings: MessageReader<RingBell>,
    clock: Res<PrepareClock>,
    mut waves: ResMut<Waves>,
    mut next: ResMut<NextState<GameState>>,
) {
    let rung = rings.read().count() > 0 || keys.just_pressed(BELL_KEY);
    if !rung || clock.left <= 0.0 {
        return;
    }
    let bonus = bell_bonus(clock.left);
    waves.coins += bonus;
    waves.bell_bonus = Some(bonus);
    next.set(GameState::Wave);
}

fn start_wave(mut cleared: ResMut<Cleared>) {
    cleared.0 = None;
}

/// The clock is wound for the next prepare phase, so the timeline shows the plan against it.
fn start_tile_offer(mut clock: ResMut<PrepareClock>) {
    *clock = PrepareClock::default();
}

/// Ends the wave: game over when the hearth is out, survived once every slime is gone.
fn end_wave(
    time: Res<Time>,
    run: Res<WaveRun>,
    hearth: Res<HearthHealth>,
    mut cleared: ResMut<Cleared>,
    mut waves: ResMut<Waves>,
    mut announce: MessageWriter<WaveCleared>,
    mut next: ResMut<NextState<GameState>>,
) {
    if hearth.is_out() {
        next.set(GameState::GameOver);
        return;
    }
    if !run.cleared() {
        return;
    }
    match &mut cleared.0 {
        None => {
            cleared.0 = Some(0.0);
            announce.write(WaveCleared(waves.current()));
        }
        Some(since) => {
            *since += time.delta_secs();
            if *since >= CLEARED_PAUSE {
                waves.completed += 1;
                next.set(after_wave());
            }
        }
    }
}

fn restart(
    keys: Res<ButtonInput<KeyCode>>,
    mut restarts: MessageReader<Restart>,
    mut next: ResMut<NextState<GameState>>,
) {
    if restarts.read().count() > 0 || keys.just_pressed(RESTART_KEY) {
        next.set(GameState::Intro);
    }
}

fn reset_run(mut waves: ResMut<Waves>, mut clock: ResMut<PrepareClock>) {
    *waves = Waves::default();
    *clock = PrepareClock::default();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_runs_out_once() {
        let mut clock = PrepareClock::new(1.0);
        assert!(!clock.tick(0.6));
        assert!((clock.elapsed() - 0.6).abs() < 1e-6);
        assert!(clock.tick(0.6), "runs out on this tick");
        assert_eq!(clock.left, 0.0);
        assert!(!clock.tick(0.6), "and only once");
    }

    #[test]
    fn bell_bonus_counts_whole_seconds_left() {
        assert_eq!(bell_bonus(12.7), (12.0 * BELL_COINS_PER_SECOND) as u32);
        assert_eq!(bell_bonus(0.4), 0);
        assert_eq!(bell_bonus(-1.0), 0);
    }

    #[test]
    fn deadline_only_while_preparing() {
        let clock = PrepareClock {
            total: 30.0,
            left: 12.0,
        };
        assert_eq!(wave_deadline(GameState::Prepare, &clock), Some(12.0));
        assert_eq!(wave_deadline(GameState::Intro, &clock), Some(12.0));
        assert_eq!(wave_deadline(GameState::TileOffer, &clock), Some(12.0));
        assert_eq!(wave_deadline(GameState::Wave, &clock), None);
        assert_eq!(wave_deadline(GameState::GameOver, &clock), None);
    }

    #[test]
    fn no_building_during_the_wave() {
        assert!(building_allowed(GameState::Intro));
        assert!(building_allowed(GameState::Prepare));
        assert!(!building_allowed(GameState::Wave));
        assert!(!building_allowed(GameState::GameOver));
        assert!(
            building_allowed(GameState::TileOffer),
            "plans for the next prepare phase"
        );
    }

    #[test]
    fn the_keeper_waits_while_a_tile_is_picked() {
        assert!(keeper_working(GameState::Prepare));
        assert!(keeper_working(GameState::Wave));
        assert!(!keeper_working(GameState::TileOffer));
        assert!(!keeper_working(GameState::GameOver));
    }

    #[test]
    fn a_survived_wave_leads_to_the_tile_offer() {
        assert_eq!(after_wave(), GameState::TileOffer);
    }
}
