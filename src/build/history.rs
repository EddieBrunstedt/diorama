//! Building as plain data: strokes (a click or a drag, applied live to the grid) and the
//! undo/redo history of finished strokes.

use crate::grid::{Change, Grid, Structure};
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
    Place(Structure),
    Remove,
}

/// A click or drag in progress. It changes the grid live, so feedback is instant, and becomes
/// one undo step when it's finished.
#[derive(Clone, Debug)]
pub struct Stroke {
    pub kind: StrokeKind,
    start: IVec2,
    end: IVec2,
    /// What the stroke has changed so far, in order.
    changes: Vec<Change>,
}

impl Stroke {
    /// Starts a stroke at `start`, which already acts on that cell.
    pub fn begin(kind: StrokeKind, start: IVec2, grid: &mut Grid) -> (Self, Vec<Change>) {
        let mut stroke = Self {
            kind,
            start,
            end: start,
            changes: Vec::new(),
        };
        let changes = stroke.update(grid, start);
        (stroke, changes)
    }

    /// The cells the stroke covers now.
    pub fn cells(&self) -> Vec<IVec2> {
        line(self.start, self.end)
    }

    /// Whether the stroke changed `cell`.
    pub fn changed(&self, cell: IVec2) -> bool {
        self.changes.iter().any(|change| change.cell == cell)
    }

    /// What stood on `cell` before the stroke began, given the grid as it is now.
    pub fn structure_before(&self, grid: &Grid, cell: IVec2) -> Option<Structure> {
        match self.changes.iter().find(|change| change.cell == cell) {
            Some(change) => change.before,
            None => grid.structure(cell),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Moves the stroke's end to `end`: takes back what left the line and acts on the new
    /// cells. Returns the changes made to the grid, in order.
    pub fn update(&mut self, grid: &mut Grid, end: IVec2) -> Vec<Change> {
        self.end = end;
        let cells = line(self.start, end);
        let mut applied = Vec::new();

        // Newest first, so the grid steps back through the same states it went through.
        let mut kept = Vec::new();
        for change in self.changes.drain(..).rev() {
            if cells.contains(&change.cell) {
                kept.push(change);
            } else if let Some(undone) = grid.apply(change.inverse()) {
                applied.push(undone);
            }
        }
        kept.reverse();
        self.changes = kept;

        for cell in cells {
            if self.changed(cell) {
                continue;
            }
            let wanted = match self.kind {
                StrokeKind::Place(structure) if grid.check_build(cell).is_ok() => Some(structure),
                StrokeKind::Remove if grid.structure(cell).is_some() => None,
                _ => continue,
            };
            if let Some(change) = grid.set_structure(cell, wanted) {
                self.changes.push(change);
                applied.push(change);
            }
        }
        applied
    }

    /// Takes back everything the stroke did. Returns the changes made to the grid.
    pub fn cancel(mut self, grid: &mut Grid) -> Vec<Change> {
        self.changes
            .drain(..)
            .rev()
            .filter_map(|change| grid.apply(change.inverse()))
            .collect()
    }

    /// The stroke as one undo step.
    pub fn finish(self) -> Edit {
        Edit {
            changes: self.changes,
        }
    }
}

/// One undo step: the changes it made, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Edit {
    changes: Vec<Change>,
}

/// Undo and redo stacks for the build phase.
#[derive(Resource, Clone, Debug, Default)]
pub struct History {
    undo: Vec<Edit>,
    redo: Vec<Edit>,
}

impl History {
    /// Adds a finished edit. Anything that was undone can't be redone after this.
    pub fn record(&mut self, edit: Edit) {
        if edit.changes.is_empty() {
            return;
        }
        self.undo.push(edit);
        self.redo.clear();
    }

