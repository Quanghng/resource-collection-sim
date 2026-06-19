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

    fn handle(&mut self, msg: Message) {
        match msg {
            Message::Discovery { resources, obstacles } => {
                self.handle_discovery(resources, obstacles)
            }
            Message::PositionUpdate { id, pos, carrying } => {
                let mut w = self.world.write().unwrap();
                if let Some(view) = w.robots.get_mut(id) {
                    view.pos = pos;
                    view.carrying = carrying;
                }
            }
            Message::ResourcePickedUp { pos, kind } => self.handle_pickup(pos, kind),
            Message::ResourceDelivered { kind } => {
                let mut w = self.world.write().unwrap();
                match kind {
                    ResourceKind::Energy => w.total_energy += 1,
                    ResourceKind::Crystal => w.total_crystals += 1,
                }
                w.deliveries += 1;
            }
            Message::Claim { previous, new } => {
                let mut w = self.world.write().unwrap();
                if let Some(p) = previous {
                    if let Some(c) = w.claims.get_mut(&p) {
                        *c = c.saturating_sub(1);
                    }
                }
                if let Some(p) = new {
                    *w.claims.entry(p).or_insert(0) += 1;
                }
            }
        }
    }

    fn handle_discovery(
        &mut self,
        resources: Vec<(Position, ResourceKind)>,
        obstacles: Vec<Position>,
    ) {
        let mut new_resources = Vec::new();
        let mut new_obstacles = Vec::new();

        for (p, kind) in resources {
            if self.known_resources.insert(p, kind).is_none() {
                new_resources.push((p, kind));
            }
        }
        for p in obstacles {
            if self.known_obstacles.insert(p) {
                new_obstacles.push(p);
            }
        }

        if new_resources.is_empty() && new_obstacles.is_empty() {
            return;
        }

        {
            let mut w = self.world.write().unwrap();
            w.discovered_resources = self.known_resources.len();
            for (p, kind) in &new_resources {
                w.push_log(format!("Discovered {} at ({}, {})", kind_label(*kind), p.x, p.y));
            }
        }

        self.broadcast(Update::Knowledge {
            resources: new_resources,
            obstacles: new_obstacles,
        });
    }

    fn handle_pickup(&mut self, pos: Position, _kind: ResourceKind) {
        let mut depleted = false;
        {
            let mut w = self.world.write().unwrap();
            if let Some(res) = w.map.resources.get_mut(&pos) {
                res.quantity = res.quantity.saturating_sub(1);
                if res.quantity == 0 {
                    depleted = true;
                }
            }
            if depleted {
                w.map.resources.remove(&pos);
                w.claims.remove(&pos);
                w.push_log(format!("Deposit at ({}, {}) depleted", pos.x, pos.y));
            }
        }
        if depleted {
            self.known_resources.remove(&pos);
            self.broadcast(Update::ResourceDepleted { pos });
        }
    }
}

fn kind_label(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Energy => "energy",
        ResourceKind::Crystal => "crystal",
    }
}