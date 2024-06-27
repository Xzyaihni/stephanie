use serde::{Serialize, Deserialize};

use crate::common::{Entity, InventoryItem};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerEntities
{
    pub player: Entity,
    pub holding: Entity,
    pub holding_right: Entity,
    pub other: Vec<Entity>
}

impl PlayerEntities
{
    pub fn is_player(&self, entity: Entity) -> bool
    {
        self.player == entity
            || self.holding == entity
            || self.other.contains(&entity)
    }

    pub fn iter(&self) -> impl Iterator<Item=&Entity>
    {
        [&self.player, &self.holding].into_iter().chain(self.other.iter())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player
{
    pub name: String,
    pub strength: f32,
    pub holding: Option<InventoryItem>
}

impl Player
{
    pub fn newtons(&self) -> f32
    {
        self.strength * 30.0
    }
}
