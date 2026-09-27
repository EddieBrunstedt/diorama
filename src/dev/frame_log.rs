//! `DIORAMA_FRAME_LOG=1` logs, once a second: the average and worst frame, how many enemies
//! are out, and how long the horde's step and the towers' step took (wall-clock time from the
//! first system of the step to the last, so it includes anything running in parallel with it).

use crate::horde::{Brute, HordeSystems, Swarmer};
use crate::towers::TowerSystems;
use bevy::prelude::*;
use std::time::{Duration, Instant};

pub struct FrameLogPlugin;

impl Plugin for FrameLogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FrameLog>().add_systems(
            Update,
            (
                start::<0>.before(HordeSystems),
                stop::<0>.after(HordeSystems).before(TowerSystems),
                start::<1>.after(HordeSystems).before(TowerSystems),
                stop::<1>.after(TowerSystems),
                log_frames.after(TowerSystems),
            ),
        );
    }
}

/// Timings since the last log line.
#[derive(Resource, Default)]
struct FrameLog {
    since: f32,
    frames: u32,
    total: f32,
    worst: f32,
    /// When each step started this frame, and its time summed and at worst since the last line.
    started: [Option<Instant>; 2],
    steps: [Duration; 2],
    worst_steps: [Duration; 2],
}

fn start<const STEP: usize>(mut log: ResMut<FrameLog>) {
    log.started[STEP] = Some(Instant::now());
}

fn stop<const STEP: usize>(mut log: ResMut<FrameLog>) {
    if let Some(started) = log.started[STEP].take() {
        let took = started.elapsed();
        log.steps[STEP] += took;
        log.worst_steps[STEP] = log.worst_steps[STEP].max(took);
    }
}

fn log_frames(
    time: Res<Time<Real>>,
    mut log: ResMut<FrameLog>,
    swarmers: Query<(), With<Swarmer>>,
    brutes: Query<(), With<Brute>>,
) {
    let frame = time.delta_secs();
    if frame <= 0.0 {
        return;
    }
    log.since += frame;
    log.frames += 1;
    log.total += frame;
    log.worst = log.worst.max(frame);
    if log.since < 1.0 {
        return;
    }
    let frames = log.frames as f32;
    let ms = |duration: Duration| duration.as_secs_f32() * 1000.0;
    info!(
        "frames {:.2} ms avg, {:.2} ms worst | {} swarmers, {} brutes | horde {:.2} ms avg ({:.2} worst), towers {:.2} ms avg ({:.2} worst)",
        log.total / frames * 1000.0,
        log.worst * 1000.0,
        swarmers.iter().len(),
        brutes.iter().len(),
        ms(log.steps[0]) / frames,
        ms(log.worst_steps[0]),
        ms(log.steps[1]) / frames,
        ms(log.worst_steps[1]),
    );
    *log = FrameLog::default();
}
