use std::ops::{Index, IndexMut};

use serde::{Serialize, Deserialize};

use crate::common::{ObjectsStore, Item};

pub use sorter::InventorySorter;

mod sorter;


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InventoryItem(usize);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inventory
{
    items: ObjectsStore<Item>
}

impl Index<InventoryItem> for Inventory
{
    type Output = Item;

    fn index(&self, index: InventoryItem) -> &Self::Output
    {
        &self.items[index.0]
    }
}

impl IndexMut<InventoryItem> for Inventory
{
    fn index_mut(&mut self, index: InventoryItem) -> &mut Self::Output
    {
        &mut self.items[index.0]
    }
}

impl Inventory
{
    pub fn new() -> Self
    {
        Self{items: ObjectsStore::new()}
    }

    pub fn push(&mut self, item: Item) -> InventoryItem
    {
        InventoryItem(self.items.push(item))
    }

    pub fn get(&self, id: InventoryItem) -> Option<&Item>
    {
        self.items.get(id.0)
    }

    pub fn get_mut(&mut self, id: InventoryItem) -> Option<&mut Item>
    {
        self.items.get_mut(id.0)
    }

    pub fn remove(&mut self, id: InventoryItem) -> Option<Item>
    {
        self.items.remove(id.0)
    }

    pub fn is_empty(&self) -> bool
    {
        self.items.is_empty()
    }

    pub fn random(&self) -> Option<InventoryItem>
    {
        if self.items.is_empty()
        {
            return None;
        }

        let id = fastrand::usize(0..self.items.len());

        Some(InventoryItem(id))
    }

    pub fn items(&self) -> impl Iterator<Item=&Item>
    {
        self.items.iter().map(|(_, x)| x)
    }

    pub fn items_ids(&self) -> impl Iterator<Item=(InventoryItem, &Item)>
    {
        self.items.iter().map(|(index, item)| (InventoryItem(index), item))
    }
}
