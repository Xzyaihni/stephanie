use serde::{Serialize, Deserialize};

use crate::common::Item;

pub use sorter::InventorySorter;

mod sorter;


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InventoryItem(usize);

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

    pub fn push(&mut self, item: Item)
    {
        self.items.push(item);
    }

    pub fn get(&self, id: InventoryItem) -> &Item
    {
        &self.items[id.0]
    }

    pub fn items(&self) -> &[Item]
    {
        &self.items
    }
}
