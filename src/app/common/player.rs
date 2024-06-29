use serde::{Serialize, Deserialize};

use crate::common::Entity;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerEntities
{
    pub player: Entity,
    pub other: Vec<Entity>
}

impl PlayerEntities
{
    pub fn is_player(&self, entity: Entity) -> bool
    {
        self.player == entity
            || self.other.contains(&entity)
    }

    pub fn iter(&self) -> impl Iterator<Item=&Entity>
    {
        [&self.player].into_iter().chain(self.other.iter())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player
{
    pub name: String,
    pub strength: f32
}

impl Player
{
    pub fn newtons(&self) -> f32
    {
        self.strength * 30.0
    }
}
