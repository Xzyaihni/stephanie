use serde::{Serialize, Deserialize};

use strum::{EnumIter, EnumCount};

use crate::common::{Entity, Pos3};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnConnectInfo
{
    pub player_entity: Entity,
    pub player_position: Pos3<f32>,
    pub time: f64
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct StatLevel
{
    pub level: u32,
    pub experience: f32
}

impl Default for StatLevel
{
    fn default() -> Self
    {
        Self{level: 0, experience: 0.0}
    }
}

#[derive(Debug, Clone, PartialEq, Eq, EnumIter, EnumCount, Serialize, Deserialize)]
pub enum StatId
{
    Melee = 0,
    Bash,
    Poke,
    Throw,
    Ranged
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player
{
    pub kills: u32,
    pub levels: [StatLevel; StatId::COUNT]
}

impl Default for Player
{
    fn default() -> Self
    {
        Self{kills: 0, levels: [StatLevel::default(); StatId::COUNT]}
    }
}
