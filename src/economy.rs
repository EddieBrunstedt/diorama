//! Money: what building costs, what selling and repairs are worth, and what the horde pays when
//! it dies. The rules are plain data ([`Wallet`], [`Purse`], [`change_cost`] and friends); the
//! plugin pays out kill rewards.
//!
//! Every edit to the map has a cost (negative for a refund), and the undo history keeps it, so
//! taking an edit back gives back exactly what it took (see [`crate::build::history`]).
//!
//! Income doesn't grow with the size of the horde: a wave's swarmers share a purse ([`Purse`]),
//! so each is worth less as their numbers grow. Brutes pay a fixed bounty, and clearing a wave
//! pays a bonus ([`wave_bonus`], paid by [`crate::waves`]). Both purse and bonus grow a little
//! with every wave.

use crate::flow::Class;
use crate::grid::{Change, CommandCenterHp, Kind, Structure};
use crate::horde::{DeathSystems, EnemyKilled};
use crate::tuning::economy::*;
use crate::waves::{CurrentWave, Phase};
use bevy::prelude::*;

pub struct EconomyPlugin;

impl Plugin for EconomyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StartingMoney>()
            .init_resource::<Wallet>()
            .init_resource::<Purse>()
            .add_message::<CantAfford>()
            .add_message::<Earned>()
            .add_systems(Startup, fill_wallet)
            .add_systems(OnEnter(Phase::Wave), open_purse)
            .add_systems(OnExit(Phase::GameOver), fill_wallet)
            .add_systems(
                Update,
                pay_for_kills.in_set(EconomySystems).after(DeathSystems),
            );
    }
}

/// Pays out rewards. Whatever shows money coming in runs after it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EconomySystems;

/// Something the player tried couldn't be paid for.
#[derive(Message, Clone, Copy, Debug)]
pub struct CantAfford;

/// A kill of `class` at `at` paid `amount` (only sent when it paid at least a coin).
#[derive(Message, Clone, Copy, Debug)]
pub struct Earned {
    pub at: Vec3,
    pub amount: u32,
    pub class: Class,
}

/// What a run starts with (`DIORAMA_MONEY` overrides it, see [`crate::dev`]).
#[derive(Resource, Clone, Copy, Debug)]
pub struct StartingMoney(pub u32);

impl Default for StartingMoney {
    fn default() -> Self {
        Self(STARTING_MONEY)
    }
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

/// What a wave's swarmers pay: they share a purse, so each one is worth a fraction of a coin once
/// there are many of them. Coins are paid out as the fractions add up.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct Purse {
    per_swarmer: f64,
    /// Fractions of a coin earned but not paid yet.
    owed: f64,
}

impl Purse {
    /// `total` shared by `swarmers`.
    pub fn new(total: f64, swarmers: u32) -> Self {
        Self {
            per_swarmer: if swarmers > 0 {
                total / swarmers as f64
            } else {
                0.0
            },
            owed: 0.0,
        }
    }

    /// A swarmer was killed: the whole coins that pays out now.
    pub fn swarmer_killed(&mut self) -> u32 {
        self.owed += self.per_swarmer;
        // A hair of slack, so the fractions of an exact coin don't fall just short of it.
        let coins = (self.owed + 1e-9).floor();
        self.owed = (self.owed - coins).max(0.0);
        coins as u32
    }
}

/// What killing all of wave `number`'s swarmers pays.
pub fn swarm_purse(number: u32) -> f64 {
    SWARM_PURSE + SWARM_PURSE_PER_WAVE * number.saturating_sub(1) as f64
}

/// What clearing wave `number` pays.
pub fn wave_bonus(number: u32) -> u32 {
    WAVE_BONUS + WAVE_BONUS_PER_WAVE * number.saturating_sub(1)
}

