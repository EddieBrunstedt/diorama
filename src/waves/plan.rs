//! Waves as plain data: what each wave brings ([`wave`]), how it comes out of the nests in packs
//! ([`packs`] and [`Schedule`]), and how a run goes from wave to wave ([`Run`]).
//!
//! Waves follow a fixed plan, not dice: swarmers grow by a fifth a wave, brutes join from wave 4,
//! and every fifth wave is a heavy one (twice the brutes, fewer swarmers), so a layout that held
//! the last wave may not hold the next. Every enemy gets tougher as the waves go by.

use crate::tuning::waves::*;
use bevy::prelude::*;

/// What a wave brings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wave {
    /// Counting from 1.
    pub number: u32,
    pub swarmers: u32,
    pub brutes: u32,
    /// How many packs it comes out in.
    pub packs: u32,
    /// Enemy HP, as a multiple of what they have in wave 1.
    pub toughness: f32,
    /// A heavy wave: more brutes, fewer swarmers.
    pub heavy: bool,
}

impl Wave {
    /// The same wave with other counts (the dev overrides, see [`crate::dev`]).
    pub fn with_counts(self, swarmers: Option<u32>, brutes: Option<u32>) -> Self {
        Self {
            swarmers: swarmers.unwrap_or(self.swarmers),
            brutes: brutes.unwrap_or(self.brutes),
            ..self
        }
    }
}

/// Wave `number` of a run (wave 0 counts as wave 1).
pub fn wave(number: u32) -> Wave {
    let number = number.max(1);
    let heavy = number.is_multiple_of(HEAVY_EVERY);
    let mut swarmers = FIRST_SWARMERS * SWARMER_GROWTH.powi(number as i32 - 1);
    let mut brutes = if number >= BRUTES_FROM_WAVE {
        (number - 2) / WAVES_PER_BRUTE
    } else {
        0
    };
    if heavy {
        swarmers *= HEAVY_SWARMERS;
        brutes *= HEAVY_BRUTES;
    }
    Wave {
        number,
        swarmers: round_to(swarmers, ROUND_TO),
        brutes,
        packs: FIRST_PACKS + number / WAVES_PER_PACK,
        toughness: 1.0 + TOUGHNESS_PER_WAVE * (number - 1) as f32,
        heavy,
    }
}

/// `value` rounded to the nearest multiple of `step`.
fn round_to(value: f64, step: u32) -> u32 {
    let step = step.max(1) as f64;
    ((value / step).round() * step) as u32
}

/// Part of a wave that comes out of the nests together: its brutes lead, and its swarmers pour
/// out over [`PACK_SECONDS`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pack {
    /// Seconds into the wave.
    pub at: f32,
    pub swarmers: u32,
    pub brutes: u32,
}

/// How `wave` comes out: its swarmers split evenly over its packs, [`PACK_INTERVAL`] apart,
/// and its brutes spread over the packs after the first (so the first pack is always swarmers
/// only, unless there's just one pack).
pub fn packs(wave: &Wave) -> Vec<Pack> {
    let count = wave.packs.max(1);
    let brute_packs = if count > 1 { count - 1 } else { 1 };
    (0..count)
        .map(|index| {
            let brutes = if count == 1 {
                wave.brutes
            } else if index == 0 {
                0
            } else {
                share(wave.brutes, brute_packs, index - 1)
            };
            Pack {
                at: index as f32 * PACK_INTERVAL,
                swarmers: share(wave.swarmers, count, index),
                brutes,
            }
        })
        .collect()
}

/// Part `index` of `total` split into `parts` nearly equal parts that add up to it.
fn share(total: u32, parts: u32, index: u32) -> u32 {
    let (total, parts, index) = (total as u64, parts as u64, index as u64);
    ((total * (index + 1)) / parts - (total * index) / parts) as u32
}

/// A wave's packs coming due as it goes on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Schedule {
    packs: Vec<Pack>,
    elapsed: f32,
    /// The first pack not released yet.
    next: usize,
}

impl Schedule {
    pub fn new(packs: Vec<Pack>) -> Self {
        Self {
            packs,
            elapsed: 0.0,
            next: 0,
        }
    }

    /// Moves the wave on by `dt` seconds. Returns the packs that came due (the first one comes
    /// due on the first step).
    pub fn advance(&mut self, dt: f32) -> &[Pack] {
        self.elapsed += dt;
        let from = self.next;
        while let Some(pack) = self.packs.get(self.next)
            && pack.at <= self.elapsed
        {
            self.next += 1;
        }
        &self.packs[from..self.next]
    }

    /// Whether every pack has been released.
    pub fn is_done(&self) -> bool {
        self.next >= self.packs.len()
    }

    /// How many packs have been released, and how many there are.
    pub fn progress(&self) -> (usize, usize) {
        (self.next, self.packs.len())
    }

