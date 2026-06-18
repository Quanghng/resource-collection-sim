//! Shared, render-facing world state. The base thread is the *single writer*;
//! the UI and robot threads only ever take read locks.

use std::collections::HashMap;

use crate::map::Map;
use crate::types::{Position, RobotKind};

/// A minimal, render-only snapshot of a robot's externally visible state.
#[derive(Clone)]
pub struct RobotView {
    pub id: usize,
    pub kind: RobotKind,
    pub pos: Position,
    pub carrying: bool,
}

/// Aggregated global state shared behind an `Arc<RwLock<_>>`.
pub struct WorldState {
    pub map: Map,
    pub robots: Vec<RobotView>,
    pub total_energy: u32,
    pub total_crystals: u32,
    pub deliveries: u32,
    /// Number of collectors currently heading for each deposit (load balancing).
    pub claims: HashMap<Position, usize>,
    /// How many distinct deposits the base has learned about.
    pub discovered_resources: usize,
    /// Rolling event log shown in the UI sidebar.
    pub log: Vec<String>,
}

impl WorldState {
    pub fn new(map: Map, robots: Vec<RobotView>) -> Self {
        Self {
            map,
            robots,
            total_energy: 0,
            total_crystals: 0,
            deliveries: 0,
            claims: HashMap::new(),
            discovered_resources: 0,
            log: vec!["Simulation started.".to_string()],
        }
    }

    /// Append an event to the rolling log, keeping only the most recent entries.
    pub fn push_log(&mut self, msg: impl Into<String>) {
        self.log.push(msg.into());
        let max = 8;
        if self.log.len() > max {
            let drop = self.log.len() - max;
            self.log.drain(0..drop);
        }
    }
}