    /// Takes back the last edit. Returns the changes made to the grid (empty if there was
    /// nothing to undo).
    pub fn undo(&mut self, grid: &mut Grid) -> Vec<Change> {
        let Some(edit) = self.undo.pop() else {
            return Vec::new();
        };
        let applied = edit
            .changes
            .iter()
            .rev()
            .filter_map(|change| grid.apply(change.inverse()))
            .collect();
        self.redo.push(edit);
        applied
    }

    /// Does the last undone edit again. Returns the changes made to the grid.
    pub fn redo(&mut self, grid: &mut Grid) -> Vec<Change> {
        let Some(edit) = self.redo.pop() else {
            return Vec::new();
        };
        let applied = edit
            .changes
            .iter()
            .filter_map(|&change| grid.apply(change))
            .collect();
        self.undo.push(edit);
        applied
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

    const WALL: StrokeKind = StrokeKind::Place(Structure::Wall);

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
        let (stroke, changes) = Stroke::begin(WALL, IVec2::new(3, 4), &mut grid);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].after, Some(Structure::Wall));
        assert!(!stroke.is_empty());
        assert_eq!(walls(&grid), cells(&[(3, 4)]));
    }

    #[test]
    fn dragging_places_a_line_around_blocked_cells() {
        let mut grid = grid();
        grid.set_terrain(IVec2::new(3, 1), Terrain::Rock);
        grid.set_structure(IVec2::new(5, 1), Some(Structure::Wall));
        let (mut stroke, _) = Stroke::begin(WALL, IVec2::new(1, 1), &mut grid);
        let changes = stroke.update(&mut grid, IVec2::new(6, 2));
        let placed: Vec<IVec2> = changes.iter().map(|change| change.cell).collect();
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
        let (mut stroke, _) = Stroke::begin(WALL, IVec2::new(1, 1), &mut grid);
        stroke.update(&mut grid, IVec2::new(4, 1));
        let changes = stroke.update(&mut grid, IVec2::new(2, 1));
        assert_eq!(changes.len(), 2);
        assert!(changes.iter().all(|change| change.after.is_none()));
        assert_eq!(changes[0].cell, IVec2::new(4, 1), "newest first");
        assert_eq!(walls(&grid), cells(&[(1, 1), (2, 1)]));

        stroke.update(&mut grid, IVec2::new(1, 3));
        assert_eq!(walls(&grid), cells(&[(1, 1), (1, 2), (1, 3)]));
        assert_eq!(stroke.cells(), cells(&[(1, 1), (1, 2), (1, 3)]));
    }

    #[test]
    fn a_drag_never_removes_walls_it_did_not_place() {
        let mut grid = grid();
        grid.set_structure(IVec2::new(3, 1), Some(Structure::Wall));
        let (mut stroke, _) = Stroke::begin(WALL, IVec2::new(1, 1), &mut grid);
        stroke.update(&mut grid, IVec2::new(4, 1));
        stroke.update(&mut grid, IVec2::new(1, 1));
        assert_eq!(walls(&grid), cells(&[(1, 1), (3, 1)]));
    }

    #[test]
    fn remove_strokes_clear_walls() {
        let mut grid = grid();
        for x in 0..5 {
            grid.set_structure(IVec2::new(x, 0), Some(Structure::Wall));
        }
        let (mut stroke, changes) = Stroke::begin(StrokeKind::Remove, IVec2::new(1, 0), &mut grid);
        assert_eq!(changes.len(), 1);
        stroke.update(&mut grid, IVec2::new(3, 0));
        assert_eq!(walls(&grid), cells(&[(0, 0), (4, 0)]));
        // Going back puts them up again.
        stroke.update(&mut grid, IVec2::new(2, 0));
        assert_eq!(walls(&grid), cells(&[(0, 0), (3, 0), (4, 0)]));
    }

    #[test]
    fn a_stroke_remembers_what_stood_before_it() {
        let mut grid = grid();
        grid.set_structure(IVec2::new(2, 0), Some(Structure::Wall));
        let (mut stroke, _) = Stroke::begin(WALL, IVec2::new(0, 0), &mut grid);
        stroke.update(&mut grid, IVec2::new(3, 0));
        let before = |x| stroke.structure_before(&grid, IVec2::new(x, 0));
        assert_eq!(before(0), None, "placed by the stroke");
        assert_eq!(before(2), Some(Structure::Wall), "already there");
        assert_eq!(before(5), None, "untouched");

        let (stroke, _) = Stroke::begin(StrokeKind::Remove, IVec2::new(1, 0), &mut grid);
        assert_eq!(grid.structure(IVec2::new(1, 0)), None);
        assert_eq!(
            stroke.structure_before(&grid, IVec2::new(1, 0)),
            Some(Structure::Wall),
            "removed by the stroke"
        );
    }

    #[test]
    fn a_click_on_nothing_is_an_empty_stroke() {
        let mut grid = grid();
        grid.set_terrain(IVec2::new(2, 2), Terrain::Rock);
        let (stroke, changes) = Stroke::begin(WALL, IVec2::new(2, 2), &mut grid);
        assert!(changes.is_empty() && stroke.is_empty());
        let (stroke, _) = Stroke::begin(StrokeKind::Remove, IVec2::new(0, 0), &mut grid);
        assert!(stroke.is_empty());
        let (stroke, _) = Stroke::begin(WALL, IVec2::new(-5, 0), &mut grid);
        assert!(stroke.is_empty(), "outside the map");
    }

    #[test]
    fn cancelling_restores_the_grid() {
        let mut grid = grid();
        grid.set_structure(IVec2::new(0, 5), Some(Structure::Wall));
        let before = grid.clone();
        let (mut stroke, _) = Stroke::begin(WALL, IVec2::new(0, 0), &mut grid);
        stroke.update(&mut grid, IVec2::new(0, 8));
        let changes = stroke.cancel(&mut grid);
        assert_eq!(changes.len(), 8);
        assert_eq!(grid, before);
    }

    #[test]
    fn undo_and_redo_whole_strokes() {
        let mut grid = grid();
        let mut history = History::default();
        let empty = grid.clone();

        let (mut stroke, _) = Stroke::begin(WALL, IVec2::new(0, 0), &mut grid);
        stroke.update(&mut grid, IVec2::new(3, 0));
        history.record(stroke.finish());
        let one_line = grid.clone();

        let (stroke, _) = Stroke::begin(StrokeKind::Remove, IVec2::new(1, 0), &mut grid);
        history.record(stroke.finish());
        assert_eq!(walls(&grid), cells(&[(0, 0), (2, 0), (3, 0)]));

        assert_eq!(history.undo(&mut grid).len(), 1);
        assert_eq!(grid, one_line);
        let undone = history.undo(&mut grid);
        assert_eq!(undone.len(), 4);
        assert_eq!(undone[0].cell, IVec2::new(3, 0), "newest first");
        assert_eq!(grid, empty);
        assert!(!history.can_undo());
        assert!(history.undo(&mut grid).is_empty());

        assert_eq!(history.redo(&mut grid).len(), 4);
        assert_eq!(grid, one_line);
        assert!(history.can_redo());
    }

    #[test]
    fn a_new_edit_drops_the_redo_stack() {
        let mut grid = grid();
        let mut history = History::default();
        let (stroke, _) = Stroke::begin(WALL, IVec2::new(0, 0), &mut grid);
        history.record(stroke.finish());
        history.undo(&mut grid);
        assert!(history.can_redo());

        // An empty stroke isn't an edit, so it keeps the redo.
        grid.set_terrain(IVec2::new(9, 9), Terrain::Rock);
        let (stroke, _) = Stroke::begin(WALL, IVec2::new(9, 9), &mut grid);
        history.record(stroke.finish());
        assert!(history.can_redo());

        let (stroke, _) = Stroke::begin(WALL, IVec2::new(5, 5), &mut grid);
        history.record(stroke.finish());
        assert!(!history.can_redo());
        assert!(history.redo(&mut grid).is_empty());
    }
}
