//! Building as plain data: strokes (a click or a drag, applied live to the grid) and the
//! undo/redo history of finished strokes. Money goes with every change: a stroke only does what
//! the wallet can pay for, and undo gives back exactly what an edit took.

use crate::economy::{
    Wallet, affordable_command_center_repair, affordable_repair, change_cost,
    command_center_repair_cost,
};
use crate::grid::{Change, Grid, Kind, Structure};
use bevy::prelude::*;

/// The cells of a straight line from `start` toward `end`, locked to whichever axis the drag
/// went further along, in order from `start`.
pub fn line(start: IVec2, end: IVec2) -> Vec<IVec2> {
    let delta = end - start;
    let (step, length) = if delta.x.abs() >= delta.y.abs() {
        (IVec2::new(delta.x.signum(), 0), delta.x.abs())
    } else {
        (IVec2::new(0, delta.y.signum()), delta.y.abs())
    };
    (0..=length).map(|i| start + step * i).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokeKind {
    /// Builds `Kind` on every free cell, as long as the money lasts.
    Place(Kind),
    /// Puts HP back on every damaged structure, as far as the money goes.
    Repair,
    /// Sells everything it can.
    Remove,
}

/// Something a stroke did to the map: changed what stands on a cell, or put HP back on the
/// Command Center (whose HP belongs to the whole building, not to a cell).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Cell(Change),
    /// Repaired the Command Center from `from` HP to `to`, through `cell`, one of its cells.
    CommandCenter {
        cell: IVec2,
        from: u32,
        to: u32,
    },
}

impl Action {
    /// The cell it acted on.
    pub fn cell(self) -> IVec2 {
        match self {
            Action::Cell(change) => change.cell,
            Action::CommandCenter { cell, .. } => cell,
        }
    }

    /// The change to a cell, if that's what it was.
    pub fn change(self) -> Option<Change> {
        match self {
            Action::Cell(change) => Some(change),
            Action::CommandCenter { .. } => None,
        }
    }

    /// The action that takes it back.
    pub fn inverse(self) -> Self {
        match self {
            Action::Cell(change) => Action::Cell(change.inverse()),
            Action::CommandCenter { cell, from, to } => Action::CommandCenter {
                cell,
                from: to,
                to: from,
            },
        }
    }

    /// Whether the map still holds what it left behind, so it can be taken back.
    fn holds(self, grid: &Grid) -> bool {
        match self {
            Action::Cell(change) => grid.structure(change.cell) == change.after,
            Action::CommandCenter { to, .. } => grid.command_center.hp == to,
        }
    }

    /// Whether the map holds what it starts from, so it can be done (again).
    fn can_apply(self, grid: &Grid) -> bool {
        self.inverse().holds(grid)
    }

    /// Does it, if the map still holds what it starts from. Returns it if anything changed.
    fn apply(self, grid: &mut Grid) -> Option<Action> {
        match self {
            Action::Cell(change) => grid.apply(change).map(Action::Cell),
            Action::CommandCenter { from, to, .. } => {
                if grid.command_center.hp != from || from == to {
                    return None;
                }
                grid.command_center.hp = to;
                Some(self)
            }
        }
    }
}

/// One thing a stroke did, and what it cost (negative if it paid back).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Entry {
    pub action: Action,
    pub cost: i64,
}

/// A click or drag in progress. It changes the grid live, so feedback is instant, and becomes
/// one undo step when it's finished.
#[derive(Clone, Debug)]
pub struct Stroke {
    pub kind: StrokeKind,
    start: IVec2,
    end: IVec2,
    /// What the stroke has changed so far, in order.
    entries: Vec<Entry>,
    /// Whether it skipped a cell for lack of money since [`Stroke::take_short`] last asked.
    short: bool,
}

impl Stroke {
    /// Starts a stroke at `start`, which already acts on that cell.
    pub fn begin(
        kind: StrokeKind,
        start: IVec2,
        grid: &mut Grid,
        wallet: &mut Wallet,
    ) -> (Self, Vec<Action>) {
        let mut stroke = Self {
            kind,
            start,
            end: start,
            entries: Vec::new(),
            short: false,
        };
        let changes = stroke.update(grid, wallet, start);
        (stroke, changes)
    }

