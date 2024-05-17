use serde::{Serialize, Deserialize};

use crate::common::Item;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inventory
{
    items: Vec<Item>
}

impl Inventory
{
    pub fn new() -> Self
    {
        Self{items: Vec::new()}
    }
}
