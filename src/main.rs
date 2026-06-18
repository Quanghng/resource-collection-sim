mod map;
mod simulation;
mod types;
mod ui;
mod world;

fn main() -> std::io::Result<()> {
    simulation::run()
}