    /// The cells the stroke covers now.
    pub fn cells(&self) -> Vec<IVec2> {
        line(self.start, self.end)
    }

    /// Whether the stroke changed `cell`.
    pub fn changed(&self, cell: IVec2) -> bool {
        self.entries.iter().any(|entry| entry.action.cell() == cell)
    }

    /// What stood on `cell` before the stroke began, given the grid as it is now.
    pub fn structure_before(&self, grid: &Grid, cell: IVec2) -> Option<Structure> {
        let change = self
            .entries
            .iter()
            .filter_map(|entry| entry.action.change())
            .find(|change| change.cell == cell);
        match change {
            Some(change) => change.before,
            None => grid.structure(cell),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether it skipped a cell it couldn't pay for since the last time this was asked.
    pub fn take_short(&mut self) -> bool {
        std::mem::take(&mut self.short)
    }

    /// Moves the stroke's end to `end`: takes back what left the line and acts on the new
    /// cells. Returns what it did to the map, in order.
    pub fn update(&mut self, grid: &mut Grid, wallet: &mut Wallet, end: IVec2) -> Vec<Action> {
        self.end = end;
        let cells = line(self.start, end);
        let mut applied = Vec::new();

        // Newest first, so the grid steps back through the same states it went through.
        let mut kept = Vec::new();
        for entry in self.entries.drain(..).rev() {
            if cells.contains(&entry.action.cell()) {
                kept.push(entry);
            } else if let Some(undone) = take_back(entry, grid, wallet) {
                applied.push(undone);
            } else if entry.action.holds(grid) {
                // Couldn't pay back a sale: it stays part of the stroke.
                kept.push(entry);
            }
        }
        kept.reverse();
        self.entries = kept;

        for cell in cells {
            if self.changed(cell) {
                continue;
            }
            let Some(entry) = self.act_on(grid, wallet.money(), cell) else {
                continue;
            };
            if !wallet.pay(entry.cost) {
                self.short = true;
                continue;
            }
            if let Some(action) = entry.action.apply(grid) {
                self.entries.push(entry);
                applied.push(action);
            }
        }
        applied
    }

    /// What the stroke would do to `cell` with `money` to spend, if anything.
    fn act_on(&mut self, grid: &Grid, money: u32, cell: IVec2) -> Option<Entry> {
        let before = grid.structure(cell);
        if self.kind == StrokeKind::Repair && before == Some(Structure::CommandCenter) {
            return self.repair_command_center(grid, money, cell);
        }
        let after = match self.kind {
            StrokeKind::Place(kind) => {
                grid.check_build(cell).ok()?;
                Some(Structure::new(kind))
            }
            StrokeKind::Repair => {
                let structure = before.filter(|structure| structure.damaged())?;
                match affordable_repair(structure, money) {
                    Some((hp, _)) => Some(structure.with_hp(hp)),
                    None => {
                        self.short = true;
                        return None;
                    }
                }
            }
            StrokeKind::Remove => {
                before.filter(|structure| structure.removable())?;
                None
            }
        };
        let change = Change {
            cell,
            before,
            after,
        };
        Some(Entry {
            action: Action::Cell(change),
            cost: change_cost(change),
        })
    }

    /// Repairs the Command Center through `cell` (once a stroke, however many of its cells the
    /// line crosses), as far as `money` goes.
    fn repair_command_center(&mut self, grid: &Grid, money: u32, cell: IVec2) -> Option<Entry> {
        let health = grid.command_center;
        let repaired = self
            .entries
            .iter()
            .any(|entry| matches!(entry.action, Action::CommandCenter { .. }));
        if repaired || !health.damaged() {
            return None;
        }
        let Some((to, cost)) = affordable_command_center_repair(health, money) else {
            self.short = true;
            return None;
        };
        debug_assert_eq!(cost, command_center_repair_cost(health.hp, to));
        Some(Entry {
            action: Action::CommandCenter {
                cell,
                from: health.hp,
                to,
            },
            cost: cost as i64,
        })
    }

    /// Takes back everything the stroke did. Returns what that did to the map.
    pub fn cancel(mut self, grid: &mut Grid, wallet: &mut Wallet) -> Vec<Action> {
        self.entries
            .drain(..)
            .rev()
            .filter_map(|entry| take_back(entry, grid, wallet))
            .collect()
    }

    /// The stroke as one undo step.
    pub fn finish(self) -> Edit {
        Edit {
            entries: self.entries,
        }
    }
}

/// Undoes one entry, giving back what it cost (or paying back what it paid), if the map still
/// holds what it left there and the money allows.
fn take_back(entry: Entry, grid: &mut Grid, wallet: &mut Wallet) -> Option<Action> {
    if !entry.action.holds(grid) || !wallet.pay(-entry.cost) {
        return None;
    }
    entry.action.inverse().apply(grid)
}

/// One undo step: the changes it made, in order, with their costs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Edit {
    entries: Vec<Entry>,
}

impl Edit {
    /// What undoing it costs (negative when it pays back), counting only the changes that still
    /// hold on the grid.
    fn undo_cost(&self, grid: &Grid) -> i64 {
        self.entries
            .iter()
            .filter(|entry| entry.action.holds(grid))
            .map(|entry| -entry.cost)
            .sum()
    }