    /// Stops the wave: nothing more comes out.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// How a run is going: the wave being fought (or up next), how many were cleared, and the best
/// run this session. Score = waves cleared.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    /// The wave a run starts at (1, unless a dev helper says otherwise).
    first: u32,
    /// The wave being fought, or up next in the build phase.
    pub wave: u32,
    /// Waves cleared this run.
    pub cleared: u32,
    /// The most waves cleared in a run this session.
    pub best: u32,
}

impl Run {
    pub fn new(first: u32) -> Self {
        let first = first.max(1);
        Self {
            first,
            wave: first,
            cleared: 0,
            best: 0,
        }
    }

    /// The wave was cleared: on to the next one.
    pub fn clear_wave(&mut self) {
        self.cleared += 1;
        self.wave += 1;
        self.best = self.best.max(self.cleared);
    }

    /// Starts over from the first wave. The best run stays.
    pub fn restart(&mut self) {
        *self = Self {
            best: self.best,
            ..Self::new(self.first)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waves_follow_the_plan() {
        let rows: Vec<(u32, u32, bool)> = [1, 2, 3, 4, 5, 8, 10, 14, 15, 19, 20, 24, 25]
            .map(|number| {
                let wave = wave(number);
                (wave.swarmers, wave.brutes, wave.heavy)
            })
            .into();
        assert_eq!(
            rows,
            vec![
                (100, 0, false),
                (120, 0, false),
                (140, 0, false),
                (170, 1, false),
                (120, 2, true),
                (360, 3, false),
                (310, 8, true),
                (1070, 6, false),
                (770, 12, true),
                (2660, 8, false),
                (1920, 18, true),
                (6620, 11, false),
                (4770, 22, true),
            ]
        );
        assert_eq!(wave(0), wave(1), "there's no wave 0");
    }

    #[test]
    fn waves_get_bigger_and_tougher() {
        for number in 1..40 {
            let (this, next) = (wave(number), wave(number + 1));
            assert!(next.toughness > this.toughness);
            assert!(next.packs >= this.packs);
            // Heavy waves trade swarmers for brutes; otherwise the horde only grows.
            if !this.heavy && !next.heavy {
                assert!(next.swarmers > this.swarmers, "wave {number}");
                assert!(next.brutes >= this.brutes, "wave {number}");
            }
            if next.heavy {
                assert!(next.brutes > this.brutes, "wave {number}");
            }
        }
        assert_eq!(wave(1).toughness, 1.0);
        assert_eq!(wave(1).packs, 2);
        assert_eq!(wave(20).packs, 7);
    }

    #[test]
    fn packs_split_the_wave() {
        for number in [1, 4, 5, 13, 20, 33] {
            let wave = wave(number);
            let packs = packs(&wave);
            assert_eq!(packs.len() as u32, wave.packs);
            assert_eq!(packs.iter().map(|p| p.swarmers).sum::<u32>(), wave.swarmers);
            assert_eq!(packs.iter().map(|p| p.brutes).sum::<u32>(), wave.brutes);
            assert_eq!(packs[0].brutes, 0, "the first pack is swarmers only");
            let (min, max) = packs.iter().fold((u32::MAX, 0), |(min, max), pack| {
                (min.min(pack.swarmers), max.max(pack.swarmers))
            });
            assert!(max - min <= 1, "even packs");
            for (index, pack) in packs.iter().enumerate() {
                assert_eq!(pack.at, index as f32 * PACK_INTERVAL);
            }
        }
        let lone = Wave {
            packs: 1,
            ..wave(6)
        };
        assert_eq!(packs(&lone)[0].brutes, lone.brutes);
    }

    #[test]
    fn schedules_release_packs_on_time() {
        let packs = packs(&wave(4));
        assert_eq!(packs.len(), 3);
        let mut schedule = Schedule::new(packs.clone());
        assert_eq!(
            schedule.advance(1.0),
            &packs[..1],
            "the first comes at once"
        );
        assert!(schedule.advance(PACK_INTERVAL - 2.0).is_empty());
        assert_eq!(schedule.advance(1.0), &packs[1..2]);
        assert!(!schedule.is_done());
        assert!(schedule.advance(0.0).is_empty(), "each pack comes once");
        assert_eq!(schedule.advance(100.0), &packs[2..]);
        assert!(schedule.is_done());
        assert_eq!(schedule.progress(), (3, 3));
        schedule.clear();
        assert!(schedule.is_done(), "nothing left to come");
    }

    #[test]
    fn runs_count_cleared_waves() {
        let mut run = Run::new(1);
        run.clear_wave();
        run.clear_wave();
        assert_eq!((run.wave, run.cleared, run.best), (3, 2, 2));
        run.restart();
        assert_eq!(
            (run.wave, run.cleared, run.best),
            (1, 0, 2),
            "the best stays"
        );
        run.clear_wave();
        assert_eq!(run.best, 2);
        let mut late = Run::new(12);
        late.clear_wave();
        late.restart();
        assert_eq!(
            (late.wave, late.cleared),
            (12, 0),
            "starts over where it started"
        );
    }
}
