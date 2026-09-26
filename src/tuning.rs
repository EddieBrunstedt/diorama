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

/// Length of the prepare phase before each wave, in seconds.
pub const PREPARE_SECONDS: f32 = 30.0;
/// Bonus coin per second left on the prepare timer when the bell is rung.
/// (Coin isn't spent on anything yet; the bonus is only shown.)
pub const BELL_COINS_PER_SECOND: f32 = 1.0;

/// The hearth's hit points. The run is over when they're gone.
pub const HEARTH_HP: u32 = 10;
/// Damage a slime does when it reaches the hearth.
pub const SLIME_DAMAGE: u32 = 1;

/// Slimes in wave 1, and how many more each later wave brings.
pub const SLIMES_FIRST_WAVE: u32 = 4;
pub const SLIMES_PER_WAVE: u32 = 2;
/// Slime hit points in wave 1, and how many waves it takes for slimes to gain one more.
pub const SLIME_HP_FIRST_WAVE: u32 = 2;
pub const WAVES_PER_SLIME_HP: u32 = 2;
/// Seconds between slimes in wave 1, how much shorter it gets each wave, and the shortest it gets.
pub const SLIME_INTERVAL_FIRST_WAVE: f32 = 1.5;
pub const SLIME_INTERVAL_PER_WAVE: f32 = 0.08;
pub const SLIME_INTERVAL_MIN: f32 = 0.6;
/// Seconds from the start of a wave until the first slime shows up.
pub const FIRST_SLIME_DELAY: f32 = 1.2;
/// Slime speed along the road, in tiles per second (averaged over its hops).
pub const SLIME_SPEED: f32 = 0.5;

/// How far a tower shoots, in tiles from its middle.
pub const TOWER_RANGE: f32 = 1.8;
/// Seconds between a tower's shots.
pub const TOWER_COOLDOWN: f32 = 0.8;
pub const TOWER_DAMAGE: u32 = 1;
/// Arrow speed in tiles per second.
pub const ARROW_SPEED: f32 = 5.0;
