//! A tower's hit points, as plain data. Slimes spit goo at towers near the road; a tower with no
//! hit points left is broken and stops shooting until the keeper repairs it.

use crate::tuning::TOWER_HP;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TowerHealth {
    pub hp: u32,
    pub max: u32,
}

impl Default for TowerHealth {
    fn default() -> Self {
        Self {
            hp: TOWER_HP,
            max: TOWER_HP,
        }
    }
}

impl TowerHealth {
    /// Takes a hit. Returns whether that broke the tower.
    pub fn damage(&mut self, amount: u32) -> bool {
        let was_broken = self.is_broken();
        self.hp = self.hp.saturating_sub(amount);
        !was_broken && self.is_broken()
    }

    /// Out of hit points: it doesn't shoot, and slimes leave it alone.
    pub fn is_broken(&self) -> bool {
        self.hp == 0
    }

    /// Hurt at all, so a repair would do something.
    pub fn is_damaged(&self) -> bool {
        self.hp < self.max
    }

    pub fn repair(&mut self) {
        self.hp = self.max;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breaks_once_and_repairs_fully() {
        let mut health = TowerHealth { hp: 2, max: 3 };
        assert!(health.is_damaged());
        assert!(!health.damage(1));
        assert!(health.damage(1), "broken by this hit");
        assert!(health.is_broken());
        assert!(!health.damage(1), "already broken");
        assert_eq!(health.hp, 0);
        health.repair();
        assert_eq!(health, TowerHealth { hp: 3, max: 3 });
        assert!(!health.is_damaged());
    }

    #[test]
    fn towers_start_whole() {
        let health = TowerHealth::default();
        assert!(!health.is_damaged());
        assert!(!health.is_broken());
    }
}