    /// What doing it again costs, counting only the changes that can still be made.
    fn redo_cost(&self, grid: &Grid) -> i64 {
        self.entries
            .iter()
            .filter(|entry| entry.action.can_apply(grid))
            .map(|entry| entry.cost)
            .sum()
    }
}

/// Undo and redo couldn't happen: taking the edit back (or doing it again) costs more money than
/// there is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TooExpensive;

/// Undo and redo stacks for the build phase.
#[derive(Resource, Clone, Debug, Default)]
pub struct History {
    undo: Vec<Edit>,
    redo: Vec<Edit>,
}

impl History {
    /// Adds a finished edit. Anything that was undone can't be redone after this.
    pub fn record(&mut self, edit: Edit) {
        if edit.entries.is_empty() {
            return;
        }
        self.undo.push(edit);
        self.redo.clear();
    }

    /// Takes back the last edit, with its money. Returns what that did to the map (empty if
    /// there was nothing to undo). Changes that no longer hold (e.g. a wall the horde broke since)
    /// are skipped, and so is their money. If the edit can't be paid for, nothing happens.
    pub fn undo(
        &mut self,
        grid: &mut Grid,
        wallet: &mut Wallet,
    ) -> Result<Vec<Action>, TooExpensive> {
        let Some(edit) = self.undo.last() else {
            return Ok(Vec::new());
        };
        if !wallet.pay(edit.undo_cost(grid)) {
            return Err(TooExpensive);
        }
        let edit = self.undo.pop().expect("checked above");
        let applied = edit
            .entries
            .iter()
            .rev()
            .filter_map(|entry| entry.action.inverse().apply(grid))
            .collect();
        self.redo.push(edit);
        Ok(applied)
    }

    /// Does the last undone edit again, with its money. Returns what that did to the map.
    pub fn redo(
        &mut self,
        grid: &mut Grid,
        wallet: &mut Wallet,
    ) -> Result<Vec<Action>, TooExpensive> {
        let Some(edit) = self.redo.last() else {
            return Ok(Vec::new());
        };
        if !wallet.pay(edit.redo_cost(grid)) {
            return Err(TooExpensive);
        }
        let edit = self.redo.pop().expect("checked above");
        let applied = edit
            .entries
            .iter()
            .filter_map(|entry| entry.action.apply(grid))
            .collect();
        self.undo.push(edit);
        Ok(applied)
    }

    /// Forgets everything: what was built can't be taken back any more (a wave started).
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{Chunk, Terrain};

    const WALL: StrokeKind = StrokeKind::Place(Kind::Wall);

    fn rich() -> Wallet {
        Wallet::new(10_000)
    }

    fn grid() -> Grid {
        let mut grid = Grid::default();
        grid.insert_chunk(IVec2::ZERO, Chunk::new(Terrain::Ground));
        grid
    }

    fn walls(grid: &Grid) -> Vec<IVec2> {
        let mut cells: Vec<IVec2> = (0..16)
            .flat_map(|y| (0..16).map(move |x| IVec2::new(x, y)))
            .filter(|&cell| grid.structure(cell).is_some())
            .collect();
        cells.sort_by_key(|cell| (cell.y, cell.x));
        cells
    }

