//! All gameplay numbers in one place, so they're easy to find and tune.
//!
//! Visual-only constants (colors, model sizes, animation timings) live next to the code that uses
//! them; this file is for numbers that change how the game plays.

/// Trees spawned on each forest tile. Each tree will be one chop target (milestone 2).
pub const TREES_PER_FOREST_TILE: usize = 3;
