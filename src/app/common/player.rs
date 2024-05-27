use serde::{Serialize, Deserialize};

use crate::common::InventoryItem;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player
{
    pub name: String,
    pub holding: Option<InventoryItem>
}