    fn cells(list: &[(i32, i32)]) -> Vec<IVec2> {
        list.iter().map(|&(x, y)| IVec2::new(x, y)).collect()
    }

    #[test]
    fn lines_lock_to_the_longer_axis() {
        let at = |x, y| IVec2::new(x, y);
        assert_eq!(line(at(2, 2), at(2, 2)), vec![at(2, 2)]);
        assert_eq!(
            line(at(2, 2), at(5, 3)),
            cells(&[(2, 2), (3, 2), (4, 2), (5, 2)])
        );
        assert_eq!(
            line(at(2, 2), at(1, -1)),
            cells(&[(2, 2), (2, 1), (2, 0), (2, -1)])
        );
        // A tie goes horizontal.
        assert_eq!(
            line(at(0, 0), at(-2, 2)),
            cells(&[(0, 0), (-1, 0), (-2, 0)])
        );
    }

    #[test]
    fn a_click_places_one_wall() {
        let mut grid = grid();
        let mut wallet = rich();
        let (stroke, changes) = Stroke::begin(WALL, IVec2::new(3, 4), &mut grid, &mut wallet);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change().unwrap().after, Some(Structure::WALL));
        assert!(!stroke.is_empty());
        assert_eq!(walls(&grid), cells(&[(3, 4)]));
    }

    #[test]
    fn dragging_places_a_line_around_blocked_cells() {
        let mut grid = grid();
        let mut wallet = rich();
        grid.set_terrain(IVec2::new(3, 1), Terrain::Rock);
        grid.set_structure(IVec2::new(5, 1), Some(Structure::WALL));
        let (mut stroke, _) = Stroke::begin(WALL, IVec2::new(1, 1), &mut grid, &mut wallet);
        let changes = stroke.update(&mut grid, &mut wallet, IVec2::new(6, 2));
        let placed: Vec<IVec2> = changes.iter().map(|action| action.cell()).collect();
        assert_eq!(
            placed,
            cells(&[(2, 1), (4, 1), (6, 1)]),
            "in order, skipping blocked"
        );
        assert_eq!(
            walls(&grid),
            cells(&[(1, 1), (2, 1), (4, 1), (5, 1), (6, 1)])
        );
    }

    #[test]
    fn shrinking_or_turning_a_drag_takes_walls_back() {
        let mut grid = grid();
        let mut wallet = rich();
        let (mut stroke, _) = Stroke::begin(WALL, IVec2::new(1, 1), &mut grid, &mut wallet);
        stroke.update(&mut grid, &mut wallet, IVec2::new(4, 1));
        let changes = stroke.update(&mut grid, &mut wallet, IVec2::new(2, 1));
        assert_eq!(changes.len(), 2);
        assert!(changes.iter().all(|a| a.change().unwrap().after.is_none()));
        assert_eq!(changes[0].cell(), IVec2::new(4, 1), "newest first");
        assert_eq!(walls(&grid), cells(&[(1, 1), (2, 1)]));

        stroke.update(&mut grid, &mut wallet, IVec2::new(1, 3));
        assert_eq!(walls(&grid), cells(&[(1, 1), (1, 2), (1, 3)]));
        assert_eq!(stroke.cells(), cells(&[(1, 1), (1, 2), (1, 3)]));
    }

    #[test]
    fn a_drag_never_removes_walls_it_did_not_place() {
        let mut grid = grid();
        let mut wallet = rich();
        grid.set_structure(IVec2::new(3, 1), Some(Structure::WALL));
        let (mut stroke, _) = Stroke::begin(WALL, IVec2::new(1, 1), &mut grid, &mut wallet);
        stroke.update(&mut grid, &mut wallet, IVec2::new(4, 1));
        stroke.update(&mut grid, &mut wallet, IVec2::new(1, 1));
        assert_eq!(walls(&grid), cells(&[(1, 1), (3, 1)]));
    }

