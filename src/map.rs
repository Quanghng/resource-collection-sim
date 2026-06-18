//! Procedural map generation: Perlin-noise obstacles and randomly placed resources.

use std::collections::HashMap;

use noise::{NoiseFn, Perlin};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::types::{Position, ResourceKind};

/// A single terrain cell. Resources are stored separately (see [`Map::resources`])
/// because their quantity changes over time, whereas terrain is static.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tile {
    Empty,
    Obstacle,
}

/// A resource deposit with a depleting quantity.
#[derive(Clone, Debug)]
pub struct Resource {
    pub kind: ResourceKind,
    pub quantity: u32,
}

/// The simulation world grid: static terrain plus mutable resource deposits.
#[derive(Clone)]
pub struct Map {
    pub width: i32,
    pub height: i32,
    tiles: Vec<Tile>,
    pub base: Position,
    /// Live resource deposits keyed by position. Entries are removed on depletion.
    pub resources: HashMap<Position, Resource>,
    pub seed: u32,
}

impl Map {
    /// Generate a map of `width` x `height` using Perlin noise for obstacles and
    /// scattered energy/crystal deposits with random quantities (50-200 units).
    pub fn generate(width: i32, height: i32, seed: u32) -> Self {
        let perlin = Perlin::new(seed);
        let mut tiles = vec![Tile::Empty; (width * height) as usize];

        // Sample Perlin noise; cells above a threshold become obstacles.
        let scale = 0.12;
        for y in 0..height {
            for x in 0..width {
                let v = perlin.get([x as f64 * scale, y as f64 * scale]);
                if v > 0.35 {
                    tiles[(y * width + x) as usize] = Tile::Obstacle;
                }
            }
        }

        let base = Position::new(width / 2, height / 2);
        let mut map = Map {
            width,
            height,
            tiles,
            base,
            resources: HashMap::new(),
            seed,
        };

        // Guarantee the base and its immediate surroundings are traversable so
        // robots are never spawned inside a wall.
        map.set_tile(base, Tile::Empty);
        for n in base.neighbors4() {
            map.set_tile(n, Tile::Empty);
        }

        map.place_resources(seed);
        map
    }

    fn idx(&self, p: Position) -> usize {
        (p.y * self.width + p.x) as usize
    }

    /// Whether a position lies within the grid bounds.
    pub fn in_bounds(&self, p: Position) -> bool {
        p.x >= 0 && p.y >= 0 && p.x < self.width && p.y < self.height
    }

    fn set_tile(&mut self, p: Position, tile: Tile) {
        if self.in_bounds(p) {
            let i = self.idx(p);
            self.tiles[i] = tile;
        }
    }

    /// True only when the in-bounds tile is solid terrain (an obstacle).
    pub fn is_obstacle_tile(&self, p: Position) -> bool {
        self.in_bounds(p) && self.tiles[self.idx(p)] == Tile::Obstacle
    }

    /// Scatter resource deposits on empty, non-base tiles.
    fn place_resources(&mut self, seed: u32) {
        let mut rng = StdRng::seed_from_u64(seed as u64 ^ 0xDEAD_BEEF);
        let target = ((self.width * self.height) as f64 * 0.025) as i32;
        let mut placed = 0;
        let mut attempts = 0;
        while placed < target && attempts < target * 60 {
            attempts += 1;
            let p = Position::new(rng.gen_range(0..self.width), rng.gen_range(0..self.height));
            if p == self.base || self.is_obstacle_tile(p) || self.resources.contains_key(&p) {
                continue;
            }
            let kind = if rng.gen_bool(0.5) {
                ResourceKind::Energy
            } else {
                ResourceKind::Crystal
            };
            let quantity = rng.gen_range(50..=200);
            self.resources.insert(p, Resource { kind, quantity });
            placed += 1;
        }
    }

    /// Total units of a given kind still present on the map (UI / diagnostics).
    pub fn remaining_units(&self) -> u32 {
        self.resources.values().map(|r| r.quantity).sum()
    }
}
