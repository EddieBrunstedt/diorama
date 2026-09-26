//! Game flow: the timed prepare phase, ringing the bell to start the wave early, and the wave.
//!
//! `Intro` (the island assembles and the keeper lands) → `Prepare` (timed) → `Wave` → `Prepare`
//! → ... Milestone 4 fills in the wave (slimes, towers, game over) and milestone 5 puts a tile offer
//! between the wave and the next prepare phase. For now the wave is a short placeholder.

use crate::juice::Tween;
use crate::keeper::{Keeper, PlanSystems};
use crate::tuning::{BELL_COINS_PER_SECOND, PLACEHOLDER_WAVE_SECONDS, PREPARE_SECONDS};
use bevy::prelude::*;

pub const BELL_KEY: KeyCode = KeyCode::KeyB;

pub struct PhasePlugin;

impl Plugin for PhasePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameState>()
            .init_resource::<PrepareClock>()
            .init_resource::<Waves>()
            .init_resource::<WaveClock>()
            .add_message::<RingBell>()
            .add_systems(OnEnter(GameState::Prepare), start_prepare)
            .add_systems(OnEnter(GameState::Wave), start_wave)
            .add_systems(
                Update,
                (
                    end_intro.run_if(in_state(GameState::Intro)),
                    (tick_prepare, ring_bell)
                        .chain()
                        .run_if(in_state(GameState::Prepare)),
                    run_placeholder_wave.run_if(in_state(GameState::Wave)),
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
    /// Enemies attack.
    Wave,
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

/// Seconds until the wave starts, while there's a deadline to plan against.
pub fn wave_deadline(state: GameState, clock: &PrepareClock) -> Option<f32> {
    match state {
        GameState::Intro | GameState::Prepare => Some(clock.left),
        GameState::Wave => None,
    }
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

/// Time left in the placeholder wave.
#[derive(Resource, Default)]
struct WaveClock(f32);

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

fn start_wave(mut wave: ResMut<WaveClock>) {
    wave.0 = PLACEHOLDER_WAVE_SECONDS;
}

fn run_placeholder_wave(
    time: Res<Time>,
    mut wave: ResMut<WaveClock>,
    mut waves: ResMut<Waves>,
    mut next: ResMut<NextState<GameState>>,
) {
    wave.0 -= time.delta_secs();
    if wave.0 <= 0.0 {
        waves.completed += 1;
        next.set(GameState::Prepare);
    }
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
        assert_eq!(wave_deadline(GameState::Wave, &clock), None);
    }
}
