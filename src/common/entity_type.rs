use serde::{Serialize, Deserialize};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityType
{
	Player(usize)
}