use serde::{Serialize, Deserialize};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityType
{
	Player(usize),
    Enemy(usize)
}

impl EntityType
{
    pub fn is_player(&self) -> bool
    {
        match self
        {
            Self::Player(_) => true,
            _ => false
        }
    }
}
