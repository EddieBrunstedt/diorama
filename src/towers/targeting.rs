//! How towers pick targets and deal damage, as plain data. Positions are on the ground plane
//! (world x and z), in cells.

use bevy::prelude::*;

/// Something a tower could shoot at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate<Id> {
    pub id: Id,
    pub at: Vec2,
    /// How far it still has to go to the Command Center (its flow-field distance). Lower is
    /// further along, and more urgent.
    pub progress: f32,
    /// How much it counts in a clump (a brute is worth several swarmers).
    pub weight: f32,
}

/// Whether `at` is between `min` and `max` away from `from`.
pub fn in_range(from: Vec2, at: Vec2, min: f32, max: f32) -> bool {
    let distance = from.distance_squared(at);
    distance >= min * min && distance <= max * max
}

/// The candidate within `range` of `from` that's closest to the Command Center.
pub fn first<Id: Copy>(
    from: Vec2,
    range: f32,
    candidates: impl IntoIterator<Item = Candidate<Id>>,
) -> Option<Candidate<Id>> {
    candidates
        .into_iter()
        .filter(|candidate| in_range(from, candidate.at, 0.0, range))
        .min_by(|a, b| a.progress.total_cmp(&b.progress))
}

/// The candidate within `range` of `from` that's nearest to it.
pub fn nearest<Id: Copy>(
    from: Vec2,
    range: f32,
    candidates: impl IntoIterator<Item = Candidate<Id>>,
) -> Option<Candidate<Id>> {
    candidates
        .into_iter()
        .filter(|candidate| in_range(from, candidate.at, 0.0, range))
        .min_by(|a, b| {
            from.distance_squared(a.at)
                .total_cmp(&from.distance_squared(b.at))
        })
}

/// Whether `at` is inside the cone from `from` along `aim` (unit length), `range` long and
/// `half_angle` radians to each side. Something big reaches in by its `radius`.
pub fn in_cone(from: Vec2, aim: Vec2, half_angle: f32, range: f32, at: Vec2, radius: f32) -> bool {
    let offset = at - from;
    let distance = offset.length();
    if distance > range + radius {
        return false;
    }
    if distance <= radius {
        return true;
    }
    // Widen the cone by the angle the radius covers at that distance.
    let angle = aim.angle_to(offset).abs();
    angle <= half_angle + (radius / distance).min(1.0).asin()
}

/// Turns `aim` (unit length) toward `toward` by at most `rate` radians per second.
pub fn turn_toward(aim: Vec2, toward: Vec2, rate: f32, dt: f32) -> Vec2 {
    let Some(toward) = toward.try_normalize() else {
        return aim;
    };
    let angle = aim.angle_to(toward);
    let step = angle.clamp(-rate * dt, rate * dt);
    Vec2::from_angle(step).rotate(aim).normalize_or(toward)
}

/// Whether `aim` points at `toward` to within `tolerance` radians.
pub fn on_target(aim: Vec2, toward: Vec2, tolerance: f32) -> bool {
    toward
        .try_normalize()
        .is_some_and(|toward| aim.angle_to(toward).abs() <= tolerance)
}

/// A blast's damage `distance` from its middle: full at the middle, down to `edge` (a fraction)
/// at `radius`, nothing beyond. Something big is caught by its `size` (its radius).
pub fn splash(distance: f32, size: f32, radius: f32, damage: f32, edge: f32) -> Option<f32> {
    let distance = (distance - size).max(0.0);
    (distance <= radius).then(|| damage * (1.0 - (1.0 - edge) * distance / radius))
}

/// Where something at `at` moving at `velocity` will be in `seconds`.
pub fn lead(at: Vec2, velocity: Vec2, seconds: f32) -> Vec2 {
    at + velocity * seconds
}

/// `point` moved along the line from `from` until it's between `min` and `max` away.
pub fn clamp_to_range(from: Vec2, point: Vec2, min: f32, max: f32) -> Vec2 {
    let offset = point - from;
    let distance = offset.length();
    if distance < 1e-6 {
        return from + Vec2::Y * min;
    }
    from + offset / distance * distance.clamp(min, max)
}

