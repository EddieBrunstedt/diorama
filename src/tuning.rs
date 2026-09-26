//! All gameplay numbers in one place, so they're easy to find and tune.
//!
//! Visual-only constants (colors, model sizes, animation timings) live next to the code that uses
//! them; this file is for numbers that change how the game plays.

/// Trees spawned on each forest tile. Each tree is one chop target.
pub const TREES_PER_FOREST_TILE: usize = 3;

/// The keeper's walking speed in tiles per second.
pub const KEEPER_SPEED: f32 = 1.2;
/// Wood in the backpack at the start.
pub const STARTING_WOOD: u32 = 0;
pub const BACKPACK_CAPACITY: u32 = 5;

/// Seconds to chop down one tree, once the keeper is standing next to it.
pub const CHOP_SECONDS: f32 = 2.0;
pub const WOOD_PER_TREE: u32 = 1;

/// Seconds to build a tower, once the keeper is standing next to the site.
pub const BUILD_SECONDS: f32 = 4.0;
pub const TOWER_WOOD_COST: u32 = 3;

/// Most orders the queue can hold.
pub const MAX_ORDERS: usize = 12;
