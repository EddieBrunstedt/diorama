//! The wave loop, as Bevy states ([`Phase`]):
//!
//! - **Build**: no timer. After a wave, a chunk has to be claimed first ([`crate::chunks`]).
//!   Everything built, sold, repaired or claimed can be undone until the wave starts (Space, or
//!   the Start button). The HUD previews the wave: its counts, packs and toughness, and what
//!   comes out of each nest.
//! - **Wave**: no building. The wave comes out of every nest in packs ([`plan`]), and Tab cycles
//!   the game speed (1×/2×/3×). It ends when every enemy is dead: the bonus is paid, and it's
//!   back to building for the next one.
//! - **GameOver**: the Command Center fell. The horde swarms over its ruin until R (or the
//!   Restart button) starts a new run from wave 1.
//!
//! Each feature area hooks into the phases itself: the build is committed when a wave starts
//! ([`crate::build`]), the purse opens ([`crate::economy`]), and a restart puts every area back
//! the way a run starts (`OnExit(Phase::GameOver)`). What each wave brings is plain data in
//! [`plan`].

pub mod plan;

use crate::chunks::ChunkOffer;
use crate::command_center::CommandCenterFell;
use crate::economy::{EconomySystems, Wallet, wave_bonus};
use crate::grid::Grid;
use crate::horde::{BruteQueue, DeathSystems, Health, SpawnQueue};
use crate::juice::{HitStop, Shake, TimeScale};
use crate::tuning::juice::{WAVE_CLEAR_HIT_STOP, WAVE_CLEAR_SHAKE};
use crate::tuning::waves::{PACK_SECONDS, SPEEDS};
use bevy::prelude::*;
use plan::{Run, Schedule, Wave};

pub struct WavesPlugin;

impl Plugin for WavesPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<Phase>()
            .init_resource::<WaveSettings>()
            .init_resource::<ActiveWave>()
            .init_resource::<GameSpeed>()
            .add_message::<FlowButton>()
            .add_message::<WaveCleared>()
            .add_message::<ClaimFirst>()
            .add_systems(PreStartup, start_run)
            .add_systems(OnEnter(Phase::Wave), (begin_wave, speed_up))
            .add_systems(OnExit(Phase::Wave), slow_down)
            .add_systems(OnEnter(Phase::GameOver), stop_wave)
            .add_systems(OnExit(Phase::GameOver), start_over)
            .add_systems(
                Update,
                (
                    start_wave.run_if(in_state(Phase::Build)),
                    (release_packs, cycle_speed).run_if(in_state(Phase::Wave)),
                    restart.run_if(in_state(Phase::GameOver)),
                ),
            )
            .add_systems(
                Update,
                (end_wave.run_if(in_state(Phase::Wave)), fall)
                    .chain()
                    .in_set(WaveSystems)
                    .after(DeathSystems)
                    .before(EconomySystems),
            );
    }
}

/// Where the game is in the wave loop.
#[derive(States, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Phase {
    /// Building for the next wave, with no timer.
    #[default]
    Build,
    /// The horde is out. No building.
    Wave,
    /// The Command Center fell.
    GameOver,
}

/// Decides when a wave ends and when the run is over. Whatever shows it runs after.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WaveSystems;

/// A HUD button asking for what a key does too.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowButton {
    /// Start the wave (Space).
    StartWave,
    /// Cycle the game speed (Tab).
    Speed,
    /// Start a new run (R).
    Restart,
}

/// The player asked to start the wave while a chunk still has to be claimed.
#[derive(Message, Clone, Copy, Debug)]
pub struct ClaimFirst;

/// Wave `wave` was cleared, and paid `bonus`.
#[derive(Message, Clone, Copy, Debug)]
pub struct WaveCleared {
    pub wave: u32,
    pub bonus: u32,
}

/// Dev overrides for the waves (see [`crate::dev`]): the wave a run starts at, and fixed counts
/// for every wave.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct WaveSettings {
    pub first: Option<u32>,
    pub swarmers: Option<u32>,
    pub brutes: Option<u32>,
}

impl WaveSettings {
    /// What wave `number` brings, with the overrides.
    pub fn wave(&self, number: u32) -> Wave {
        plan::wave(number).with_counts(self.swarmers, self.brutes)
    }
}

/// The wave being fought, or up next in the build phase.
#[derive(Resource, Clone, Copy, Debug)]
pub struct CurrentWave(pub Wave);

/// The packs of the wave being fought that haven't come out yet.
#[derive(Resource, Default, Debug)]
pub struct ActiveWave(pub Schedule);

/// The game speed picked for waves, as an index into [`SPEEDS`]. The build phase always runs at
/// 1×.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameSpeed(pub usize);

impl GameSpeed {
    pub fn factor(self) -> f32 {
        SPEEDS[self.0 % SPEEDS.len()]
    }

    fn next(self) -> Self {
        Self((self.0 + 1) % SPEEDS.len())
    }
}

fn start_run(mut commands: Commands, settings: Res<WaveSettings>) {
    let run = Run::new(settings.first.unwrap_or(1));
    commands.insert_resource(CurrentWave(settings.wave(run.wave)));
    commands.insert_resource(run);
}

