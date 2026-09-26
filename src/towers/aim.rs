//! Which slime a tower shoots at, as plain data.

use bevy::prelude::*;

/// A slime a tower could shoot at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate<T> {
    pub id: T,
    /// World x/z.
    pub position: Vec2,
    /// How far along the road it is.
    pub progress: f32,
    /// Arrows already on their way will pop it.
    pub doomed: bool,
}

/// The slime a tower at `from` shoots next: the one furthest down the road (closest to the hearth)
/// within `range`, leaving out slimes that arrows already on their way will pop.
pub fn pick_target<T: Copy>(
    from: Vec2,
    range: f32,
    candidates: impl IntoIterator<Item = Candidate<T>>,
) -> Option<T> {
    candidates
        .into_iter()
        .filter(|candidate| !candidate.doomed && candidate.position.distance(from) <= range)
        .max_by(|a, b| a.progress.total_cmp(&b.progress))
        .map(|candidate| candidate.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slime(id: usize, x: f32, progress: f32, doomed: bool) -> Candidate<usize> {
        Candidate {
            id,
            position: Vec2::new(x, 0.0),
            progress,
            doomed,
        }
    }

    #[test]
    fn shoots_the_slime_closest_to_the_hearth_in_range() {
        let slimes = [
            slime(0, 1.0, 0.5, false),
            slime(1, 1.5, 1.2, false),
            slime(2, 3.0, 2.0, false),
        ];
        assert_eq!(pick_target(Vec2::ZERO, 2.0, slimes), Some(1));
    }

    #[test]
    fn leaves_out_doomed_slimes() {
        let slimes = [slime(0, 1.0, 0.5, false), slime(1, 1.0, 1.2, true)];
        assert_eq!(pick_target(Vec2::ZERO, 2.0, slimes), Some(0));
        assert_eq!(pick_target(Vec2::ZERO, 2.0, [slimes[1]]), None);
    }

    #[test]
    fn nothing_in_range() {
        assert_eq!(
            pick_target(Vec2::ZERO, 2.0, [slime(0, 2.5, 1.0, false)]),
            None
        );
        assert_eq!(pick_target::<usize>(Vec2::ZERO, 2.0, []), None);
    }
}
