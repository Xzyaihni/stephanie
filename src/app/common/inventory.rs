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

    pub fn get(&self, id: InventoryItem) -> Option<&Item>
    {
        self.items.get(id.0)
    }

    pub fn remove(&mut self, id: InventoryItem) -> Option<Item>
    {
        if self.items.get(id.0).is_none()
        {
            None
        } else
        {
            Some(self.items.remove(id.0))
        }
    }

    pub fn is_empty(&self) -> bool
    {
        self.items.is_empty()
    }

    pub fn items(&self) -> &[Item]
    {
        &self.items
    }

    pub fn random(&self) -> InventoryItem
    {
        let id = fastrand::usize(0..self.items.len());

        InventoryItem(id)
    }

    pub fn items_ids(&self) -> impl Iterator<Item=(InventoryItem, &Item)>
    {
        self.items.iter().enumerate().map(|(index, item)| (InventoryItem(index), item))
    }
}
