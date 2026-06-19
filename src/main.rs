mod base;
mod map;
mod message;
mod robot;
mod simulation;
mod types;
mod ui;
mod world;

fn main() -> std::io::Result<()> {
    simulation::run()
}
