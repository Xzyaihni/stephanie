use serde::{Serialize, Deserialize};


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
