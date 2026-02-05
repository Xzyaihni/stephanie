use std::{
    iter,
    ops::{Index, Deref, DerefMut}
};

use serde::{Serialize, Deserialize};

use crate::common::{
    some_or_value,
    some_or_return,
    SimpleF32,
    ObjectsStore,
    Item,
    ItemsInfo,
    Entity,
    AnyEntities,
    entity::ClientEntities
};

pub use sorter::InventorySorter;

mod sorter;


pub const BASE_INVENTORY_LIMIT: f32 = 2.0;

pub fn inventory_remove_item(entities: &ClientEntities, entity: Entity, item: InventoryItem) -> Option<Item>
{
    inventory_remove_item_with(entities, entity, item, || on_removed_item(entities, entity, item))
}

pub fn inventory_remove_item_with(
    entities: &ClientEntities,
    entity: Entity,
    item: InventoryItem,
    on_removed: impl FnOnce()
) -> Option<Item>
{
    let mut inventory = entities.inventory_mut(entity)?;

    let value = inventory.items.remove(item.0);

    on_removed();

    inventory.inventory_updated(&entities.infos().items_info);

    value
}

pub fn inventory_remove_items(
    entities: &ClientEntities,
    entity: Entity,
    items: impl Iterator<Item=InventoryItem>
)
{
    let mut inventory = some_or_return!(entities.inventory_mut(entity));

    items.for_each(|item|
    {
        inventory.items.remove(item.0);

        on_removed_item(entities, entity, item);
    });

    inventory.inventory_updated(&entities.infos().items_info);
}

pub fn damage_durability(entities: &ClientEntities, entity: Entity, id: InventoryItem) -> bool
{
    damage_durability_with(entities, entity, id, || on_removed_item(entities, entity, id))
}

pub fn damage_durability_with(
    entities: &ClientEntities,
    entity: Entity,
    id: InventoryItem,
    on_removed: impl FnOnce()
) -> bool
{
    let mut inventory = some_or_value!(entities.inventory_mut(entity), false);

    if let Some(item) = inventory.items.get_mut(id.0)
    {
        let destroyed = item.damage_durability();

        drop(inventory);

        if destroyed
        {
            inventory_remove_item_with(entities, entity, id, on_removed);
        }

        destroyed
    } else
    {
        false
    }
}

fn on_removed_item(entities: &ClientEntities, entity: Entity, item: InventoryItem)
{
    if let Some(mut character) = entities.character_mut(entity)
    {
        character.on_removed_item(item);
    }
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
        self.weight_total = self.items.iter().map(|(_, x)|
        {
            x.ammo.iter().copied().chain(iter::once(x.id)).map(|id| info.get(id).mass).sum::<f32>()
        }).sum::<f32>().into();
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
