//! Core domain types shared across the simulation.

/// A 2D grid coordinate. `Copy` so it can be passed cheaply between threads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

impl Position {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Manhattan distance, used as a cheap navigation heuristic.
    pub fn manhattan(&self, other: Position) -> i32 {
        (self.x - other.x).abs() + (self.y - other.y).abs()
    }

    /// The four orthogonally adjacent cells (N, S, W, E).
    pub fn neighbors4(&self) -> [Position; 4] {
        [
            Position::new(self.x, self.y - 1),
            Position::new(self.x, self.y + 1),
            Position::new(self.x - 1, self.y),
            Position::new(self.x + 1, self.y),
        ]
    }
}

/// The two collectable resource types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    Energy,
    Crystal,
}

/// Distinguishes the two robot behaviours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RobotKind {
    Scout,
    Collector,
}