    #[test]
    fn remove_strokes_clear_walls() {
        let mut grid = grid();
        let mut wallet = rich();
        for x in 0..5 {
            grid.set_structure(IVec2::new(x, 0), Some(Structure::WALL));
        }
        let (mut stroke, changes) =
            Stroke::begin(StrokeKind::Remove, IVec2::new(1, 0), &mut grid, &mut wallet);
        assert_eq!(changes.len(), 1);
        stroke.update(&mut grid, &mut wallet, IVec2::new(3, 0));
        assert_eq!(walls(&grid), cells(&[(0, 0), (4, 0)]));
        // Going back puts them up again.
        stroke.update(&mut grid, &mut wallet, IVec2::new(2, 0));
        assert_eq!(walls(&grid), cells(&[(0, 0), (3, 0), (4, 0)]));
    }

    #[test]
    fn the_command_center_cant_be_removed() {
        let mut grid = grid();
        let mut wallet = rich();
        let base = IVec2::new(2, 0);
        grid.set_structure(base, Some(Structure::CommandCenter));
        grid.set_structure(IVec2::new(3, 0), Some(Structure::WALL));
        let (mut stroke, changes) = Stroke::begin(StrokeKind::Remove, base, &mut grid, &mut wallet);
        assert!(changes.is_empty() && stroke.is_empty());
        // A line across it removes the wall next to it, and leaves the Command Center standing.
        stroke.update(&mut grid, &mut wallet, IVec2::new(4, 0));
        assert_eq!(grid.structure(base), Some(Structure::CommandCenter));
        assert_eq!(grid.structure(IVec2::new(3, 0)), None);
    }

    #[test]
    fn a_stroke_remembers_what_stood_before_it() {
        let mut grid = grid();
        let mut wallet = rich();
        grid.set_structure(IVec2::new(2, 0), Some(Structure::WALL));
        let (mut stroke, _) = Stroke::begin(WALL, IVec2::new(0, 0), &mut grid, &mut wallet);
        stroke.update(&mut grid, &mut wallet, IVec2::new(3, 0));
        let before = |x| stroke.structure_before(&grid, IVec2::new(x, 0));
        assert_eq!(before(0), None, "placed by the stroke");
        assert_eq!(before(2), Some(Structure::WALL), "already there");
        assert_eq!(before(5), None, "untouched");

        let (stroke, _) =
            Stroke::begin(StrokeKind::Remove, IVec2::new(1, 0), &mut grid, &mut wallet);
        assert_eq!(grid.structure(IVec2::new(1, 0)), None);
        assert_eq!(
            stroke.structure_before(&grid, IVec2::new(1, 0)),
            Some(Structure::WALL),
            "removed by the stroke"
        );
    }

    #[test]
    fn a_click_on_nothing_is_an_empty_stroke() {
        let mut grid = grid();
        let mut wallet = rich();
        grid.set_terrain(IVec2::new(2, 2), Terrain::Rock);
        let (stroke, changes) = Stroke::begin(WALL, IVec2::new(2, 2), &mut grid, &mut wallet);
        assert!(changes.is_empty() && stroke.is_empty());
        let (stroke, _) =
            Stroke::begin(StrokeKind::Remove, IVec2::new(0, 0), &mut grid, &mut wallet);
        assert!(stroke.is_empty());
        let (stroke, _) = Stroke::begin(WALL, IVec2::new(-5, 0), &mut grid, &mut wallet);
        assert!(stroke.is_empty(), "outside the map");
    }

    #[test]
    fn cancelling_restores_the_grid() {
        let mut grid = grid();
        let mut wallet = rich();
        grid.set_structure(IVec2::new(0, 5), Some(Structure::WALL));
        let before = grid.clone();
        let (mut stroke, _) = Stroke::begin(WALL, IVec2::new(0, 0), &mut grid, &mut wallet);
        stroke.update(&mut grid, &mut wallet, IVec2::new(0, 8));
        let changes = stroke.cancel(&mut grid, &mut wallet);
        assert_eq!(changes.len(), 8);
        assert_eq!(grid, before);
    }

