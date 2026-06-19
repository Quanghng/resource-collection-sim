use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use rand::Rng;

use crate::base::Base;
use crate::map::Map;
use crate::robot::{run_collector, run_scout};
use crate::types::RobotKind;
use crate::world::{RobotView, WorldState};

const MAP_WIDTH: i32 = 70;
const MAP_HEIGHT: i32 = 34;
const NUM_SCOUTS: usize = 4;
const NUM_COLLECTORS: usize = 4;
const ROBOT_TICK: Duration = Duration::from_millis(90);
const UI_TICK: Duration = Duration::from_millis(60);

/// Build and run the entire simulation. Blocks until the user exits.
pub fn run() -> std::io::Result<()> {
    let seed: u32 = rand::thread_rng().gen();
    let map = Map::generate(MAP_WIDTH, MAP_HEIGHT, seed);
    let base_pos = map.base;

    // All robots start at the base. IDs are dense indices: scouts first.
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

    // Robots -> base channel (many producers, single consumer).
    let (to_base_tx, to_base_rx) = mpsc::channel();
    // Base -> each robot channel (single producer per robot).
    let mut update_senders = Vec::new();
    let mut handles = Vec::new();

    for id in 0..(NUM_SCOUTS + NUM_COLLECTORS) {
        let (utx, urx) = mpsc::channel();
        update_senders.push(utx);

        let world_c = Arc::clone(&world);
        let to_base = to_base_tx.clone();
        let running_c = Arc::clone(&running);
        let is_scout = id < NUM_SCOUTS;
        let start = base_pos;

        let handle = thread::Builder::new()
            .name(format!("robot-{id}"))
            .spawn(move || {
                if is_scout {
                    run_scout(id, start, world_c, to_base, urx, running_c, ROBOT_TICK);
                } else {
                    run_collector(id, start, world_c, to_base, urx, running_c, ROBOT_TICK);
                }
            })
            .expect("failed to spawn robot thread");
        handles.push(handle);
    }
    // Drop the original sender so only robot clones keep the channel alive.
    drop(to_base_tx);

    // Base hub thread (single writer of global state).
    let base_world = Arc::clone(&world);
    let base_running = Arc::clone(&running);
    let base_handle = thread::Builder::new()
        .name("base".to_string())
        .spawn(move || {
            let mut base = Base::new(base_world, update_senders);
            base.run(to_base_rx, base_running);
        })
        .expect("failed to spawn base thread");

    let ui_result = crate::ui::event_loop(Arc::clone(&world), Arc::clone(&running), UI_TICK);

    running.store(false, Ordering::Relaxed);
    let _ = base_handle.join();
    for handle in handles {
        let _ = handle.join();
    }

    ui_result
}
