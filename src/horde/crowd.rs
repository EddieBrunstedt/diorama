//! The crowd as plain data: a spatial hash for finding neighbors, and the rules the horde moves
//! by. An enemy steers along its flow field, gets pushed apart from the ones it overlaps, and is
//! pushed back out of cells that aren't clear (walls, rock, off the map). One whose way leads
//! through a wall it's touching stops there and attacks it ([`walls_in_the_way`]).
//!
//! Positions are on the ground plane (world x and z), in cells.

use bevy::prelude::*;

/// Points bucketed by position, so finding the ones near a point only looks at a few buckets.
/// Rebuilt from scratch every frame with a counting sort, reusing its memory.
#[derive(Clone, Debug)]
pub struct CrowdIndex<Id> {
    /// The corner of the first bucket, and the size of each (square) bucket.
    origin: Vec2,
    bucket_size: f32,
    /// Buckets along x and y. Points outside are put in the nearest edge bucket.
    buckets: IVec2,
    /// Where each bucket's entries start in `entries`, plus the end of the last one.
    starts: Vec<u32>,
    entries: Vec<(Vec2, Id)>,
    /// Scratch space for `rebuild`: where the next entry of each bucket goes.
    next: Vec<u32>,
}

impl<Id: Copy + PartialEq> CrowdIndex<Id> {
    /// An empty index covering `area`, in buckets of `bucket_size`. Best when a bucket is about as
    /// big as the distance neighbors are looked for at.
    pub fn new(area: Rect, bucket_size: f32) -> Self {
        let buckets = (area.size() / bucket_size)
            .ceil()
            .as_ivec2()
            .max(IVec2::ONE);
        Self {
            origin: area.min,
            bucket_size,
            buckets,
            starts: vec![0; (buckets.x * buckets.y) as usize + 1],
            entries: Vec::new(),
            next: Vec::new(),
        }
    }

    /// Whether the index was made for this area and bucket size.
    pub fn covers(&self, area: Rect, bucket_size: f32) -> bool {
        let same = Self::new(area, bucket_size);
        self.origin == same.origin
            && self.bucket_size == same.bucket_size
            && self.buckets == same.buckets
    }

    fn bucket(&self, point: Vec2) -> IVec2 {
        ((point - self.origin) / self.bucket_size)
            .floor()
            .as_ivec2()
            .clamp(IVec2::ZERO, self.buckets - IVec2::ONE)
    }

    fn bucket_index(&self, bucket: IVec2) -> usize {
        (bucket.y * self.buckets.x + bucket.x) as usize
    }

    /// Replaces the contents with `items`.
    pub fn rebuild(&mut self, items: &[(Vec2, Id)]) {
        // Count per bucket, turn the counts into where each bucket ends, then fill backwards so
        // each bucket's start ends up in place.
        self.starts.fill(0);
        for &(point, _) in items {
            let bucket = self.bucket_index(self.bucket(point));
            self.starts[bucket + 1] += 1;
        }
        for i in 1..self.starts.len() {
            self.starts[i] += self.starts[i - 1];
        }
        self.entries.clear();
        if let Some(&(point, id)) = items.first() {
            self.entries.resize(items.len(), (point, id));
        }
        self.next.clone_from(&self.starts);
        for &(point, id) in items {
            let bucket = self.bucket_index(self.bucket(point));
            self.entries[self.next[bucket] as usize] = (point, id);
            self.next[bucket] += 1;
        }
    }

    /// Everything in the buckets within `radius` of `point`: a superset of what's actually that
    /// close.
    pub fn near(&self, point: Vec2, radius: f32) -> impl Iterator<Item = &(Vec2, Id)> {
        let low = self.bucket(point - Vec2::splat(radius));
        let high = self.bucket(point + Vec2::splat(radius));
        (low.y..=high.y).flat_map(move |y| {
            let first = self.bucket_index(IVec2::new(low.x, y));
            let last = self.bucket_index(IVec2::new(high.x, y));
            let range = self.starts[first] as usize..self.starts[last + 1] as usize;
            self.entries[range].iter()
        })
    }
}

/// How far to move a swarmer at `at` (`me` in the index) so it overlaps its neighbors less.
/// Each overlap closer than `spacing` is pushed apart by half, times `stiffness` (the neighbor
/// takes the other half). At most `max_neighbors` count, so a dense pile stays cheap.
/// `fallback` is the way to go when a neighbor sits exactly on top of it: give each swarmer a
/// different one.
pub fn separation<Id: Copy + PartialEq>(
    index: &CrowdIndex<Id>,
    at: Vec2,
    me: Id,
    spacing: f32,
    stiffness: f32,
    max_neighbors: usize,
    fallback: Vec2,
) -> Vec2 {
    let mut push = Vec2::ZERO;
    let mut neighbors = 0;
    for &(other, id) in index.near(at, spacing) {
        if id == me {
            continue;
        }
        let offset = at - other;
        let squared = offset.length_squared();
        if squared >= spacing * spacing {
            continue;
        }
        let distance = squared.sqrt();
        let away = if distance > 1e-4 {
            offset / distance
        } else {
            fallback
        };
        push += away * (spacing - distance) * 0.5;
        neighbors += 1;
        if neighbors >= max_neighbors {
            break;
        }
    }
    push * stiffness
}

