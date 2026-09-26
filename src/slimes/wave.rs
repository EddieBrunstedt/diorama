//! What each wave brings and how it's going, as plain data: how many slimes, how tough, how
//! quickly they come, and how many are still around.

use crate::tuning::{
    FIRST_SLIME_DELAY, SLIME_HP_FIRST_WAVE, SLIME_INTERVAL_FIRST_WAVE, SLIME_INTERVAL_MIN,
    SLIME_INTERVAL_PER_WAVE, SLIMES_FIRST_WAVE, SLIMES_PER_WAVE, WAVES_PER_SLIME_HP,
};
use bevy::prelude::*;

/// What a wave brings.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WaveSpec {
    pub count: u32,
    /// Hit points of each slime.
    pub hp: u32,
    /// Seconds between slimes.
    pub interval: f32,
}

impl WaveSpec {
    /// Wave `number` (starting at 1): more, tougher slimes, closer together, every wave.
    pub fn for_wave(number: u32) -> Self {
        let later = number.saturating_sub(1);
        Self {
            count: SLIMES_FIRST_WAVE + SLIMES_PER_WAVE * later,
            hp: SLIME_HP_FIRST_WAVE + later / WAVES_PER_SLIME_HP,
            interval: (SLIME_INTERVAL_FIRST_WAVE - SLIME_INTERVAL_PER_WAVE * later as f32)
                .max(SLIME_INTERVAL_MIN),
        }
    }
}

/// The wave in progress: when the next slime comes and how many are gone (popped or into the
/// hearth).
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct WaveRun {
    pub spec: WaveSpec,
    spawned: u32,
    gone: u32,
    next_in: f32,
}

impl WaveRun {
    pub fn new(spec: WaveSpec) -> Self {
        Self {
            spec,
            spawned: 0,
            gone: 0,
            next_in: FIRST_SLIME_DELAY,
        }
    }

    /// Advances time. Returns how many slimes come out now.
    pub fn tick(&mut self, seconds: f32) -> u32 {
        let mut due = 0;
        self.next_in -= seconds;
        while self.next_in <= 0.0 && self.spawned < self.spec.count {
            self.spawned += 1;
            due += 1;
            self.next_in += self.spec.interval;
        }
        due
    }

    /// One slime is gone.
    pub fn slime_gone(&mut self) {
        self.gone = (self.gone + 1).min(self.spawned);
    }

    /// Slimes still to come or still around.
    pub fn left(&self) -> u32 {
        self.spec.count - self.gone
    }

    /// Every slime has come and gone.
    pub fn cleared(&self) -> bool {
        self.gone >= self.spec.count
    }
}

/// A slime's hit points, and the damage already flying at it, so towers don't waste arrows on a
/// slime that's as good as popped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Health {
    pub hp: u32,
    incoming: u32,
}

impl Health {
    pub fn new(hp: u32) -> Self {
        Self { hp, incoming: 0 }
    }

    /// The arrows on their way will pop it.
    pub fn doomed(&self) -> bool {
        self.incoming >= self.hp
    }

    /// An arrow was shot at it.
    pub fn expect(&mut self, damage: u32) {
        self.incoming += damage;
    }

    /// An arrow hit. Returns whether that popped it.
    pub fn hit(&mut self, damage: u32) -> bool {
        self.incoming = self.incoming.saturating_sub(damage);
        self.hp = self.hp.saturating_sub(damage);
        self.hp == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waves_get_harder() {
        let first = WaveSpec::for_wave(1);
        assert_eq!(first.count, SLIMES_FIRST_WAVE);
        assert_eq!(first.hp, SLIME_HP_FIRST_WAVE);
        assert_eq!(first.interval, SLIME_INTERVAL_FIRST_WAVE);
        let mut previous = first;
        for number in 2..40 {
            let wave = WaveSpec::for_wave(number);
            assert!(wave.count > previous.count);
            assert!(wave.hp >= previous.hp);
            assert!(wave.interval <= previous.interval);
            assert!(wave.interval >= SLIME_INTERVAL_MIN);
            previous = wave;
        }
        assert!(previous.hp > first.hp);
    }

    #[test]
    fn slimes_come_one_interval_apart() {
        let spec = WaveSpec {
            count: 3,
            hp: 1,
            interval: 1.0,
        };
        let mut run = WaveRun::new(spec);
        assert_eq!(run.tick(FIRST_SLIME_DELAY - 0.01), 0);
        assert_eq!(run.tick(0.02), 1);
        assert_eq!(run.tick(0.5), 0);
        // A long frame lets out everything that's due, but never more than the wave has.
        assert_eq!(run.tick(10.0), 2);
        assert_eq!(run.tick(10.0), 0);
    }

    #[test]
    fn cleared_once_every_slime_is_gone() {
        let mut run = WaveRun::new(WaveSpec {
            count: 2,
            hp: 1,
            interval: 1.0,
        });
        assert!(!run.cleared());
        run.slime_gone();
        assert_eq!(run.left(), 2, "none has come out yet");
        run.tick(FIRST_SLIME_DELAY + 1.0);
        run.slime_gone();
        assert_eq!(run.left(), 1);
        assert!(!run.cleared());
        run.slime_gone();
        assert!(run.cleared());
        assert!(WaveRun::new(WaveSpec::default()).cleared(), "an empty wave");
    }

    #[test]
    fn arrows_on_their_way_doom_a_slime() {
        let mut health = Health::new(2);
        health.expect(1);
        assert!(!health.doomed());
        health.expect(1);
        assert!(health.doomed());
        assert!(!health.hit(1));
        assert!(health.doomed(), "one hit left and one arrow still coming");
        assert!(health.hit(1));
        assert_eq!(health.hp, 0);
    }
}
