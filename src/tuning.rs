//! All tuning numbers in one place: how the game plays, how the camera moves, and how the
//! feedback feels. The grey-box look (shape sizes, light strength) stays next to the code that
//! draws it, and colors live in [`crate::palette`].
//!
//! Distances are in world units, and one grid cell is one world unit.

/// The map: cells grouped into chunks.
pub mod grid {
    /// Cells along one side of a chunk.
    pub const CHUNK_SIZE: i32 = 16;
    /// The starting map is this many chunks along each side, centered on the origin.
    pub const STARTING_CHUNKS: i32 = 2;
    /// Cells along each side of the Command Center and of a nest.
    pub const COMMAND_CENTER_SIZE: i32 = 2;
    pub const NEST_SIZE: i32 = 2;
}

/// What the player builds: what each thing can take before it breaks.
pub mod structures {
    pub const WALL_HP: u32 = 100;
    pub const AUTOCANNON_HP: u32 = 150;
    pub const FLAMER_HP: u32 = 200;
    pub const MORTAR_HP: u32 = 120;
}

/// Towers: how far they reach, how fast they fire and how hard they hit. Ranges are in cells
/// from the middle of the tower's cell, damage in enemy HP.
pub mod towers {
    /// Autocannon: single target, hitscan. Fires at the enemy in range that's closest to the
    /// Command Center (the lowest flow-field distance).
    pub const AUTOCANNON_RANGE: f32 = 5.5;
    pub const AUTOCANNON_SHOTS_PER_SECOND: f32 = 4.0;
    pub const AUTOCANNON_DAMAGE: f32 = 10.0;
    /// How fast its turret swings (radians per second). It fires only once it's on target.
    pub const AUTOCANNON_TURN_RATE: f32 = 14.0;
    /// Between looks for the best target (this often, in seconds), a tower keeps tracking the
    /// one it has, as long as it's alive and in range. Keeps the cost down in a big horde.
    pub const RETARGET_SECONDS: f32 = 0.1;

    /// Flamer: burns everything in a cone, pointed at the nearest enemy in range.
    pub const FLAMER_RANGE: f32 = 2.5;
    /// The cone's full width (degrees).
    pub const FLAMER_CONE_DEGREES: f32 = 70.0;
    pub const FLAMER_DAMAGE_PER_SECOND: f32 = 25.0;
    pub const FLAMER_TURN_RATE: f32 = 7.0;
    /// The flamer burns in ticks this far apart (seconds), each dealing that much of its damage
    /// per second, rather than every frame.
    pub const FLAMER_TICK_SECONDS: f32 = 0.1;

    /// Mortar: lobs a shell at the biggest clump in range, leading it, and can't hit anything
    /// closer than its minimum range.
    pub const MORTAR_MIN_RANGE: f32 = 3.0;
    pub const MORTAR_RANGE: f32 = 11.0;
    pub const MORTAR_RELOAD_SECONDS: f32 = 3.0;
    pub const MORTAR_FLIGHT_SECONDS: f32 = 1.0;
    pub const MORTAR_TURN_RATE: f32 = 6.0;
    /// Full damage at the middle of the blast, falling off to `MORTAR_EDGE_DAMAGE` (a fraction)
    /// at its edge.
    pub const MORTAR_DAMAGE: f32 = 40.0;
    pub const MORTAR_SPLASH_RADIUS: f32 = 1.25;
    pub const MORTAR_EDGE_DAMAGE: f32 = 0.5;
    /// How many enemies in range it weighs as the middle of a clump when it aims. A brute counts
    /// as this many swarmers.
    pub const MORTAR_CANDIDATES: usize = 24;
    pub const MORTAR_BRUTE_WEIGHT: f32 = 6.0;
}

/// Money: what things cost, what kills pay, and what selling and repairs are worth.
pub mod economy {
    pub const STARTING_MONEY: u32 = 150;
    pub const WALL_PRICE: u32 = 5;
    pub const AUTOCANNON_PRICE: u32 = 40;
    pub const FLAMER_PRICE: u32 = 50;
    pub const MORTAR_PRICE: u32 = 70;
    /// What a kill pays, per enemy class.
    pub const SWARMER_REWARD: u32 = 1;
    pub const BRUTE_REWARD: u32 = 20;
    /// Paid when the whole horde is dead (until milestone 5's waves: after each release).
    pub const WAVE_BONUS: u32 = 25;
    /// Selling something built before the last wave started pays back this fraction of its price,
    /// times the fraction of its HP left. Anything built since sells for its full price (times
    /// its HP left): that's just taking it back.
    pub const COMMITTED_REFUND: f32 = 0.5;
    /// Repairing costs this fraction of the price for the HP put back, as a fraction of the most
    /// it can have. Always cheaper than selling and building a new one.
    pub const REPAIR_FRACTION: f32 = 0.5;
}

