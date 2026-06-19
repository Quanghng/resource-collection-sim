//! Autonomous robot behaviours. Each robot runs on its own thread, owns its
//! [`LocalKnowledge`], and communicates with the base purely via channels.
//!
//! Concurrency rules enforced here:
//! * A robot only ever takes a **read** lock on the shared world, and never
//!   holds the lock across a channel `send` (avoids deadlock / contention).
//! * All mutations to global state happen on the base thread (single-writer).

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::message::{Message, Update};
use crate::types::{Position, ResourceKind};
use crate::world::WorldState;

/// A robot's private, incrementally-built model of the world. Robots start blind.
#[derive(Default)]
pub struct LocalKnowledge {
    pub resources: HashMap<Position, ResourceKind>,
    pub obstacles: HashSet<Position>,
    pub depleted: HashSet<Position>,
}

impl LocalKnowledge {
    /// Merge knowledge broadcast by the base (other robots' discoveries).
    fn apply(&mut self, update: Update) {
        match update {
            Update::Knowledge {
                resources,
                obstacles,
            } => {
                for (p, kind) in resources {
                    self.resources.entry(p).or_insert(kind);
                }
                for p in obstacles {
                    self.obstacles.insert(p);
                }
            }
            Update::ResourceDepleted { pos } => {
                self.depleted.insert(pos);
                self.resources.remove(&pos);
            }
        }
    }
}

// --- Shared navigation helpers -------------------------------------------------

/// A cell is impassable if it is out of bounds or solid terrain.
fn is_blocked(p: Position, world: &WorldState) -> bool {
    !world.map.in_bounds(p) || world.map.is_obstacle_tile(p)
}

/// Cells currently occupied by *other* robots (for soft collision avoidance).
fn occupied_cells(world: &WorldState, self_id: usize) -> HashSet<Position> {
    world
        .robots
        .iter()
        .filter(|r| r.id != self_id)
        .map(|r| r.pos)
        .collect()
}

/// Sense the surroundings (Chebyshev radius 2), recording anything new into
/// `knowledge` and returning only the freshly discovered items to report.
fn sense(
    pos: Position,
    world: &WorldState,
    knowledge: &mut LocalKnowledge,
) -> (Vec<(Position, ResourceKind)>, Vec<Position>) {
    let mut new_res = Vec::new();
    let mut new_obs = Vec::new();
    let r = 2;
    for dy in -r..=r {
        for dx in -r..=r {
            let p = Position::new(pos.x + dx, pos.y + dy);
            if !world.map.in_bounds(p) {
                continue;
            }
            if world.map.is_obstacle_tile(p) {
                if knowledge.obstacles.insert(p) {
                    new_obs.push(p);
                }
            } else if let Some(res) = world.map.resources.get(&p) {
                if res.quantity > 0
                    && !knowledge.depleted.contains(&p)
                    && !knowledge.resources.contains_key(&p)
                {
                    knowledge.resources.insert(p, res.kind);
                    new_res.push((p, res.kind));
                }
            }
        }
    }
    (new_res, new_obs)
}

/// Breadth-first search returning the *first step* of a shortest path from
/// `start` to `goal`, or `None` if `goal` is unreachable. Bounded by map size.
fn bfs_step(start: Position, goal: Position, world: &WorldState) -> Option<Position> {
    if start == goal {
        return Some(start);
    }
    let mut visited: HashSet<Position> = HashSet::new();
    let mut came_from: HashMap<Position, Position> = HashMap::new();
    let mut queue: VecDeque<Position> = VecDeque::new();
    visited.insert(start);
    queue.push_back(start);

    while let Some(cur) = queue.pop_front() {
        for n in cur.neighbors4() {
            if visited.contains(&n) {
                continue;
            }
            if n != goal && is_blocked(n, world) {
                continue;
            }
            visited.insert(n);
            came_from.insert(n, cur);
            if n == goal {
                // Walk the predecessor chain back to the first move after `start`.
                let mut step = n;
                while came_from[&step] != start {
                    step = came_from[&step];
                }
                return Some(step);
            }
            queue.push_back(n);
        }
    }
    None
}

