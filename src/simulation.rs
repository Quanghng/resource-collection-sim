//! Top-level simulation wiring.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use rand::Rng;

use crate::map::Map;
use crate::world::WorldState;

const MAP_WIDTH: i32 = 70;
const MAP_HEIGHT: i32 = 34;
const UI_TICK: Duration = Duration::from_millis(60);

pub fn run() -> std::io::Result<()> {
    let seed: u32 = rand::thread_rng().gen();
    let map = Map::generate(MAP_WIDTH, MAP_HEIGHT, seed);
    let world = Arc::new(RwLock::new(WorldState::new(map, Vec::new())));
    let running = Arc::new(AtomicBool::new(true));

    crate::ui::event_loop(world, running, UI_TICK)
}
