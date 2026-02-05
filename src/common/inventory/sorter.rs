use std::cmp::Ordering;

use crate::common::{ItemsInfo, Item};


#[derive(Debug, Clone)]
pub enum Order
{
    Alphabetical
}

impl Default for Order
{
    fn default() -> Self
    {
        Self::Alphabetical
    }
}

impl Order
{
    pub fn order(
        &self,
        info: &ItemsInfo,
        this: &Item,
        other: &Item
    ) -> Ordering
    {
        match self
        {
            Self::Alphabetical =>
            {
                let ord = info.get(this.id).name.cmp(&info.get(other.id).name);

                if let Ordering::Equal = ord
                {
                    let rarity_ord = this.rarity.cmp(&other.rarity);

                    if let Ordering::Equal = rarity_ord
                    {
                        this.durability.cmp(&other.durability)
                    } else
                    {
                        rarity_ord
                    }
                } else
                {
                    ord
                }
            }
        }
    }
}

#[derive(Clone, Default)]
pub struct InventorySorter
{
    order: Order
}

impl InventorySorter
{
    pub fn order(&self, info: &ItemsInfo, a: &Item, b: &Item) -> Ordering
    {
        self.order.order(info, a, b)
    }
}
