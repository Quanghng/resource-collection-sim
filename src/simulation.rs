use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use rand::Rng;

use crate::map::Map;
use crate::types::RobotKind;
use crate::world::{RobotView, WorldState};

const MAP_WIDTH: i32 = 70;
const MAP_HEIGHT: i32 = 34;
const NUM_SCOUTS: usize = 4;
const NUM_COLLECTORS: usize = 4;
const UI_TICK: Duration = Duration::from_millis(60);

pub fn run() -> std::io::Result<()> {
    let seed: u32 = rand::thread_rng().gen();
    let map = Map::generate(MAP_WIDTH, MAP_HEIGHT, seed);
    let base_pos = map.base;

    let mut robots = Vec::new();
    for id in 0..NUM_SCOUTS {
        robots.push(RobotView {
            id,
            kind: RobotKind::Scout,
            pos: base_pos,
            carrying: false,
        });
    }
    for i in 0..NUM_COLLECTORS {
        let id = NUM_SCOUTS + i;
        robots.push(RobotView {
            id,
            kind: RobotKind::Collector,
            pos: base_pos,
            carrying: false,
        });
    }

    let world = Arc::new(RwLock::new(WorldState::new(map, robots)));
    let running = Arc::new(AtomicBool::new(true));

    crate::ui::event_loop(world, running, UI_TICK)
}
