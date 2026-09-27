//! Money: what building costs, what selling and repairs are worth, and what the horde pays when
//! it dies. The rules are plain data ([`Wallet`], [`change_cost`] and friends); the plugin pays
//! out kill rewards and the wave-clear bonus.
//!
//! Every edit to the map has a cost (negative for a refund), and the undo history keeps it, so
//! taking an edit back gives back exactly what it took (see [`crate::build::history`]).

use crate::flow::Class;
use crate::grid::{Change, Kind, Structure};
use crate::horde::{
    Brute, BruteQueue, DeathSystems, EnemyKilled, HordeReleased, SpawnQueue, Swarmer,
};
use crate::tuning::economy::*;
use bevy::prelude::*;

pub struct EconomyPlugin;

impl Plugin for EconomyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Wallet>()
            .init_resource::<WaveWatch>()
            .add_message::<CantAfford>()
            .add_message::<WaveCleared>()
            .add_systems(
                Update,
                (pay_for_kills, pay_wave_bonus)
                    .chain()
                    .in_set(EconomySystems)
                    .after(DeathSystems),
            );
    }
}

/// Pays out rewards. Whatever shows money coming in runs after it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EconomySystems;

/// Something the player tried couldn't be paid for.
#[derive(Message, Clone, Copy, Debug)]
pub struct CantAfford;

/// The whole horde of the last release is dead, and it paid `bonus`.
#[derive(Message, Clone, Copy, Debug)]
pub struct WaveCleared {
    pub bonus: u32,
}

/// The player's money.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wallet {
    money: u32,
}

impl Default for Wallet {
    fn default() -> Self {
        Self::new(STARTING_MONEY)
    }
}

impl Wallet {
    pub fn new(money: u32) -> Self {
        Self { money }
    }

    pub fn money(self) -> u32 {
        self.money
    }

    /// Whether it holds enough for `cost` (a negative cost is a refund, always fine).
    pub fn can_afford(self, cost: i64) -> bool {
        cost <= self.money as i64
    }

    /// Takes `cost` out (or puts a refund in, if it's negative). False, and nothing changes, if
    /// there isn't enough.
    pub fn pay(&mut self, cost: i64) -> bool {
        if !self.can_afford(cost) {
            return false;
        }
        self.money = (self.money as i64 - cost) as u32;
        true
    }

    pub fn earn(&mut self, amount: u32) {
        self.money = self.money.saturating_add(amount);
    }
}

/// What building one costs.
pub fn price(kind: Kind) -> u32 {
    match kind {
        Kind::Wall => WALL_PRICE,
        Kind::Autocannon => AUTOCANNON_PRICE,
        Kind::Flamer => FLAMER_PRICE,
        Kind::Mortar => MORTAR_PRICE,
    }
}

/// What selling it pays back: its price times the fraction of HP it has left, and only part of
/// that once it's committed (built before the last wave started). Rounded down.
pub fn sell_value(structure: Structure) -> u32 {
    let Structure::Built {
        kind,
        hp,
        committed,
    } = structure
    else {
        return 0;
    };
    let share = if committed { COMMITTED_REFUND } else { 1.0 };
    let left = hp as f64 / kind.max_hp() as f64;
    // A hair of slack, so an exact amount doesn't round down a whole coin.
    (price(kind) as f64 * share as f64 * left + 1e-9).floor() as u32
}

/// What putting `kind`'s HP back up from `from` to `to` costs: part of its price for each HP,
/// rounded up.
pub fn repair_cost(kind: Kind, from: u32, to: u32) -> u32 {
    let per_hp = price(kind) as f64 * REPAIR_FRACTION as f64 / kind.max_hp() as f64;
    (to.saturating_sub(from) as f64 * per_hp - 1e-9)
        .ceil()
        .max(0.0) as u32
}

/// How far `money` repairs `structure`: the HP it ends up with and what that costs. All the way
/// up if there's enough, else as far as the money goes. `None` if it isn't damaged, or the money
/// doesn't cover a single HP.
pub fn affordable_repair(structure: Structure, money: u32) -> Option<(u32, u32)> {
    let kind = structure.kind()?;
    let (hp, max) = structure.health()?;
    if hp >= max {
        return None;
    }
    let full = repair_cost(kind, hp, max);
    if full <= money {
        return Some((max, full));
    }
    let per_hp = price(kind) as f64 * REPAIR_FRACTION as f64 / max as f64;
    let to = (hp + (money as f64 / per_hp + 1e-9).floor() as u32).min(max);
    (to > hp).then(|| (to, repair_cost(kind, hp, to)))
}

/// What a change to the map costs the player (negative when it pays back): building pays the
/// price, removing sells, and putting HP back is a repair.
pub fn change_cost(change: Change) -> i64 {
    match (change.before, change.after) {
        (None, Some(after)) => after.kind().map_or(0, |kind| price(kind) as i64),
        (Some(before), None) => -(sell_value(before) as i64),
        (Some(before), Some(after)) if change.is_repair() => {
            let (Some(kind), Some((from, _)), Some((to, _))) =
                (before.kind(), before.health(), after.health())
            else {
                return 0;
            };
            repair_cost(kind, from, to) as i64
        }
        _ => 0,
    }
}

/// What killing one of `class` pays.
pub fn kill_reward(class: Class) -> u32 {
    match class {
        Class::Swarmer => SWARMER_REWARD,
        Class::Brute => BRUTE_REWARD,
    }
}

fn pay_for_kills(mut kills: MessageReader<EnemyKilled>, mut wallet: ResMut<Wallet>) {
    let earned: u32 = kills.read().map(|kill| kill_reward(kill.class)).sum();
    if earned > 0 {
        wallet.earn(earned);
    }
}