    #[test]
    fn undo_and_redo_whole_strokes() {
        let mut grid = grid();
        let mut wallet = rich();
        let mut history = History::default();
        let empty = grid.clone();

        let (mut stroke, _) = Stroke::begin(WALL, IVec2::new(0, 0), &mut grid, &mut wallet);
        stroke.update(&mut grid, &mut wallet, IVec2::new(3, 0));
        history.record(stroke.finish());
        let one_line = grid.clone();

        let (stroke, _) =
            Stroke::begin(StrokeKind::Remove, IVec2::new(1, 0), &mut grid, &mut wallet);
        history.record(stroke.finish());
        assert_eq!(walls(&grid), cells(&[(0, 0), (2, 0), (3, 0)]));

        assert_eq!(history.undo(&mut grid, &mut wallet).unwrap().len(), 1);
        assert_eq!(grid, one_line);
        let undone = history.undo(&mut grid, &mut wallet).unwrap();
        assert_eq!(undone.len(), 4);
        assert_eq!(undone[0].cell(), IVec2::new(3, 0), "newest first");
        assert_eq!(grid, empty);
        assert!(!history.can_undo());
        assert!(history.undo(&mut grid, &mut wallet).unwrap().is_empty());

        assert_eq!(history.redo(&mut grid, &mut wallet).unwrap().len(), 4);
        assert_eq!(grid, one_line);
        assert!(history.can_redo());
    }

    #[test]
    fn a_new_edit_drops_the_redo_stack() {
        let mut grid = grid();
        let mut wallet = rich();
        let mut history = History::default();
        let (stroke, _) = Stroke::begin(WALL, IVec2::new(0, 0), &mut grid, &mut wallet);
        history.record(stroke.finish());
        history.undo(&mut grid, &mut wallet).unwrap();
        assert!(history.can_redo());

        // An empty stroke isn't an edit, so it keeps the redo.
        grid.set_terrain(IVec2::new(9, 9), Terrain::Rock);
        let (stroke, _) = Stroke::begin(WALL, IVec2::new(9, 9), &mut grid, &mut wallet);
        history.record(stroke.finish());
        assert!(history.can_redo());

        let (stroke, _) = Stroke::begin(WALL, IVec2::new(5, 5), &mut grid, &mut wallet);
        history.record(stroke.finish());
        assert!(!history.can_redo());
        assert!(history.redo(&mut grid, &mut wallet).unwrap().is_empty());
    }

    const WALL_PRICE: u32 = crate::tuning::economy::WALL_PRICE;

    #[test]
    fn a_drag_stops_building_when_the_money_runs_out() {
        let mut grid = grid();
        let mut wallet = Wallet::new(WALL_PRICE * 3 + 1);
        let (mut stroke, _) = Stroke::begin(WALL, IVec2::new(0, 0), &mut grid, &mut wallet);
        assert!(!stroke.take_short());
        stroke.update(&mut grid, &mut wallet, IVec2::new(5, 0));
        assert_eq!(walls(&grid), cells(&[(0, 0), (1, 0), (2, 0)]));
        assert_eq!(wallet.money(), 1);
        assert!(stroke.take_short(), "skipped cells it couldn't pay for");
        assert!(!stroke.take_short(), "only says so once");
        // Pulling the drag back refunds, and the money builds further along the other way.
        stroke.update(&mut grid, &mut wallet, IVec2::new(0, 3));
        assert_eq!(walls(&grid), cells(&[(0, 0), (0, 1), (0, 2)]));
        assert_eq!(wallet.money(), 1);
        stroke.cancel(&mut grid, &mut wallet);
        assert_eq!(wallet.money(), WALL_PRICE * 3 + 1);
    }

    #[test]
    fn a_click_that_cant_be_paid_for_does_nothing() {
        let mut grid = grid();
        let mut wallet = Wallet::new(WALL_PRICE - 1);
        let (mut stroke, changes) = Stroke::begin(WALL, IVec2::new(0, 0), &mut grid, &mut wallet);
        assert!(changes.is_empty() && stroke.is_empty());
        assert!(stroke.take_short());
    }

    #[test]
    fn undo_gives_the_money_back_and_redo_takes_it_again() {
        let mut grid = grid();
        let mut wallet = Wallet::new(100);
        let mut history = History::default();
        let (mut stroke, _) = Stroke::begin(WALL, IVec2::new(0, 0), &mut grid, &mut wallet);
        stroke.update(&mut grid, &mut wallet, IVec2::new(3, 0));
        history.record(stroke.finish());
        assert_eq!(wallet.money(), 100 - 4 * WALL_PRICE);
        history.undo(&mut grid, &mut wallet).unwrap();
        assert_eq!(wallet.money(), 100);
        history.redo(&mut grid, &mut wallet).unwrap();
        assert_eq!(wallet.money(), 100 - 4 * WALL_PRICE);
    }

