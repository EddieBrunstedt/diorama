//! Tiny deterministic random numbers for procedural visuals.
//!
//! Visuals are seeded from grid coordinates, so the island looks the same every run and a tile's
//! look doesn't change when its neighbors do.

use bevy::math::IVec2;

/// SplitMix64: small, fast and good enough for picking colors and positions.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    /// A generator for one grid cell. `salt` separates independent uses (e.g. surface vs. trees).
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

    pub fn chance(&mut self, probability: f32) -> bool {
        self.f32() < probability
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