/// One step toward `goal`. Returns `Some(pos)` (stay put) if the next cell is
/// temporarily occupied, or `None` if the goal is unreachable.
fn step_toward(
    pos: Position,
    goal: Position,
    world: &WorldState,
    occupied: &HashSet<Position>,
) -> Option<Position> {
    match bfs_step(pos, goal, world) {
        Some(next) if next != pos && occupied.contains(&next) => Some(pos),
        other => other,
    }
}

/// Pick a random unobstructed, unoccupied neighbouring cell (random walk).
fn random_step(
    pos: Position,
    world: &WorldState,
    occupied: &HashSet<Position>,
    rng: &mut StdRng,
) -> Position {
    let candidates: Vec<Position> = pos
        .neighbors4()
        .into_iter()
        .filter(|p| !is_blocked(*p, world) && !occupied.contains(p))
        .collect();
    if candidates.is_empty() {
        pos
    } else {
        candidates[rng.gen_range(0..candidates.len())]
    }
}

/// Drain any pending knowledge broadcasts without blocking.
fn drain_updates(updates: &Receiver<Update>, knowledge: &mut LocalKnowledge) {
    while let Ok(u) = updates.try_recv() {
        knowledge.apply(u);
    }
}

// --- Scout ---------------------------------------------------------------------

/// Scout loop: explore randomly, avoid obstacles, and broadcast discoveries.
/// Scouts never collect resources.
pub fn run_scout(
    id: usize,
    start: Position,
    world: Arc<RwLock<WorldState>>,
    to_base: Sender<Message>,
    updates: Receiver<Update>,
    running: Arc<AtomicBool>,
    tick: Duration,
) {
    let mut rng = StdRng::seed_from_u64((id as u64).wrapping_mul(0x9E37_79B9) ^ 0xA5A5);
    let mut knowledge = LocalKnowledge::default();
    let mut pos = start;

    while running.load(Ordering::Relaxed) {
        drain_updates(&updates, &mut knowledge);

        // Single read lock: sense + decide, then drop the guard before sending.
        let (new_res, new_obs, next) = {
            let w = world.read().unwrap();
            let (nr, no) = sense(pos, &w, &mut knowledge);
            let occupied = occupied_cells(&w, id);
            let next = random_step(pos, &w, &occupied, &mut rng);
            (nr, no, next)
        };

        if !new_res.is_empty() || !new_obs.is_empty() {
            let _ = to_base.send(Message::Discovery {
                resources: new_res,
                obstacles: new_obs,
            });
        }
        pos = next;
        let _ = to_base.send(Message::PositionUpdate {
            id,
            pos,
            carrying: false,
        });

        std::thread::sleep(tick);
    }
}

// --- Collector -----------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum CollectorState {
    Searching,
    ToResource,
    Returning,
}

/// Choose the most attractive known deposit: nearest, biased away from deposits
/// that are already heavily claimed or known to be unreachable.
fn choose_target(
    pos: Position,
    knowledge: &LocalKnowledge,
    world: &WorldState,
    unreachable: &HashSet<Position>,
) -> Option<(Position, ResourceKind)> {
    let mut best: Option<(Position, ResourceKind, i32)> = None;
    for (&rp, &kind) in &knowledge.resources {
        if knowledge.depleted.contains(&rp) || unreachable.contains(&rp) {
            continue;
        }
        let remaining = world.map.resources.get(&rp).map(|r| r.quantity).unwrap_or(0);
        if remaining == 0 {
            continue;
        }
        let claims = *world.claims.get(&rp).unwrap_or(&0) as u32;
        if claims >= remaining {
            continue; // already fully spoken for
        }
        let score = pos.manhattan(rp) + claims as i32 * 3;
        if best.map_or(true, |(_, _, bs)| score < bs) {
            best = Some((rp, kind, score));
        }
    }
    best.map(|(p, k, _)| (p, k))
}

