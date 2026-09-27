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

/// How the horde finds its way: what each cell costs to walk through, in cells of open ground.
pub mod flow {
    /// A wall is passable, but costs as much as this many cells of detour. When a maze makes the
    /// way around longer than that, the horde goes for the wall instead. A placeholder until
    /// milestone 3 bases it on the wall's HP.
    pub const WALL_COST: f32 = 40.0;
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
    /// Damage a swarmer deals when it reaches the Command Center.
    pub const SWARMER_DAMAGE: u32 = 1;
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
    /// The path preview: arrows this far apart, crawling toward the Command Center at this speed
    /// (cells per second), and flaring up for a moment when the route changes.
    pub const PREVIEW_SPACING: f32 = 0.7;
    pub const PREVIEW_CRAWL: f32 = 1.2;
    pub const PREVIEW_FLARE_SECONDS: f32 = 0.35;

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