    #[test]
    fn selling_committed_structures_pays_less() {
        let mut grid = grid();
        let mut wallet = Wallet::new(100);
        let (stroke, _) = Stroke::begin(WALL, IVec2::new(0, 0), &mut grid, &mut wallet);
        let _ = stroke.finish();
        let (stroke, _) = Stroke::begin(WALL, IVec2::new(1, 0), &mut grid, &mut wallet);
        let _ = stroke.finish();
        grid.commit();
        let (mut stroke, _) = Stroke::begin(WALL, IVec2::new(2, 0), &mut grid, &mut wallet);
        stroke.take_short();
        let _ = stroke.finish();
        assert_eq!(wallet.money(), 100 - 3 * WALL_PRICE);
        // Two committed walls at half price (rounded down each), one fresh at full price.
        let (mut stroke, _) =
            Stroke::begin(StrokeKind::Remove, IVec2::new(0, 0), &mut grid, &mut wallet);
        stroke.update(&mut grid, &mut wallet, IVec2::new(2, 0));
        assert_eq!(
            wallet.money(),
            100 - 3 * WALL_PRICE + 2 * (WALL_PRICE / 2) + WALL_PRICE
        );
    }

    #[test]
    fn undoing_a_sale_needs_the_money_back() {
        let mut grid = grid();
        let mut wallet = Wallet::new(WALL_PRICE);
        let mut history = History::default();
        let (stroke, _) = Stroke::begin(WALL, IVec2::new(0, 0), &mut grid, &mut wallet);
        history.record(stroke.finish());
        let (stroke, _) =
            Stroke::begin(StrokeKind::Remove, IVec2::new(0, 0), &mut grid, &mut wallet);
        history.record(stroke.finish());
        assert_eq!(wallet.money(), WALL_PRICE);
        // Spend the refund elsewhere: the sale can't be taken back any more.
        let (stroke, _) = Stroke::begin(WALL, IVec2::new(5, 5), &mut grid, &mut wallet);
        let spent = stroke.finish();
        assert_eq!(wallet.money(), 0);
        assert_eq!(history.undo(&mut grid, &mut wallet), Err(TooExpensive));
        assert_eq!(walls(&grid), cells(&[(5, 5)]), "nothing changed");
        assert!(history.can_undo());
        // With the money back, it can.
        history.record(spent);
        history.undo(&mut grid, &mut wallet).unwrap();
        history.undo(&mut grid, &mut wallet).unwrap();
        assert_eq!(walls(&grid), cells(&[(0, 0)]));
        assert_eq!(wallet.money(), 0);
    }

    #[test]
    fn undo_skips_what_the_horde_broke() {
        let mut grid = grid();
        let mut wallet = Wallet::new(100);
        let mut history = History::default();
        let (mut stroke, _) = Stroke::begin(WALL, IVec2::new(0, 0), &mut grid, &mut wallet);
        stroke.update(&mut grid, &mut wallet, IVec2::new(1, 0));
        history.record(stroke.finish());
        grid.damage(IVec2::new(0, 0), 1000);
        grid.damage(IVec2::new(1, 0), 10);
        // Neither wall is as the edit left it: no refund for either, and the worn one stays.
        let undone = history.undo(&mut grid, &mut wallet).unwrap();
        assert!(undone.is_empty());
        assert_eq!(wallet.money(), 100 - 2 * WALL_PRICE);
        assert_eq!(walls(&grid), cells(&[(1, 0)]));
        // Redo only builds on the cell that's free again.
        history.redo(&mut grid, &mut wallet).unwrap();
        assert_eq!(walls(&grid), cells(&[(0, 0), (1, 0)]));
        assert_eq!(wallet.money(), 100 - 3 * WALL_PRICE);
    }