/// How the horde finds its way: what each cell costs to walk through, in cells of open ground.
pub mod flow {
    /// A wall is passable, but costs 1 plus its HP left times this many cells of detour, per
    /// enemy class. When a maze makes the way around longer than that, the horde goes for the
    /// wall instead. A fresh wall costs a swarmer 41 cells. Brutes break walls fast, so they'd
    /// rather smash through than walk far. Damaged walls get cheaper, so the horde sticks with the
    /// wall it started on.
    pub const SWARMER_COST_PER_HP: f32 = 0.4;
    pub const BRUTE_COST_PER_HP: f32 = 0.1;
    /// While walls take damage, the flow fields are rebuilt at most this often (seconds). A wall
    /// that breaks, or anything built or removed, rebuilds them right away.
    pub const WEAR_REBUILD_INTERVAL: f32 = 0.25;
}

/// The horde: swarmers pouring out of the nests toward the Command Center.
pub mod horde {
    /// Swarmers released by one press of Space (until the wave loop of milestone 5).
    pub const BATCH: u32 = 100;
    /// Swarmers coming out of each nest per second. Big batches come out faster, so that a
    /// batch is always out within `BATCH_MAX_SECONDS`.
    pub const SPAWN_RATE: f32 = 60.0;
    pub const BATCH_MAX_SECONDS: f32 = 4.0;
    /// Top speed in cells per second, and how much it varies from swarmer to swarmer (a
    /// fraction), so the horde doesn't move in lockstep.
    pub const SWARMER_SPEED: f32 = 3.0;
    pub const SPEED_VARIATION: f32 = 0.12;
    /// How quickly a swarmer turns and gets up to speed (per second, exponential).
    pub const STEERING_RATE: f32 = 10.0;
    /// Half a swarmer's width: they keep this far from walls, and twice this from each other.
    pub const SWARMER_RADIUS: f32 = 0.25;
    /// How much of an overlap between two swarmers is pushed apart each frame (0..1). Lower is
    /// squishier.
    pub const SEPARATION_STIFFNESS: f32 = 0.6;
    /// Each swarmer is pushed apart from at most this many neighbors a frame, so a dense pile
    /// can't make the cost explode.
    pub const MAX_NEIGHBORS: usize = 12;
    /// What it takes to kill each class.
    pub const SWARMER_HP: f32 = 10.0;
    pub const BRUTE_HP: f32 = 300.0;
    /// Damage a swarmer deals when it reaches the Command Center.
    pub const SWARMER_DAMAGE: u32 = 1;
    /// A swarmer bites a wall in its way for this much, this often (seconds): 5 damage a second.
    pub const SWARMER_BITE: u32 = 2;
    pub const SWARMER_BITE_INTERVAL: f32 = 0.4;

    /// Brutes released by one press of B, one every `BRUTE_SPAWN_INTERVAL` seconds.
    pub const BRUTE_BATCH: u32 = 3;
    pub const BRUTE_SPAWN_INTERVAL: f32 = 0.6;
    /// Half a brute's width: 1.5 cells wide, too big for a 1-cell gap.
    pub const BRUTE_RADIUS: f32 = 0.75;
    pub const BRUTE_SPEED: f32 = 1.5;
    /// Brutes are heavy: they turn and get up to speed slower than swarmers.
    pub const BRUTE_STEERING_RATE: f32 = 4.0;
    pub const BRUTE_DAMAGE: u32 = 20;
    /// A brute smashes every wall in its way (up to two, since it needs a 2-wide hole) for this
    /// much, this often (seconds).
    pub const BRUTE_SMASH: u32 = 30;
    pub const BRUTE_SMASH_INTERVAL: f32 = 1.0;

    /// Enemies attack a wall they touch (within this much past touching it) when their way
    /// leads through it: it's no further than this angle (degrees) off where they're heading,
    /// and the flow field goes downhill through it.
    pub const ATTACK_REACH: f32 = 0.06;
    pub const ATTACK_ANGLE: f32 = 60.0;
}

