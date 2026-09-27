//! Tiny deterministic random numbers.
//!
//! Cell visuals are seeded from grid coordinates, so the map looks the same every run and a
//! cell's look doesn't change when its neighbors do. Everything else is seeded from [`RunSeed`].

use bevy::{math::IVec2, prelude::Resource};

/// The seed for this run's randomness. `DIORAMA_SEED` fixes it (see [`crate::dev`]), so scripted
/// runs repeat; otherwise it comes from the clock.
#[derive(Resource, Clone, Copy, Debug)]
pub struct RunSeed(pub u64);

impl Default for RunSeed {
    fn default() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos() as u64);
        Self(nanos)
    }
}

/// SplitMix64: small, fast and good enough for colors, positions and rolls.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    /// A generator for one grid cell. `salt` separates independent uses (e.g. color vs. height).
    pub fn for_cell(cell: IVec2, salt: u64) -> Self {
        let packed = ((cell.x as u32 as u64) << 32) | cell.y as u32 as u64;
        Self::new(packed.wrapping_mul(0xD6E8_FEB8_6659_FD93) ^ salt)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }

    /// Uniform integer in `[0, n)`.
    pub fn index(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    pub fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.index(items.len())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_cell_same_sequence() {
        let mut a = Rng::for_cell(IVec2::new(-2, 3), 7);
        let mut b = Rng::for_cell(IVec2::new(-2, 3), 7);
        for _ in 0..10 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn f32_stays_in_unit_range() {
        let mut rng = Rng::new(42);
        for _ in 0..10_000 {
            let x = rng.f32();
            assert!((0.0..1.0).contains(&x));
        }
    }
}
