use serde::{Serialize, Deserialize};

use crate::common::{ItemsInfo, Item};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Order
{
    Alphabetical
}

impl Order
{
    pub fn before(
        &self,
        info: &ItemsInfo,
        this: &Item,
        other: &Item
    ) -> bool
    {
        match self
        {
            Self::Alphabetical =>
            {
                let this = &info.get(this.id).name;
                let other = &info.get(other.id).name;

                this < other
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inventory
{
    order: Order,
    items: Vec<Item>
}

impl Inventory
{
    pub fn new() -> Self
    {
        Self{order: Order::Alphabetical, items: Vec::new()}
    }

    pub fn push(&mut self, items_info: &ItemsInfo, item: Item)
    {
        let index = self.items.partition_point(|x| self.order.before(items_info, x, &item));

        self.items.insert(index, item);
    }

    pub fn items(&self) -> &[Item]
    {
        &self.items
    }
}