/// Moves a circle of `radius` at `at` out of the cells around it that aren't clear. A circle
/// whose middle ended up inside a blocked cell is moved out through the nearest side.
pub fn push_out(mut at: Vec2, radius: f32, clear: impl Fn(IVec2) -> bool) -> Vec2 {
    let cell = at.floor().as_ivec2();
    if !clear(cell) {
        // Out through the nearest side that leads somewhere clear.
        let local = at - cell.as_vec2();
        let sides = [
            (IVec2::NEG_X, local.x, Vec2::new(-local.x - radius, 0.0)),
            (
                IVec2::X,
                1.0 - local.x,
                Vec2::new(1.0 - local.x + radius, 0.0),
            ),
            (IVec2::NEG_Y, local.y, Vec2::new(0.0, -local.y - radius)),
            (
                IVec2::Y,
                1.0 - local.y,
                Vec2::new(0.0, 1.0 - local.y + radius),
            ),
        ];
        if let Some((_, _, out)) = sides
            .iter()
            .filter(|(step, ..)| clear(cell + *step))
            .min_by(|a, b| a.1.total_cmp(&b.1))
        {
            at += *out;
        }
    }
    // Sides first, then corners, so a circle in a corridor slides along it.
    let cell = at.floor().as_ivec2();
    for step in [
        IVec2::X,
        IVec2::NEG_X,
        IVec2::Y,
        IVec2::NEG_Y,
        IVec2::ONE,
        IVec2::NEG_ONE,
        IVec2::new(1, -1),
        IVec2::new(-1, 1),
    ] {
        let blocked = cell + step;
        if clear(blocked) {
            continue;
        }
        let min = blocked.as_vec2();
        let nearest = at.clamp(min, min + Vec2::ONE);
        let offset = at - nearest;
        let distance = offset.length();
        if distance < radius && distance > 1e-6 {
            at += offset / distance * (radius - distance);
        }
    }
    at
}

/// How far `at` has to move to stop overlapping a heavy neighbor at `other` that doesn't budge
/// (e.g. a swarmer in a brute's way), with `spacing` between their middles.
pub fn shove(at: Vec2, other: Vec2, spacing: f32, fallback: Vec2) -> Vec2 {
    let offset = at - other;
    let distance = offset.length();
    if distance >= spacing {
        return Vec2::ZERO;
    }
    let away = if distance > 1e-4 {
        offset / distance
    } else {
        fallback
    };
    away * (spacing - distance)
}

/// The cells around a circle that it could touch, as long as its radius is under a cell.
const AROUND: [IVec2; 8] = [
    IVec2::X,
    IVec2::NEG_X,
    IVec2::Y,
    IVec2::NEG_Y,
    IVec2::ONE,
    IVec2::NEG_ONE,
    IVec2::new(1, -1),
    IVec2::new(-1, 1),
];

/// The walls a circle of `radius` at `at`, heading along `heading` (unit length), attacks: the
/// cells around it that it touches (up to `reach` past its edge), no more than the angle whose
/// cosine is `alignment` off its heading, and that `breakable` accepts (a wall its way leads
/// through). Each comes with how well it lines up with the heading (1 is dead ahead).
pub fn walls_in_the_way(
    at: Vec2,
    radius: f32,
    reach: f32,
    heading: Vec2,
    alignment: f32,
    breakable: impl Fn(IVec2) -> bool,
) -> impl Iterator<Item = (IVec2, f32)> {
    debug_assert!(
        radius + reach < 1.0,
        "only looks at the cells right around it"
    );
    let cell = at.floor().as_ivec2();
    AROUND.into_iter().filter_map(move |step| {
        let wall = cell + step;
        let min = wall.as_vec2();
        let offset = at.clamp(min, min + Vec2::ONE) - at;
        let distance = offset.length();
        if distance > radius + reach || distance < 1e-6 {
            return None;
        }
        let lined_up = (offset / distance).dot(heading);
        (lined_up >= alignment && breakable(wall)).then_some((wall, lined_up))
    })
}