    #[test]
    fn repair_strokes_mend_damaged_structures() {
        let mut grid = grid();
        let mut wallet = Wallet::new(1000);
        for x in 0..4 {
            grid.set_structure(IVec2::new(x, 0), Some(Structure::new(Kind::Autocannon)));
        }
        grid.commit();
        grid.damage(IVec2::new(0, 0), 75);
        grid.damage(IVec2::new(2, 0), 30);
        let (mut stroke, changes) =
            Stroke::begin(StrokeKind::Repair, IVec2::new(0, 0), &mut grid, &mut wallet);
        assert_eq!(changes.len(), 1);
        assert!(changes[0].change().unwrap().is_repair());
        let changes = stroke.update(&mut grid, &mut wallet, IVec2::new(3, 0));
        assert_eq!(changes.len(), 1, "only the damaged ones");
        let health = |grid: &Grid, x| grid.structure(IVec2::new(x, 0)).unwrap().health().unwrap();
        assert!((0..4).all(|x| health(&grid, x).0 == health(&grid, x).1));
        let cost = crate::economy::repair_cost(Kind::Autocannon, 75, 150)
            + crate::economy::repair_cost(Kind::Autocannon, 120, 150);
        assert_eq!(wallet.money(), 1000 - cost);
        stroke.cancel(&mut grid, &mut wallet);
        assert_eq!(wallet.money(), 1000);
        assert_eq!(health(&grid, 0).0, 75);
    }

    #[test]
    fn repairing_the_command_center_is_an_edit() {
        let mut grid = grid();
        let mut wallet = Wallet::new(1000);
        let mut history = History::default();
        let base = [IVec2::new(2, 2), IVec2::new(3, 2)];
        for cell in base {
            grid.set_structure(cell, Some(Structure::CommandCenter));
        }
        grid.set_structure(IVec2::new(4, 2), Some(Structure::WALL));
        grid.damage(IVec2::new(4, 2), 50);
        grid.command_center.damage(400);
        let cost = command_center_repair_cost(600, 1000);

        // A line across both of its cells and a worn wall: the building is repaired once.
        let (mut stroke, changes) =
            Stroke::begin(StrokeKind::Repair, base[0], &mut grid, &mut wallet);
        assert_eq!(
            changes,
            vec![Action::CommandCenter {
                cell: base[0],
                from: 600,
                to: 1000
            }]
        );
        let changes = stroke.update(&mut grid, &mut wallet, IVec2::new(4, 2));
        assert_eq!(changes.len(), 1, "just the wall");
        let wall_cost = crate::economy::repair_cost(Kind::Wall, 50, 100);
        assert_eq!(grid.command_center.hp, 1000);
        assert_eq!(wallet.money(), 1000 - cost - wall_cost);
        history.record(stroke.finish());

        history.undo(&mut grid, &mut wallet).unwrap();
        assert_eq!(grid.command_center.hp, 600);
        assert_eq!(wallet.money(), 1000);
        history.redo(&mut grid, &mut wallet).unwrap();
        assert_eq!(grid.command_center.hp, 1000);
        assert_eq!(wallet.money(), 1000 - cost - wall_cost);

        // Whole: nothing to repair. Broke: as far as the money goes.
        let (stroke, _) = Stroke::begin(StrokeKind::Repair, base[1], &mut grid, &mut wallet);
        assert!(stroke.is_empty());
        grid.command_center.damage(500);
        let mut wallet = Wallet::new(10);
        let (mut stroke, _) = Stroke::begin(StrokeKind::Repair, base[1], &mut grid, &mut wallet);
        assert!(grid.command_center.hp > 500 && grid.command_center.hp < 1000);
        assert_eq!(wallet.money(), 0);
        assert!(!stroke.take_short(), "spent it all on what it could");
        stroke.cancel(&mut grid, &mut wallet);
        assert_eq!((grid.command_center.hp, wallet.money()), (500, 10));
    }

    #[test]
    fn clearing_forgets_everything() {
        let mut grid = grid();
        let mut wallet = rich();
        let mut history = History::default();
        let (stroke, _) = Stroke::begin(WALL, IVec2::new(0, 0), &mut grid, &mut wallet);
        history.record(stroke.finish());
        history.clear();
        assert!(!history.can_undo() && !history.can_redo());
    }
}
