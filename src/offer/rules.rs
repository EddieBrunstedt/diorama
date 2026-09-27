//! The tile offer as plain data: which tiles are on offer, and where a tile may go.
//!
//! Tiles go on empty cells touching the island. A road tile has to continue the road from its
//! open end without touching the road anywhere else (no shortcuts), and needs a free side for
//! slimes to come in from. The cell in front of the road's open end is kept for roads, so the
//! road can never be closed off.

use crate::island::{Island, TileKind};
use crate::rng::Rng;
use crate::tuning::{OFFER_SIZE, OFFER_WEIGHT_FOREST, OFFER_WEIGHT_MEADOW, OFFER_WEIGHT_ROAD};
use bevy::prelude::*;

/// The kinds of tile that can be offered, and how likely each is.
const OFFERED: [(TileKind, u32); 3] = [
    (TileKind::Forest, OFFER_WEIGHT_FOREST),
    (TileKind::Meadow, OFFER_WEIGHT_MEADOW),
    (TileKind::Road, OFFER_WEIGHT_ROAD),
];

/// Why a tile can't go on a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocked {
    /// There's a tile there already.
    Taken,
    /// Not touching the island.
    Adrift,
    /// A road has to continue from the road's open end.
    RoadNotConnected,
    /// A road here would also touch the road somewhere else (or the hearth).
    RoadTouches,
    /// A road here would have no free side for slimes to come in from.
    RoadClosedIn,
    /// Only a road can go in front of the road's open end.
    RoadMouth,
}

impl Blocked {
    pub fn message(self) -> &'static str {
        match self {
            Blocked::Taken => "There's a tile here already",
            Blocked::Adrift => "Tiles go on the island's edge",
            Blocked::RoadNotConnected => "Roads continue from the road's open end",
            Blocked::RoadTouches => "Would run into the road",
            Blocked::RoadClosedIn => "No way in for the slimes",
            Blocked::RoadMouth => "Keep the road's end open for slimes",
        }
    }
}

/// Whether a tile of `kind` can go on `cell`.
pub fn check(island: &Island, cell: IVec2, kind: TileKind) -> Result<(), Blocked> {
    if island.get(cell).is_some() {
        return Err(Blocked::Taken);
    }
    if island.neighbors(cell).next().is_none() {
        return Err(Blocked::Adrift);
    }
    if kind != TileKind::Road {
        let mouth = island.road_entry().map(|(end, side)| end + side.offset());
        return if mouth == Some(cell) {
            Err(Blocked::RoadMouth)
        } else {
            Ok(())
        };
    }
    let end = island.road_end().ok_or(Blocked::RoadNotConnected)?;
    if island
        .neighbors(cell)
        .all(|(side, _)| cell + side.offset() != end)
    {
        return Err(Blocked::RoadNotConnected);
    }
    let touches = island.neighbors(cell).any(|(side, kind)| {
        cell + side.offset() != end && matches!(kind, TileKind::Road | TileKind::Hearth)
    });
    if touches {
        return Err(Blocked::RoadTouches);
    }
    let mut placed = island.clone();
    placed.place(cell, kind);
    match placed.road_entry() {
        Some((new_end, _)) if new_end == cell => Ok(()),
        _ => Err(Blocked::RoadClosedIn),
    }
}

/// Every cell a tile of `kind` can go on, row by row.
pub fn valid_cells(island: &Island, kind: TileKind) -> Vec<IVec2> {
    island
        .edge_cells()
        .into_iter()
        .filter(|&cell| check(island, cell, kind).is_ok())
        .collect()
}

/// Rolls the tiles on offer. Only kinds that fit somewhere are offered, and never all the same.
pub fn roll(island: &Island, rng: &mut Rng) -> [TileKind; OFFER_SIZE] {
    let pool: Vec<(TileKind, u32)> = OFFERED
        .into_iter()
        .filter(|&(kind, weight)| weight > 0 && !valid_cells(island, kind).is_empty())
        .collect();
    let mut offer = [TileKind::Meadow; OFFER_SIZE];
    for slot in &mut offer {
        *slot = pick(&pool, rng).unwrap_or(TileKind::Meadow);
    }
    let first = offer[0];
    if offer.iter().all(|&kind| kind == first) {
        let others: Vec<_> = pool
            .into_iter()
            .filter(|&(kind, _)| kind != first)
            .collect();
        if let Some(other) = pick(&others, rng) {
            offer[rng.index(OFFER_SIZE)] = other;
        }
    }
    offer
}

