use serde::{Serialize, Deserialize};

use crate::common::{Entity, Pos3};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnConnectInfo
{
    pub player_entity: Entity,
    pub player_position: Pos3<f32>,
    pub time: f64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player
{
    pub kills: u32
}

impl Default for Player
{
    fn default() -> Self
    {
        Self{kills: 0}
    }
}
