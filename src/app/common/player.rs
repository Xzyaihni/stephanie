use serde::{Serialize, Deserialize};


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