/// What building one costs.
pub fn price(kind: Kind) -> u32 {
    match kind {
        Kind::Wall => WALL_PRICE,
        Kind::Autocannon => AUTOCANNON_PRICE,
        Kind::Flamer => FLAMER_PRICE,
        Kind::Mortar => MORTAR_PRICE,
        Kind::ArtilleryCommand => ARTILLERY_COMMAND_PRICE,
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

/// How far `money` repairs the Command Center: the HP it ends up with and what that costs. All
/// the way up if there's enough, else as far as the money goes. `None` if it isn't damaged, or
/// the money doesn't cover a single HP.
pub fn affordable_command_center_repair(health: CommandCenterHp, money: u32) -> Option<(u32, u32)> {
    if !health.damaged() {
        return None;
    }
    let full = command_center_repair_cost(health.hp, health.max);
    if full <= money {
        return Some((health.max, full));
    }
    let to = (health.hp + (money as f64 / COMMAND_CENTER_REPAIR_PER_HP + 1e-9).floor() as u32)
        .min(health.max);
    // Rounding up can make the last HP cost a coin more than the money: step back until it fits.
    let to = (health.hp..=to)
        .rev()
        .find(|&to| command_center_repair_cost(health.hp, to) <= money)?;
    (to > health.hp).then(|| (to, command_center_repair_cost(health.hp, to)))
}

/// What putting the Command Center's HP back up from `from` to `to` costs, rounded up.
pub fn command_center_repair_cost(from: u32, to: u32) -> u32 {
    (to.saturating_sub(from) as f64 * COMMAND_CENTER_REPAIR_PER_HP - 1e-9)
        .ceil()
        .max(0.0) as u32
}

fn fill_wallet(start: Res<StartingMoney>, mut wallet: ResMut<Wallet>) {
    *wallet = Wallet::new(start.0);
}

fn open_purse(wave: Res<CurrentWave>, mut purse: ResMut<Purse>) {
    *purse = Purse::new(swarm_purse(wave.0.number), wave.0.swarmers);
}

fn pay_for_kills(
    mut kills: MessageReader<EnemyKilled>,
    mut wallet: ResMut<Wallet>,
    mut purse: ResMut<Purse>,
    mut earned: MessageWriter<Earned>,
) {
    for kill in kills.read() {
        let amount = match kill.class {
            Class::Swarmer => purse.swarmer_killed(),
            Class::Brute => BRUTE_REWARD,
        };
        if amount > 0 {
            wallet.earn(amount);
            earned.write(Earned {
                at: kill.at,
                amount,
                class: kill.class,
            });
        }
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
    fn swarmers_share_a_purse() {
        // Wave 1: a coin each.
        let mut purse = Purse::new(100.0, 100);
        assert!((0..100).all(|_| purse.swarmer_killed() == 1));
        // Many swarmers: all of them together still pay the purse.
        let mut purse = Purse::new(290.0, 3190);
        let paid: u32 = (0..3190).map(|_| purse.swarmer_killed()).sum();
        assert_eq!(paid, 290);
        // Fewer than the purse: more than a coin each.
        let mut purse = Purse::new(100.0, 40);
        let paid: u32 = (0..40).map(|_| purse.swarmer_killed()).sum();
        assert_eq!(paid, 100);
        assert_eq!(Purse::new(100.0, 0).swarmer_killed(), 0);
    }

    #[test]
    fn income_grows_slowly_with_the_waves() {
        assert_eq!(swarm_purse(1), SWARM_PURSE);
        assert_eq!(wave_bonus(1), WAVE_BONUS);
        assert_eq!(swarm_purse(20), SWARM_PURSE + 19.0 * SWARM_PURSE_PER_WAVE);
        assert_eq!(wave_bonus(20), WAVE_BONUS + 19 * WAVE_BONUS_PER_WAVE);
    }

    #[test]
    fn command_center_repairs_go_as_far_as_the_money() {
        let worn = CommandCenterHp { hp: 600, max: 1000 };
        let full = command_center_repair_cost(600, 1000);
        assert_eq!(full, (400.0 * COMMAND_CENTER_REPAIR_PER_HP).ceil() as u32);
        assert_eq!(
            affordable_command_center_repair(worn, 10_000),
            Some((1000, full))
        );
        let (to, cost) = affordable_command_center_repair(worn, 7).unwrap();
        assert!(to > 600 && to < 1000 && cost <= 7, "{to} for {cost}");
        assert_eq!(affordable_command_center_repair(worn, 0), None, "broke");
        let whole = CommandCenterHp {
            hp: 1000,
            max: 1000,
        };
        assert_eq!(affordable_command_center_repair(whole, 100), None);
        assert_eq!(command_center_repair_cost(999, 1000), 1, "never free");
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