/// The Command Center: what the horde is after.
pub mod command_center {
    pub const MAX_HP: u32 = 1000;
}

/// The overview camera: steep, looking straight up the map (rows stay level on screen).
pub mod camera {
    /// Vertical field of view. Narrow, so there's little perspective distortion.
    pub const FOV_DEGREES: f32 = 30.0;
    /// How far the camera looks down (90 is straight down).
    pub const PITCH_DEGREES: f32 = 62.0;
    /// Room around the map when the camera frames all of it (1.0 is edge to edge).
    pub const FRAMING_MARGIN: f32 = 1.12;
    /// Closest and farthest zoom, as the distance from the camera to the ground point it looks at.
    pub const MIN_DISTANCE: f32 = 10.0;
    pub const MAX_DISTANCE: f32 = 90.0;
    /// How much one scroll notch zooms, as a fraction of the distance.
    pub const ZOOM_STEP: f32 = 0.14;
    /// How quickly the camera eases to a new zoom (per second, exponential).
    pub const ZOOM_EASE_RATE: f32 = 16.0;
    /// Keyboard pan speed, as a fraction of the camera distance per second, so it feels the same
    /// at every zoom.
    pub const PAN_SPEED: f32 = 1.0;
    /// How far past the map's edge the camera may look.
    pub const PAN_OVERSCROLL: f32 = 4.0;
}

/// Feedback. Everything here is about feel, none of it changes the rules.
pub mod juice {
    /// A structure drops in from this high, taking this long (seconds), accelerating all the way.
    pub const DROP_HEIGHT: f32 = 7.0;
    pub const DROP_SECONDS: f32 = 0.13;
    /// When a drag places several walls in one frame, each one lands this much after the previous
    /// one, so a fast drag ripples down the line.
    pub const LINE_STAGGER: f32 = 0.022;
    /// When many walls change at once (e.g. undoing a long line), the stagger shrinks so the
    /// whole ripple takes at most this long.
    pub const MAX_RIPPLE_SECONDS: f32 = 0.3;
    /// Squash on landing (negative is flatter), and screen shake: trauma per landing, and the
    /// most that landings alone can build up.
    pub const LAND_SQUASH: f32 = -0.45;
    pub const LAND_SHAKE: f32 = 0.22;
    pub const LAND_SHAKE_MAX: f32 = 0.38;
    /// Dust kicked up on landing.
    pub const DUST_COUNT: usize = 16;
    pub const DUST_SIZE: f32 = 0.2;
    pub const DUST_SPREAD: f32 = 3.2;
    pub const DUST_LIFT: f32 = 1.3;
    pub const DUST_LIFETIME: f32 = 0.6;
    /// A removed structure pops up and shrinks away, throwing out some rubble.
    pub const REMOVE_SECONDS: f32 = 0.16;
    pub const RUBBLE_COUNT: usize = 7;
    pub const RUBBLE_SIZE: f32 = 0.16;
    pub const RUBBLE_SPREAD: f32 = 1.8;
    pub const RUBBLE_LIFT: f32 = 3.2;
    pub const RUBBLE_LIFETIME: f32 = 0.55;
    /// A click that can't do anything shakes what's in the way (radians), and the hover highlight
    /// flashes for this long (seconds).
    pub const DENIED_WOBBLE: f32 = 0.18;
    pub const DENIED_FLASH_SECONDS: f32 = 0.25;
    /// The hover highlight pops (stretch amount) when it moves to another cell.
    pub const HOVER_POP: f32 = 0.35;

