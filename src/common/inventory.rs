use std::ops::{Index, Deref, DerefMut};

use serde::{Serialize, Deserialize};

use crate::common::{
    SimpleF32,
    ObjectsStore,
    Item,
    ItemsInfo,
    Anatomy
};

pub use sorter::InventorySorter;

mod sorter;


pub fn anatomy_weight_limit(anatomy: &Anatomy) -> f32
{
    let strength = anatomy.strength();

    if strength <= 0.0 { return 0.0; }

    strength
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InventoryItem(usize);

pub struct ItemMutRef<'a, F: FnMut(&mut Inventory)>
{
    inventory: &'a mut Inventory,
    on_drop: F,
    id: InventoryItem
}

impl<F: FnMut(&mut Inventory)> Drop for ItemMutRef<'_, F>
{
    fn drop(&mut self)
    {
        (self.on_drop)(self.inventory);
    }
}

impl<F: FnMut(&mut Inventory)> Deref for ItemMutRef<'_, F>
{
    type Target = Item;

    fn deref(&self) -> &Self::Target
    {
        &self.inventory.items[self.id.0]
    }
}

impl<F: FnMut(&mut Inventory)> DerefMut for ItemMutRef<'_, F>
{
    fn deref_mut(&mut self) -> &mut Self::Target
    {
        &mut self.inventory.items[self.id.0]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inventory
{
    weight_limit: f32,
    weight_total: SimpleF32,
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

impl Inventory
{
    pub fn new(weight_limit: f32) -> Self
    {
        Self{weight_limit, weight_total: 0.0.into(), items: ObjectsStore::new()}
    }

    fn inventory_updated(&mut self, info: &ItemsInfo)
    {
        self.weight_total = self.items.iter().map(|(_, x)| info.get(x.id).mass).sum::<f32>().into();
    }

    pub fn set_weight_limit(&mut self, value: f32)
    {
        self.weight_limit = value;
    }

    pub fn weight_limit(&self) -> f32
    {
        self.weight_limit
    }

    pub fn weight_total(&self) -> f32
    {
        *self.weight_total
    }

    pub fn weight_fraction(&self) -> Option<f32>
    {
        if self.weight_limit == 0.0 { return None; }

        Some(*self.weight_total / self.weight_limit)
    }

    pub fn encumbrance(&self) -> f32
    {
        self.weight_fraction().map(|x|
        {
            (1.0 - (x - 1.0).max(0.0) * 2.0).max(0.0)
        }).unwrap_or(0.0)
    }

    pub fn push(&mut self, info: &ItemsInfo, item: Item) -> InventoryItem
    {
        *self.weight_total += info.get(item.id).mass;

        InventoryItem(self.items.push(item))
    }

    pub fn get(&self, id: InventoryItem) -> Option<&Item>
    {
        self.items.get(id.0)
    }

    pub fn get_mut<'a, 'b>(
        &'a mut self,
        info: &'b ItemsInfo,
        id: InventoryItem
    ) -> Option<ItemMutRef<'a, impl FnMut(&mut Self) + 'b>>
    {
        self.items.get(id.0)?;

        let on_drop = move |this: &mut Self| this.inventory_updated(info);

        Some(ItemMutRef{inventory: self, on_drop, id})
    }

    pub fn remove(&mut self, info: &ItemsInfo, id: InventoryItem) -> Option<Item>
    {
        let value = self.items.remove(id.0);

        self.inventory_updated(info);

        value
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