fn start_wave(
    keys: Res<ButtonInput<KeyCode>>,
    mut buttons: MessageReader<FlowButton>,
    grid: Res<Grid>,
    offer: Res<ChunkOffer>,
    mut next: ResMut<NextState<Phase>>,
    mut claim_first: MessageWriter<ClaimFirst>,
) {
    let clicked = buttons
        .read()
        .any(|&button| button == FlowButton::StartWave);
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    if !clicked && !(keys.just_pressed(KeyCode::Space) && !ctrl) {
        return;
    }
    if offer.pending(&grid) {
        claim_first.write(ClaimFirst);
    } else if !grid.live_nests().is_empty() {
        next.set(Phase::Wave);
    }
}

fn begin_wave(wave: Res<CurrentWave>, mut active: ResMut<ActiveWave>) {
    active.0 = Schedule::new(plan::packs(&wave.0));
}

/// Lets each pack out of the nests as it comes due.
fn release_packs(
    time: Res<Time>,
    mut active: ResMut<ActiveWave>,
    mut swarmers: ResMut<SpawnQueue>,
    mut brutes: ResMut<BruteQueue>,
) {
    for pack in active.0.advance(time.delta_secs()) {
        swarmers.release(pack.swarmers, PACK_SECONDS);
        brutes.release(pack.brutes);
    }
}

/// The wave is over once everything has come out and died: pay the bonus, and back to building.
fn end_wave(
    active: Res<ActiveWave>,
    swarmers: Res<SpawnQueue>,
    brutes: Res<BruteQueue>,
    enemies: Query<(), With<Health>>,
    grid: Res<Grid>,
    settings: Res<WaveSettings>,
    mut run: ResMut<Run>,
    mut wave: ResMut<CurrentWave>,
    mut wallet: ResMut<Wallet>,
    mut hit_stop: ResMut<HitStop>,
    mut shake: ResMut<Shake>,
    mut cleared: MessageWriter<WaveCleared>,
    mut next: ResMut<NextState<Phase>>,
) {
    let out = !active.0.is_done() || swarmers.waiting > 0 || brutes.waiting > 0;
    // The last of them may have brought the Command Center down with it: that's no clear.
    if out || !enemies.is_empty() || grid.command_center.fallen() {
        return;
    }
    let bonus = wave_bonus(run.wave);
    wallet.earn(bonus);
    cleared.write(WaveCleared {
        wave: run.wave,
        bonus,
    });
    run.clear_wave();
    wave.0 = settings.wave(run.wave);
    // The last kill lands hard: that's the wave done.
    hit_stop.add(WAVE_CLEAR_HIT_STOP);
    shake.add(WAVE_CLEAR_SHAKE);
    next.set(Phase::Build);
}

fn fall(mut fell: MessageReader<CommandCenterFell>, mut next: ResMut<NextState<Phase>>) {
    if fell.read().count() > 0 {
        next.set(Phase::GameOver);
    }
}

/// Nothing more comes out once the Command Center has fallen.
fn stop_wave(mut active: ResMut<ActiveWave>) {
    active.0.clear();
}

fn restart(
    keys: Res<ButtonInput<KeyCode>>,
    mut buttons: MessageReader<FlowButton>,
    mut next: ResMut<NextState<Phase>>,
) {
    let clicked = buttons.read().any(|&button| button == FlowButton::Restart);
    if clicked || keys.just_pressed(KeyCode::KeyR) {
        next.set(Phase::Build);
    }
}

fn start_over(settings: Res<WaveSettings>, mut run: ResMut<Run>, mut wave: ResMut<CurrentWave>) {
    run.restart();
    wave.0 = settings.wave(run.wave);
}

fn cycle_speed(
    keys: Res<ButtonInput<KeyCode>>,
    mut buttons: MessageReader<FlowButton>,
    mut speed: ResMut<GameSpeed>,
    mut scale: ResMut<TimeScale>,
) {
    let clicked = buttons.read().any(|&button| button == FlowButton::Speed);
    if clicked || keys.just_pressed(KeyCode::Tab) {
        *speed = speed.next();
        scale.play = speed.factor();
    }
}

fn speed_up(speed: Res<GameSpeed>, mut scale: ResMut<TimeScale>) {
    scale.play = speed.factor();
}

fn slow_down(mut scale: ResMut<TimeScale>) {
    scale.play = 1.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_speed_cycles() {
        let mut speed = GameSpeed::default();
        let mut seen = vec![speed.factor()];
        for _ in 0..SPEEDS.len() {
            speed = speed.next();
            seen.push(speed.factor());
        }
        assert_eq!(seen, [1.0, 2.0, 3.0, 1.0]);
    }

    #[test]
    fn overrides_change_the_counts_only() {
        let settings = WaveSettings {
            first: None,
            swarmers: Some(5000),
            brutes: None,
        };
        let wave = settings.wave(10);
        assert_eq!(wave.swarmers, 5000);
        assert_eq!(wave.brutes, plan::wave(10).brutes);
        assert_eq!(wave.toughness, plan::wave(10).toughness);
    }
}