/// A weighted random pick.
fn pick(pool: &[(TileKind, u32)], rng: &mut Rng) -> Option<TileKind> {
    let total: u32 = pool.iter().map(|(_, weight)| weight).sum();
    if total == 0 {
        return None;
    }
    let mut roll = rng.index(total as usize) as u32;
    pool.iter().find_map(|&(kind, weight)| {
        if roll < weight {
            Some(kind)
        } else {
            roll -= weight;
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::island::Side;

    /// The road ends at (0, -1). In front of it is a hole at (0, -2) with tiles all around.
    fn closed_in() -> Island {
        Island::from_map(&[
            "...", //
            ". .", //
            ".=.", //
            ".H.", //
        ])
    }

    #[test]
    fn tiles_go_on_empty_cells_on_the_edge() {
        let island = Island::starting();
        assert_eq!(check(&island, IVec2::new(-3, 0), TileKind::Meadow), Ok(()));
        assert_eq!(
            check(&island, IVec2::new(-1, 0), TileKind::Forest),
            Err(Blocked::Taken)
        );
        assert_eq!(
            check(&island, IVec2::new(-4, 0), TileKind::Forest),
            Err(Blocked::Adrift)
        );
        // Corners only touch diagonally.
        assert_eq!(
            check(&island, IVec2::new(-3, -3), TileKind::Meadow),
            Err(Blocked::Adrift)
        );
    }

    #[test]
    fn the_road_mouth_is_kept_for_roads() {
        let island = Island::starting();
        let mouth = IVec2::new(1, -3);
        assert_eq!(
            check(&island, mouth, TileKind::Meadow),
            Err(Blocked::RoadMouth)
        );
        assert_eq!(check(&island, mouth, TileKind::Road), Ok(()));
        assert_eq!(valid_cells(&island, TileKind::Road), vec![mouth]);
        assert_eq!(
            valid_cells(&island, TileKind::Forest).len(),
            island.edge_cells().len() - 1
        );
    }

    #[test]
    fn roads_continue_from_the_open_end() {
        let island = Island::starting();
        assert_eq!(
            check(&island, IVec2::new(-3, 0), TileKind::Road),
            Err(Blocked::RoadNotConnected)
        );
        // Next to the road, but not at its end.
        assert_eq!(check(&island, IVec2::new(3, -1), TileKind::Meadow), Ok(()));
        assert_eq!(
            check(&island, IVec2::new(3, -1), TileKind::Road),
            Err(Blocked::RoadNotConnected)
        );
    }

    #[test]
    fn roads_can_bend_at_the_end() {
        let island = Island::from_map(&[
            " = ", //
            " H ", //
        ]);
        // The end is (0, -1): straight on, or off to either side.
        assert_eq!(
            valid_cells(&island, TileKind::Road),
            vec![IVec2::new(0, -2), IVec2::new(-1, -1), IVec2::new(1, -1)]
        );
        let mut bent = island.clone();
        bent.place(IVec2::new(1, -1), TileKind::Road);
        assert_eq!(bent.road_end(), Some(IVec2::new(1, -1)));
        assert_eq!(bent.road_entry(), Some((IVec2::new(1, -1), Side::East)));
    }

    #[test]
    fn roads_do_not_touch_the_road_elsewhere() {
        // The road hooks around: hearth → (1, 0) → up to (1, -3) → its end at (0, -3).
        let hook = Island::from_map(&[
            "==", //
            " =", //
            ".=", //
            "H=", //
        ]);
        assert_eq!(hook.road_end(), Some(IVec2::new(0, -3)));
        // South of the end would run alongside the road at (1, -2).
        assert_eq!(
            check(&hook, IVec2::new(0, -2), TileKind::Road),
            Err(Blocked::RoadTouches)
        );
        assert_eq!(check(&hook, IVec2::new(0, -2), TileKind::Meadow), Ok(()));
        assert_eq!(check(&hook, IVec2::new(-1, -3), TileKind::Road), Ok(()));

        // The road bends back toward the hearth: south of its end touches the hearth.
        let back = Island::from_map(&[
            "==", //
            " H", //
        ]);
        assert_eq!(back.road_end(), Some(IVec2::new(-1, -1)));
        assert_eq!(
            check(&back, IVec2::new(-1, 0), TileKind::Road),
            Err(Blocked::RoadTouches)
        );
    }

    #[test]
    fn roads_need_a_way_in() {
        let island = closed_in();
        assert_eq!(
            check(&island, IVec2::new(0, -2), TileKind::Road),
            Err(Blocked::RoadClosedIn)
        );
        assert_eq!(
            check(&island, IVec2::new(0, -2), TileKind::Forest),
            Err(Blocked::RoadMouth)
        );
    }

    #[test]
    fn offers_never_hold_only_one_kind() {
        let island = Island::starting();
        let mut rng = Rng::new(7);
        for _ in 0..500 {
            let offer = roll(&island, &mut rng);
            assert!(offer.iter().any(|&kind| kind != offer[0]), "{offer:?}");
        }
    }

    #[test]
    fn offers_hold_every_kind_now_and_then() {
        let island = Island::starting();
        let mut rng = Rng::new(11);
        let offers: Vec<_> = (0..200).map(|_| roll(&island, &mut rng)).collect();
        for kind in [TileKind::Forest, TileKind::Meadow, TileKind::Road] {
            assert!(offers.iter().any(|offer| offer.contains(&kind)), "{kind:?}");
        }
        assert!(
            offers
                .iter()
                .flatten()
                .all(|&kind| kind != TileKind::Hearth)
        );
    }

    #[test]
    fn no_road_on_offer_when_it_fits_nowhere() {
        let island = closed_in();
        assert!(valid_cells(&island, TileKind::Road).is_empty());
        let mut rng = Rng::new(3);
        for _ in 0..200 {
            assert!(!roll(&island, &mut rng).contains(&TileKind::Road));
        }
    }

    #[test]
    fn same_seed_same_offer() {
        let island = Island::starting();
        let (mut a, mut b) = (Rng::new(5), Rng::new(5));
        assert_eq!(roll(&island, &mut a), roll(&island, &mut b));
    }
}
