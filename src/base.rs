use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use crate::message::{Message, Update};
use crate::types::{Position, ResourceKind};
use crate::world::WorldState;

pub struct Base {
    world: Arc<RwLock<WorldState>>,
    to_robots: Vec<Sender<Update>>,
    known_resources: HashMap<Position, ResourceKind>,
    known_obstacles: HashSet<Position>,
}

impl Base {
    pub fn new(world: Arc<RwLock<WorldState>>, to_robots: Vec<Sender<Update>>) -> Self {
        Self {
            world,
            to_robots,
            known_resources: HashMap::new(),
            known_obstacles: HashSet::new(),
        }
    }

    fn broadcast(&self, update: Update) {
        for tx in &self.to_robots {
            let _ = tx.send(update.clone());
        }
    }

    pub fn run(&mut self, from_robots: Receiver<Message>, running: Arc<AtomicBool>) {
        while running.load(Ordering::Relaxed) {
            match from_robots.recv_timeout(Duration::from_millis(50)) {
                Ok(msg) => self.handle(msg),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }

    fn handle(&mut self, _msg: Message) {}
}