//! An ability's charges, as plain data: one per building that gives it, each reloading on its
//! own once it's used.

/// An ability's charges: how many are ready, and how long each of the others has left to reload.
#[derive(Clone, Debug, PartialEq)]
pub struct Charges {
    cooldown: f32,
    ready: u32,
    /// Seconds left for each charge that's reloading.
    reloading: Vec<f32>,
}

impl Charges {
    /// `sources` charges (one per building that gives the ability), all ready, each taking
    /// `cooldown` seconds to reload once used.
    pub fn new(cooldown: f32, sources: u32) -> Self {
        Self {
            cooldown,
            ready: sources,
            reloading: Vec::new(),
        }
    }

    /// How many charges there are, ready or not: one per building that gives the ability.
    pub fn sources(&self) -> u32 {
        self.ready + self.reloading.len() as u32
    }

    pub fn ready(&self) -> u32 {
        self.ready
    }

    /// Follows the buildings that give the ability. A new one brings a ready charge. Losing one
    /// loses a charge: the one furthest from reloading first, a ready one only when none is
    /// reloading.
    pub fn set_sources(&mut self, sources: u32) {
        while self.sources() < sources {
            self.ready += 1;
        }
        while self.sources() > sources {
            let slowest = self
                .reloading
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.total_cmp(b))
                .map(|(index, _)| index);
            match slowest {
                Some(index) => {
                    self.reloading.swap_remove(index);
                }
                None => self.ready -= 1,
            }
        }
    }

    /// Every charge ready (a new wave).
    pub fn refill(&mut self) {
        self.ready = self.sources();
        self.reloading.clear();
    }

    /// Uses a ready charge, which starts reloading. False if none is ready.
    pub fn use_one(&mut self) -> bool {
        if self.ready == 0 {
            return false;
        }
        self.ready -= 1;
        self.reloading.push(self.cooldown);
        true
    }

    /// Reloads for `dt` seconds. Returns how many charges came back.
    pub fn tick(&mut self, dt: f32) -> u32 {
        for left in &mut self.reloading {
            *left -= dt;
        }
        let before = self.reloading.len();
        self.reloading.retain(|&left| left > 0.0);
        let back = (before - self.reloading.len()) as u32;
        self.ready += back;
        back
    }

    /// Seconds until the next charge comes back, if one is reloading.
    pub fn next_in(&self) -> Option<f32> {
        self.reloading.iter().copied().reduce(f32::min)
    }

    /// How far the next charge has come back (0 just used, 1 ready), if one is reloading.
    pub fn next_progress(&self) -> Option<f32> {
        self.next_in()
            .map(|left| (1.0 - left / self.cooldown.max(1e-6)).clamp(0.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_used_charge_reloads() {
        let mut charges = Charges::new(10.0, 1);
        assert!(charges.use_one());
        assert!(!charges.use_one(), "none left");
        assert_eq!(charges.ready(), 0);
        assert_eq!(charges.next_in(), Some(10.0));
        assert_eq!(charges.tick(4.0), 0);
        assert!((charges.next_progress().unwrap() - 0.4).abs() < 1e-6);
        assert_eq!(charges.tick(6.0), 1, "back");
        assert_eq!(charges.ready(), 1);
        assert_eq!(charges.next_in(), None);
    }

    #[test]
    fn each_charge_reloads_on_its_own() {
        let mut charges = Charges::new(30.0, 2);
        assert!(charges.use_one());
        charges.tick(10.0);
        assert!(charges.use_one());
        assert_eq!(charges.ready(), 0);
        assert_eq!(charges.next_in(), Some(20.0), "the first one used");
        assert_eq!(charges.tick(20.0), 1);
        assert_eq!(charges.next_in(), Some(10.0));
        assert_eq!(charges.tick(10.0), 1);
        assert_eq!(charges.ready(), 2);
    }

    #[test]
    fn losing_a_building_loses_a_reloading_charge_first() {
        let mut charges = Charges::new(30.0, 3);
        charges.use_one();
        charges.tick(20.0);
        charges.use_one();
        // One ready, one with 10 s left, one with 30 s left.
        charges.set_sources(2);
        assert_eq!(charges.ready(), 1);
        assert_eq!(charges.next_in(), Some(10.0), "the slowest one went");
        charges.set_sources(1);
        assert_eq!(charges.ready(), 1);
        assert_eq!(charges.next_in(), None);
        charges.set_sources(0);
        assert_eq!(charges.ready(), 0);
        assert_eq!(charges.sources(), 0);
        assert!(!charges.use_one());
    }

    #[test]
    fn a_new_building_brings_a_ready_charge_and_a_wave_refills() {
        let mut charges = Charges::new(30.0, 0);
        charges.set_sources(1);
        assert_eq!(charges.ready(), 1);
        charges.use_one();
        charges.set_sources(2);
        assert_eq!((charges.ready(), charges.sources()), (1, 2));
        charges.refill();
        assert_eq!(charges.ready(), 2);
        assert_eq!(charges.next_in(), None);
    }
}
