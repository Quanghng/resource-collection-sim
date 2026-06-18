use resource_collection_sim::map::Map;
use resource_collection_sim::world::WorldState;

#[test]
fn push_log_keeps_only_recent_entries() {
    let map = Map::generate(10, 10, 1);
    let mut world = WorldState::new(map, Vec::new());

    for i in 0..12 {
        world.push_log(format!("event {i}"));
    }

    assert_eq!(world.log.len(), 8);
    assert_eq!(world.log.first().unwrap(), "event 4");
    assert_eq!(world.log.last().unwrap(), "event 11");
}