/// The middle of the biggest clump: tries up to `samples` of the candidates (spread evenly
/// through them) as a blast's middle, and keeps the one whose `weight_near` (the weight of
/// everything within the blast) is highest. Ties go to the one further along.
pub fn best_clump<Id: Copy>(
    candidates: &[Candidate<Id>],
    samples: usize,
    weight_near: impl Fn(Vec2) -> f32,
) -> Option<Candidate<Id>> {
    if candidates.is_empty() || samples == 0 {
        return None;
    }
    let step = (candidates.len() as f32 / samples as f32).max(1.0);
    let mut best: Option<(f32, Candidate<Id>)> = None;
    let mut i = 0.0;
    while (i as usize) < candidates.len() {
        let candidate = candidates[i as usize];
        let score = weight_near(candidate.at);
        let better = best.is_none_or(|(best_score, best)| {
            score > best_score || (score == best_score && candidate.progress < best.progress)
        });
        if better {
            best = Some((score, candidate));
        }
        i += step;
    }
    best.map(|(_, candidate)| candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: u32, x: f32, y: f32, progress: f32) -> Candidate<u32> {
        Candidate {
            id,
            at: Vec2::new(x, y),
            progress,
            weight: 1.0,
        }
    }

    #[test]
    fn first_picks_the_one_closest_to_the_command_center() {
        let candidates = [
            candidate(0, 1.0, 0.0, 10.0),
            candidate(1, 3.0, 0.0, 4.0),
            candidate(2, 9.0, 0.0, 1.0),
        ];
        assert_eq!(first(Vec2::ZERO, 5.0, candidates).map(|c| c.id), Some(1));
        assert_eq!(first(Vec2::ZERO, 20.0, candidates).map(|c| c.id), Some(2));
        assert_eq!(first(Vec2::new(50.0, 0.0), 5.0, candidates), None);
    }

    #[test]
    fn nearest_picks_the_closest_one() {
        let candidates = [
            candidate(0, 2.0, 0.0, 1.0),
            candidate(1, 0.0, -1.5, 9.0),
            candidate(2, 9.0, 0.0, 0.0),
        ];
        assert_eq!(nearest(Vec2::ZERO, 2.5, candidates).map(|c| c.id), Some(1));
        assert_eq!(nearest(Vec2::ZERO, 1.0, candidates), None);
    }

    #[test]
    fn ranges_can_have_a_minimum() {
        assert!(in_range(Vec2::ZERO, Vec2::new(5.0, 0.0), 3.0, 11.0));
        assert!(!in_range(Vec2::ZERO, Vec2::new(2.0, 0.0), 3.0, 11.0));
        assert!(!in_range(Vec2::ZERO, Vec2::new(0.0, 12.0), 3.0, 11.0));
    }

    #[test]
    fn cones() {
        let half = 35f32.to_radians();
        let cone = |at: Vec2, radius| in_cone(Vec2::ZERO, Vec2::X, half, 2.5, at, radius);
        assert!(cone(Vec2::new(2.0, 0.0), 0.0), "dead ahead");
        assert!(cone(Vec2::new(2.0, 1.0), 0.0), "27° off");
        assert!(!cone(Vec2::new(1.0, 1.0), 0.0), "45° off");
        assert!(!cone(Vec2::new(-1.0, 0.0), 0.0), "behind");
        assert!(!cone(Vec2::new(2.6, 0.0), 0.0), "too far");
        // Something big reaches in with its edge.
        assert!(cone(Vec2::new(1.0, 1.0), 0.75));
        assert!(cone(Vec2::new(3.0, 0.0), 0.75));
        assert!(cone(Vec2::new(-0.3, 0.0), 0.75), "on top of the nozzle");
    }

    #[test]
    fn turning_is_limited_per_second() {
        let aim = turn_toward(Vec2::X, Vec2::Y, 1.0, 0.5);
        assert!((aim.to_angle() - 0.5).abs() < 1e-5, "{aim}");
        assert!((aim.length() - 1.0).abs() < 1e-5);
        let aim = turn_toward(Vec2::X, Vec2::new(0.0, -3.0), 10.0, 1.0);
        assert!(aim.distance(Vec2::NEG_Y) < 1e-5, "gets there and stops");
        assert_eq!(turn_toward(Vec2::X, Vec2::ZERO, 1.0, 1.0), Vec2::X);
        assert!(on_target(Vec2::X, Vec2::new(10.0, 0.5), 0.1));
        assert!(!on_target(Vec2::X, Vec2::new(10.0, 3.0), 0.1));
    }

    #[test]
    fn splash_falls_off_toward_the_edge() {
        assert_eq!(splash(0.0, 0.0, 1.25, 40.0, 0.5), Some(40.0));
        assert_eq!(splash(1.25, 0.0, 1.25, 40.0, 0.5), Some(20.0));
        let halfway = splash(0.625, 0.0, 1.25, 40.0, 0.5).unwrap();
        assert!((halfway - 30.0).abs() < 1e-4);
        assert_eq!(splash(1.3, 0.0, 1.25, 40.0, 0.5), None);
        assert_eq!(
            splash(1.3, 0.75, 1.25, 40.0, 0.5).map(f32::round),
            Some(31.0)
        );
    }

    #[test]
    fn shells_lead_and_stay_in_range() {
        assert_eq!(
            lead(Vec2::ZERO, Vec2::new(3.0, 0.0), 0.5),
            Vec2::new(1.5, 0.0)
        );
        let from = Vec2::ZERO;
        assert_eq!(
            clamp_to_range(from, Vec2::new(0.0, 20.0), 3.0, 11.0),
            Vec2::new(0.0, 11.0)
        );
        assert_eq!(
            clamp_to_range(from, Vec2::new(1.0, 0.0), 3.0, 11.0),
            Vec2::new(3.0, 0.0)
        );
        assert_eq!(
            clamp_to_range(from, Vec2::new(5.0, 0.0), 3.0, 11.0),
            Vec2::new(5.0, 0.0)
        );
    }

    #[test]
    fn mortars_aim_at_the_biggest_clump() {
        // A lone one close by, and a pile of five further off.
        let mut candidates = vec![candidate(0, 4.0, 0.0, 1.0)];
        for i in 0..5 {
            candidates.push(candidate(1 + i, 8.0 + i as f32 * 0.2, 0.0, 5.0));
        }
        let weight_near = |at: Vec2| {
            candidates
                .iter()
                .filter(|c| c.at.distance(at) <= 1.25)
                .map(|c| c.weight)
                .sum::<f32>()
        };
        let best = best_clump(&candidates, 24, weight_near).unwrap();
        assert!(best.at.x >= 8.0, "{best:?}");
        // With only a few samples it still finds something.
        assert!(best_clump(&candidates, 2, weight_near).is_some());
        assert_eq!(best_clump::<u32>(&[], 24, |_| 0.0), None);
    }
}