    /// A swarmer rises out of its nest from this deep, growing to full size, over this long.
    pub const EMERGE_DEPTH: f32 = 0.2;
    pub const EMERGE_SECONDS: f32 = 0.25;
    /// Swarmers scuttle: a hop this high, with the walk cycle advancing this much (radians) per
    /// cell walked. A hop is half a cycle.
    pub const SCUTTLE_HOP: f32 = 0.035;
    pub const SCUTTLE_STRIDE: f32 = 10.0;
    /// A nest heaves (stretch amount) as swarmers come out, at most once per interval (seconds).
    pub const NEST_PULSE: f32 = 0.3;
    pub const NEST_PULSE_INTERVAL: f32 = 0.18;
    /// The Command Center flinches (squash), flashes and shakes the camera when it's hit, and
    /// the swarmer bursts into gibs. Only so many bursts a frame, however many arrive at once.
    pub const BASE_HIT_SQUASH: f32 = -0.12;
    pub const BASE_HIT_FLASH_SECONDS: f32 = 0.12;
    pub const BASE_HIT_SHAKE: f32 = 0.06;
    pub const BASE_HIT_SHAKE_MAX: f32 = 0.3;
    pub const GIB_COUNT: usize = 5;
    pub const GIB_SIZE: f32 = 0.08;
    pub const GIB_SPREAD: f32 = 2.0;
    pub const GIB_LIFT: f32 = 2.6;
    pub const GIB_LIFETIME: f32 = 0.45;
    pub const MAX_GIB_BURSTS: usize = 8;
    /// When its HP runs out: a big shake and a long flash.
    pub const BASE_FALL_SHAKE: f32 = 0.8;
    pub const BASE_FALL_FLASH_SECONDS: f32 = 0.6;
    /// A swarmer lunges forward (radians of tilt) when it bites a wall, for this long.
    pub const BITE_LUNGE: f32 = 0.5;
    pub const BITE_SECONDS: f32 = 0.16;
    /// A brute's smash: it rears up (radians) over the last moments before it, slams its head
    /// down (radians) and squashes, bumping the camera.
    pub const SMASH_REAR: f32 = 0.3;
    pub const SMASH_REAR_SECONDS: f32 = 0.35;
    pub const SMASH_DIP: f32 = 0.25;
    pub const SMASH_SECONDS: f32 = 0.25;
    pub const SMASH_SQUASH: f32 = -0.3;
    pub const SMASH_SHAKE: f32 = 0.12;
    pub const SMASH_SHAKE_MAX: f32 = 0.3;
    /// A brute rocks from side to side as it walks (radians), a full rock every this many cells.
    pub const BRUTE_ROCK: f32 = 0.06;
    pub const BRUTE_STRIDE: f32 = 1.2;
    /// A wall that's hit shudders (radians) and chips off a few concrete flecks. Only so many
    /// walls chip a frame, however many are hit.
    pub const WALL_HIT_WOBBLE: f32 = 0.035;
    pub const WALL_SMASH_WOBBLE: f32 = 0.12;
    pub const CHIP_COUNT: usize = 3;
    pub const CHIP_SIZE: f32 = 0.07;
    pub const CHIP_SPREAD: f32 = 1.4;
    pub const CHIP_LIFT: f32 = 2.2;
    pub const CHIP_LIFETIME: f32 = 0.35;
    pub const MAX_CHIP_BURSTS: usize = 6;
    /// Walls look cracked below this fraction of their HP, and battered below the second.
    pub const WALL_CRACKED: f32 = 0.66;
    pub const WALL_BATTERED: f32 = 0.33;
    /// A wall that breaks sinks and crumbles over this long, in a big burst of rubble and dust,
    /// and shakes the camera.
    pub const CRUMBLE_SECONDS: f32 = 0.22;
    pub const BREAK_RUBBLE_COUNT: usize = 14;
    pub const BREAK_DUST_COUNT: usize = 18;
    pub const BREAK_SHAKE: f32 = 0.25;
    pub const BREAK_SHAKE_MAX: f32 = 0.45;

    /// The path preview: arrows this far apart, crawling toward the Command Center at this speed
    /// (cells per second), and flaring up for a moment when the route changes.
    pub const PREVIEW_SPACING: f32 = 0.7;
    pub const PREVIEW_CRAWL: f32 = 1.2;
    pub const PREVIEW_FLARE_SECONDS: f32 = 0.35;
    /// Brutes' arrows are bigger and further apart.
    pub const PREVIEW_BRUTE_SPACING: f32 = 1.3;
    /// Breach markers pulse this many times a second.
    pub const BREACH_PULSE_RATE: f32 = 1.6;