/// Whether a released horde is still out. Until milestone 5's waves, a release (Space or B)
/// stands in for starting a wave, and the wave is cleared once all of it is dead.
#[derive(Resource, Default, Debug)]
struct WaveWatch {
    active: bool,
}

fn pay_wave_bonus(
    mut released: MessageReader<HordeReleased>,
    mut watch: ResMut<WaveWatch>,
    mut wallet: ResMut<Wallet>,
    mut cleared: MessageWriter<WaveCleared>,
    queue: Res<SpawnQueue>,
    brute_queue: Res<BruteQueue>,
    swarmers: Query<(), With<Swarmer>>,
    brutes: Query<(), With<Brute>>,
) {
    if released.read().count() > 0 {
        watch.active = true;
        return;
    }
    let out =
        queue.waiting > 0 || brute_queue.waiting > 0 || !swarmers.is_empty() || !brutes.is_empty();
    if watch.active && !out {
        watch.active = false;
        wallet.earn(WAVE_BONUS);
        cleared.write(WaveCleared { bonus: WAVE_BONUS });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn committed(kind: Kind, hp: u32) -> Structure {
        Structure::Built {
            kind,
            hp,
            committed: true,
        }
    }

    #[test]
    fn wallets_never_go_negative() {
        let mut wallet = Wallet::new(10);
        assert!(wallet.pay(4));
        assert_eq!(wallet.money(), 6);
        assert!(!wallet.pay(7), "not enough");
        assert_eq!(wallet.money(), 6);
        assert!(wallet.pay(-3), "a refund");
        assert_eq!(wallet.money(), 9);
        assert!(wallet.pay(9));
        assert_eq!(wallet.money(), 0);
        wallet.earn(5);
        assert_eq!(wallet.money(), 5);
    }

    #[test]
    fn fresh_structures_sell_for_their_price() {
        for kind in Kind::ALL {
            assert_eq!(sell_value(Structure::new(kind)), price(kind));
        }
        assert_eq!(sell_value(Structure::CommandCenter), 0);
    }

    #[test]
    fn committed_structures_sell_at_a_loss_by_hp_left() {
        let max = Kind::Autocannon.max_hp();
        let price = price(Kind::Autocannon);
        assert_eq!(sell_value(committed(Kind::Autocannon, max)), price / 2);
        assert_eq!(sell_value(committed(Kind::Autocannon, max / 2)), price / 4);
        // A wall at 30%: 0.75, rounded down.
        let wall = Kind::Wall.max_hp();
        assert_eq!(sell_value(committed(Kind::Wall, wall * 3 / 10)), 0);
        // Fresh but damaged (only while the horde is out): its price times the HP left.
        let fresh = Structure::new(Kind::Mortar).with_hp(Kind::Mortar.max_hp() / 2);
        assert_eq!(sell_value(fresh), super::price(Kind::Mortar) / 2);
    }

    #[test]
    fn repairs_cost_part_of_the_price_per_hp() {
        let max = Kind::Autocannon.max_hp();
        let price = price(Kind::Autocannon);
        assert_eq!(repair_cost(Kind::Autocannon, 0, max), price / 2);
        assert_eq!(repair_cost(Kind::Autocannon, max / 2, max), price / 4);
        assert_eq!(repair_cost(Kind::Autocannon, max, max), 0);
        // A wall at 30 of 100: 1.75, rounded up.
        assert_eq!(repair_cost(Kind::Wall, 30, 100), 2);
        assert_eq!(repair_cost(Kind::Wall, 99, 100), 1, "never free");
    }

    #[test]
    fn repairs_beat_selling_and_building_again() {
        for kind in Kind::ALL {
            let max = kind.max_hp();
            for hp in [1, max / 4, max / 2, max - 1] {
                let rebuild = price(kind) - sell_value(committed(kind, hp));
                assert!(repair_cost(kind, hp, max) <= rebuild, "{kind:?} at {hp}");
            }
        }
    }

    #[test]
    fn repairs_go_as_far_as_the_money() {
        let kind = Kind::Autocannon;
        let max = kind.max_hp();
        let worn = committed(kind, max / 2);
        let full = repair_cost(kind, max / 2, max);
        assert_eq!(affordable_repair(worn, 1000), Some((max, full)));
        let (hp, cost) = affordable_repair(worn, full / 2).unwrap();
        assert!(hp > max / 2 && hp < max, "{hp}");
        assert!(cost <= full / 2);
        assert_eq!(affordable_repair(worn, 0), None, "broke");
        assert_eq!(
            affordable_repair(committed(kind, max), 100),
            None,
            "not damaged"
        );
        assert_eq!(affordable_repair(Structure::CommandCenter, 100), None);
    }

    #[test]
    fn changes_cost_what_they_do() {
        let cell = IVec2::ZERO;
        let change = |before, after| Change {
            cell,
            before,
            after,
        };
        let wall = Structure::WALL;
        assert_eq!(change_cost(change(None, Some(wall))), WALL_PRICE as i64);
        assert_eq!(change_cost(change(Some(wall), None)), -(WALL_PRICE as i64));
        let worn = committed(Kind::Flamer, 100);
        let repaired = worn.with_hp(Kind::Flamer.max_hp());
        assert_eq!(
            change_cost(change(Some(worn), Some(repaired))),
            repair_cost(Kind::Flamer, 100, Kind::Flamer.max_hp()) as i64
        );
        assert_eq!(change_cost(change(Some(repaired), Some(worn))), 0);
        assert_eq!(change_cost(change(None, Some(Structure::CommandCenter))), 0);
    }
}