/// Collector loop: locate known deposits, gather one unit at a time, and return
/// to base to unload. Implements a small state machine plus soft claims.
pub fn run_collector(
    id: usize,
    start: Position,
    world: Arc<RwLock<WorldState>>,
    to_base: Sender<Message>,
    updates: Receiver<Update>,
    running: Arc<AtomicBool>,
    tick: Duration,
) {
    let mut rng = StdRng::seed_from_u64((id as u64).wrapping_mul(0x85EB_CA77) ^ 0x1234_5678);
    let mut knowledge = LocalKnowledge::default();
    let mut pos = start;
    let mut state = CollectorState::Searching;
    let mut target: Option<Position> = None;
    let mut carrying: Option<ResourceKind> = None;
    let mut unreachable: HashSet<Position> = HashSet::new();
    let mut stuck_ticks: u32 = 0;

    while running.load(Ordering::Relaxed) {
        drain_updates(&updates, &mut knowledge);

        // If our target was depleted while we were en route, abandon it.
        if let Some(t) = target {
            if knowledge.depleted.contains(&t) && state == CollectorState::ToResource {
                let _ = to_base.send(Message::Claim {
                    previous: target,
                    new: None,
                });
                target = None;
                state = CollectorState::Searching;
            }
        }

        let mut outgoing: Vec<Message> = Vec::new();

        // All world reads happen inside this scope; the guard is dropped before
        // we send anything on the channel.
        {
            let w = world.read().unwrap();
            let (nr, no) = sense(pos, &w, &mut knowledge);
            if !nr.is_empty() || !no.is_empty() {
                outgoing.push(Message::Discovery {
                    resources: nr,
                    obstacles: no,
                });
            }
            let occupied = occupied_cells(&w, id);

            match state {
                CollectorState::Searching => {
                    if let Some((t, _kind)) = choose_target(pos, &knowledge, &w, &unreachable) {
                        outgoing.push(Message::Claim {
                            previous: target,
                            new: Some(t),
                        });
                        target = Some(t);
                        state = CollectorState::ToResource;
                    } else {
                        // Nothing known yet: wander to help discovery.
                        pos = random_step(pos, &w, &occupied, &mut rng);
                    }
                }
                CollectorState::ToResource => {
                    let t = target.expect("ToResource without target");
                    let depleted =
                        w.map.resources.get(&t).map_or(true, |r| r.quantity == 0);
                    if depleted {
                        outgoing.push(Message::Claim {
                            previous: target,
                            new: None,
                        });
                        target = None;
                        state = CollectorState::Searching;
                    } else if pos == t {
                        let kind = w.map.resources.get(&t).map(|r| r.kind).unwrap();
                        outgoing.push(Message::ResourcePickedUp { pos: t, kind });
                        carrying = Some(kind);
                        state = CollectorState::Returning;
                    } else if stuck_ticks >= 3 {
                        // Deadlocked against another robot for too long: sidestep.
                        pos = random_step(pos, &w, &occupied, &mut rng);
                        stuck_ticks = 0;
                    } else {
                        match step_toward(pos, t, &w, &occupied) {
                            Some(next) if next == pos => stuck_ticks += 1,
                            Some(next) => {
                                pos = next;
                                stuck_ticks = 0;
                            }
                            None => {
                                // Unreachable for now: blacklist and re-plan.
                                unreachable.insert(t);
                                outgoing.push(Message::Claim {
                                    previous: target,
                                    new: None,
                                });
                                target = None;
                                state = CollectorState::Searching;
                                stuck_ticks = 0;
                            }
                        }
                    }
                }
                CollectorState::Returning => {
                    let base = w.map.base;
                    if pos == base {
                        if let Some(k) = carrying.take() {
                            outgoing.push(Message::ResourceDelivered { kind: k });
                        }
                        outgoing.push(Message::Claim {
                            previous: target,
                            new: None,
                        });
                        target = None;
                        state = CollectorState::Searching;
                        stuck_ticks = 0;
                    } else if stuck_ticks >= 3 {
                        pos = random_step(pos, &w, &occupied, &mut rng);
                        stuck_ticks = 0;
                    } else {
                        match step_toward(pos, base, &w, &occupied) {
                            Some(next) if next == pos => stuck_ticks += 1,
                            Some(next) => {
                                pos = next;
                                stuck_ticks = 0;
                            }
                            None => pos = random_step(pos, &w, &occupied, &mut rng),
                        }
                    }
                }
            }
        } // read guard dropped here

        outgoing.push(Message::PositionUpdate {
            id,
            pos,
            carrying: carrying.is_some(),
        });
        for m in outgoing {
            let _ = to_base.send(m);
        }

        std::thread::sleep(tick);
    }
}