/// Turns `velocity` toward `desired`, closing the gap at `rate` per second (exponentially).
pub fn steer(velocity: Vec2, desired: Vec2, rate: f32, dt: f32) -> Vec2 {
    velocity.lerp(desired, 1.0 - (-rate * dt).exp())
}

/// Whether a circle of `radius` at `at` touches `area`.
pub fn touches(at: Vec2, radius: f32, area: Rect) -> bool {
    at.distance_squared(at.clamp(area.min, area.max)) <= radius * radius
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect::new(-4.0, -4.0, 4.0, 4.0)
    }

    #[test]
    fn the_index_finds_whats_near() {
        let points: Vec<(Vec2, u32)> = (0..200)
            .map(|i| {
                let angle = i as f32 * 0.7;
                let radius = (i % 17) as f32 * 0.3;
                (Vec2::from_angle(angle) * radius, i)
            })
            .collect();
        let mut index = CrowdIndex::new(area(), 0.5);
        index.rebuild(&points);
        for probe in [Vec2::ZERO, Vec2::new(1.3, -2.2), Vec2::new(3.9, 3.9)] {
            let mut found: Vec<u32> = index
                .near(probe, 0.5)
                .filter(|(point, _)| point.distance(probe) < 0.5)
                .map(|&(_, id)| id)
                .collect();
            let mut expected: Vec<u32> = points
                .iter()
                .filter(|(point, _)| point.distance(probe) < 0.5)
                .map(|&(_, id)| id)
                .collect();
            found.sort();
            expected.sort();
            assert_eq!(found, expected, "near {probe}");
        }
        // Points off the edge still count, in the edge buckets.
        index.rebuild(&[(Vec2::new(9.0, 0.0), 7)]);
        assert_eq!(index.near(Vec2::new(3.9, 0.0), 0.5).count(), 1);
        index.rebuild(&[]);
        assert_eq!(index.near(Vec2::ZERO, 1.0).count(), 0);
    }

    #[test]
    fn overlapping_swarmers_push_apart() {
        let mut index = CrowdIndex::new(area(), 0.5);
        let (a, b) = (Vec2::new(0.0, 0.0), Vec2::new(0.3, 0.0));
        index.rebuild(&[(a, 0u32), (b, 1)]);
        let push_a = separation(&index, a, 0, 0.5, 1.0, 8, Vec2::Y);
        let push_b = separation(&index, b, 1, 0.5, 1.0, 8, Vec2::Y);
        // Each moves half the overlap, away from the other.
        assert!(push_a.distance(Vec2::new(-0.1, 0.0)) < 1e-5, "{push_a}");
        assert!(push_b.distance(Vec2::new(0.1, 0.0)) < 1e-5, "{push_b}");
        // Far enough apart, nothing happens.
        index.rebuild(&[(a, 0), (Vec2::new(0.6, 0.0), 1)]);
        assert_eq!(separation(&index, a, 0, 0.5, 1.0, 8, Vec2::Y), Vec2::ZERO);
        // Exactly on top of each other, they use their fallback.
        index.rebuild(&[(a, 0), (a, 1)]);
        let push = separation(&index, a, 0, 0.5, 1.0, 8, Vec2::X);
        assert!(push.distance(Vec2::new(0.25, 0.0)) < 1e-5);
    }

    #[test]
    fn a_dense_pile_only_counts_some_neighbors() {
        let mut index = CrowdIndex::new(area(), 0.5);
        let pile: Vec<(Vec2, u32)> = (0..50)
            .map(|i| (Vec2::from_angle(i as f32) * 0.1, i))
            .collect();
        index.rebuild(&pile);
        let limited = separation(&index, Vec2::ZERO, 999, 0.5, 1.0, 4, Vec2::X);
        assert!(limited.length() <= 4.0 * 0.25 + 1e-5);
    }

    #[test]
    fn walls_push_circles_back_out() {
        // Only cell (1, 0) is blocked.
        let clear = |cell: IVec2| cell != IVec2::new(1, 0);
        // Touching its left side from cell (0, 0): pushed back left.
        let at = push_out(Vec2::new(0.9, 0.5), 0.25, clear);
        assert!(at.distance(Vec2::new(0.75, 0.5)) < 1e-5, "{at}");
        // Clear of it: untouched.
        assert_eq!(
            push_out(Vec2::new(0.5, 0.5), 0.25, clear),
            Vec2::new(0.5, 0.5)
        );
        // Near its corner, diagonally: pushed away from the corner.
        let at = push_out(Vec2::new(0.9, 1.1), 0.25, clear);
        assert!(at.distance(Vec2::new(1.0, 1.0)) >= 0.25 - 1e-5);
        // Inside it, near its top side: out through the top.
        let at = push_out(Vec2::new(1.5, 0.9), 0.25, clear);
        assert!(at.distance(Vec2::new(1.5, 1.25)) < 1e-5, "{at}");
    }

    #[test]
    fn a_corridor_keeps_circles_inside() {
        // A one-cell corridor along y = 0: everything else is blocked.
        let clear = |cell: IVec2| cell.y == 0;
        for x in [0.2, 0.5, 3.7] {
            for y in [0.05, 0.5, 0.95] {
                let at = push_out(Vec2::new(x, y), 0.25, clear);
                assert!(
                    (0.25 - 1e-5..=0.75 + 1e-5).contains(&at.y),
                    "{x},{y} -> {at}"
                );
                assert!((at.x - x).abs() < 1e-5, "slides along, not back");
            }
        }
    }

    #[test]
    fn heavy_neighbors_shove_all_the_way() {
        let push = shove(Vec2::new(1.0, 0.0), Vec2::ZERO, 1.5, Vec2::Y);
        assert!(push.distance(Vec2::new(0.5, 0.0)) < 1e-5, "{push}");
        assert_eq!(
            shove(Vec2::new(2.0, 0.0), Vec2::ZERO, 1.5, Vec2::Y),
            Vec2::ZERO
        );
        assert_eq!(
            shove(Vec2::ZERO, Vec2::ZERO, 1.5, Vec2::Y),
            Vec2::new(0.0, 1.5)
        );
    }

    fn attacked(at: Vec2, radius: f32, heading: Vec2, walls: &[IVec2]) -> Vec<IVec2> {
        let mut found: Vec<IVec2> =
            walls_in_the_way(at, radius, 0.05, heading, 0.5, |cell| walls.contains(&cell))
                .map(|(cell, _)| cell)
                .collect();
        found.sort_by_key(|cell| (cell.x, cell.y));
        found
    }

    #[test]
    fn attacks_the_wall_in_the_way() {
        let wall = IVec2::new(1, 0);
        // Pressed against its left side and heading into it.
        assert_eq!(
            attacked(Vec2::new(0.75, 0.5), 0.25, Vec2::X, &[wall]),
            vec![wall]
        );
        // Heading along it, or away from it: leaves it alone.
        assert!(attacked(Vec2::new(0.75, 0.5), 0.25, Vec2::Y, &[wall]).is_empty());
        assert!(attacked(Vec2::new(0.75, 0.5), 0.25, Vec2::NEG_X, &[wall]).is_empty());
        // Not touching it.
        assert!(attacked(Vec2::new(0.4, 0.5), 0.25, Vec2::X, &[wall]).is_empty());
        // Touching and heading into it, but it isn't a wall its way leads through.
        assert!(attacked(Vec2::new(0.75, 0.5), 0.25, Vec2::X, &[]).is_empty());
        // Heading at an angle into it still counts.
        let slanted = Vec2::new(1.0, 0.6).normalize();
        assert_eq!(
            attacked(Vec2::new(0.75, 0.5), 0.25, slanted, &[wall]),
            vec![wall]
        );
    }

    #[test]
    fn a_big_circle_attacks_both_walls_it_presses() {
        // A brute's middle on the corner between cells (0, 0) and (1, 0), pressed against the
        // walls at (0, 1) and (1, 1) below them.
        let walls = [IVec2::new(0, 1), IVec2::new(1, 1)];
        let at = Vec2::new(1.0, 1.0 - 0.75);
        assert_eq!(attacked(at, 0.75, Vec2::Y, &walls), walls.to_vec());
        // A step to the side, it still reaches the second one, but only at an angle.
        let at = Vec2::new(0.8, 0.25);
        let lined_up: Vec<(IVec2, f32)> =
            walls_in_the_way(at, 0.75, 0.05, Vec2::Y, 0.5, |cell| walls.contains(&cell)).collect();
        assert_eq!(lined_up.len(), 2, "{lined_up:?}");
    }

    #[test]
    fn steering_eases_toward_the_goal() {
        let velocity = steer(Vec2::ZERO, Vec2::new(3.0, 0.0), 10.0, 0.016);
        assert!(velocity.x > 0.0 && velocity.x < 3.0);
        let settled = (0..200).fold(Vec2::ZERO, |v, _| steer(v, Vec2::Y, 10.0, 0.016));
        assert!(settled.distance(Vec2::Y) < 1e-3);
    }

    #[test]
    fn touching_an_area() {
        let area = Rect::new(-1.0, -1.0, 1.0, 1.0);
        assert!(touches(Vec2::ZERO, 0.25, area));
        assert!(touches(Vec2::new(1.2, 0.0), 0.25, area));
        assert!(!touches(Vec2::new(1.3, 0.0), 0.25, area));
        assert!(
            !touches(Vec2::new(1.2, 1.2), 0.25, area),
            "corners are round"
        );
    }
}
