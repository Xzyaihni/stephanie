use serde::{Serialize, Deserialize};


#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum EntityType
{
	Player(usize)
}