    /// Towers drop in heavier than walls: more dust and a bigger bump.
    pub const TOWER_LAND_SHAKE: f32 = 0.32;
    pub const TOWER_DUST_COUNT: usize = 28;
    /// A repaired structure pops (stretch) and throws off a few sparks.
    pub const REPAIR_POP: f32 = 0.25;
    /// The autocannon: the muzzle flash lasts this long (seconds), the tracer this long, and the
    /// turret kicks back this far (cells). Sparks fly where it hits.
    pub const MUZZLE_FLASH_SECONDS: f32 = 0.05;
    pub const TRACER_SECONDS: f32 = 0.06;
    pub const TRACER_WIDTH: f32 = 0.05;
    pub const AUTOCANNON_KICK: f32 = 0.08;
    pub const SPARK_COUNT: usize = 4;
    /// The flamer: flame particles a second while it burns, how long each lives, and how big.
    pub const FLAME_RATE: f32 = 90.0;
    pub const FLAME_LIFETIME: f32 = 0.35;
    pub const FLAME_SIZE: f32 = 0.16;
    /// The mortar: the tube thumps (squash) with a puff of smoke; the shell flies this high
    /// above the straight line, trailing sparks.
    pub const MORTAR_THUMP: f32 = -0.3;
    pub const MORTAR_KICK: f32 = 0.06;
    pub const SHELL_ARC: f32 = 3.0;
    /// A shell landing: a flash of light, a shock ring as wide as the blast, dirt and embers, and
    /// a capped shake.
    pub const BLAST_FLASH_SECONDS: f32 = 0.18;
    pub const BLAST_LIGHT: f32 = 400_000.0;
    pub const BLAST_RING_SECONDS: f32 = 0.3;
    pub const BLAST_DIRT_COUNT: usize = 14;
    pub const BLAST_EMBER_COUNT: usize = 12;
    pub const BLAST_SHAKE: f32 = 0.18;
    pub const BLAST_SHAKE_MAX: f32 = 0.4;
    /// A badly damaged tower (below `WALL_BATTERED`) puffs smoke this often (seconds).
    pub const TOWER_SMOKE_INTERVAL: f32 = 0.25;

    /// Enemies dying: only so many swarmers burst into gibs a frame, however many die. A brute
    /// flinches (squash) when it's hit, and goes down with a big burst, a heavy shake and a
    /// moment of hit-stop (real seconds).
    pub const MAX_DEATH_BURSTS: usize = 12;
    pub const BRUTE_HIT_SQUASH: f32 = -0.08;
    pub const BRUTE_GIB_COUNT: usize = 22;
    pub const BRUTE_DEATH_SHAKE: f32 = 0.35;
    pub const BRUTE_DEATH_HIT_STOP: f32 = 0.06;

    /// Money: gold pips fly from kills to the counter (at most a few a frame, and so many at
    /// once), taking this long; the counter pulses when they land. A brute's kill pops its
    /// reward over it, rising for this long. The counter flashes and shakes when something
    /// can't be paid for.
    pub const MAX_PIPS_PER_FRAME: usize = 3;
    pub const MAX_PIPS: usize = 40;
    pub const PIP_SECONDS: f32 = 0.55;
    pub const MONEY_PULSE: f32 = 0.12;
    pub const REWARD_POP_SECONDS: f32 = 0.9;
    pub const BROKE_SECONDS: f32 = 0.35;

    /// Fire, sparks, flashes and tracers glow this many times brighter than their color.
    pub const GLOW_BRIGHTNESS: f32 = 2.5;

    /// Longest frame step the springs integrate at once, so a hitch can't make them explode.
    pub const MAX_SPRING_STEP: f32 = 1.0 / 30.0;
    /// Squash & stretch spring.
    pub const SQUASH_STIFFNESS: f32 = 320.0;
    pub const SQUASH_DAMPING: f32 = 13.0;
    /// Wobble: how fast it dies down, how fast it shakes, and how long it lasts at most.
    pub const WOBBLE_DECAY: f32 = 6.0;
    pub const WOBBLE_FREQUENCY: f32 = 32.0;
    pub const WOBBLE_LIFETIME: f32 = 0.7;
    /// Particles fall and bounce on the ground.
    pub const PARTICLE_GRAVITY: f32 = 12.0;
    /// Trauma lost per second, and the camera offset at full trauma, as a fraction of the camera
    /// distance (so a shake looks the same at every zoom).
    pub const SHAKE_DECAY: f32 = 1.8;
    pub const SHAKE_MAX_OFFSET: f32 = 0.018;
    /// How fast the game runs during a hit-stop, relative to normal: nearly frozen.
    pub const HIT_STOP_SPEED: f32 = 0.03;
    /// Longest hit-stop, however many hits pile up (real seconds).
    pub const MAX_HIT_STOP: f32 = 0.2;
}
