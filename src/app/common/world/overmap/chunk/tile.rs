use serde::{Serialize, Deserialize};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tile
{
    id: usize
}

impl Tile
{
    pub fn new(id: usize) -> Self
    {
        Self{id}
    }

    pub fn id(&self) -> usize
    {
        self.id
    }

    pub fn none() -> Self
    {
        Self{id: 0}
    }

    pub fn is_none(&self) -> bool
    {
        self.id == 0
    }
}
