use crate::types::{Position, ResourceKind};

pub enum Message {
    Discovery {
        resources: Vec<(Position, ResourceKind)>,
        obstacles: Vec<Position>,
    },
    PositionUpdate {
        id: usize,
        pos: Position,
        carrying: bool,
    },
    ResourcePickedUp { pos: Position, kind: ResourceKind },
    ResourceDelivered { kind: ResourceKind },
    Claim {
        previous: Option<Position>,
        new: Option<Position>,
    },
}

#[derive(Clone)]
pub enum Update {
    Knowledge {
        resources: Vec<(Position, ResourceKind)>,
        obstacles: Vec<Position>,
    },
    ResourceDepleted { pos: Position },
}