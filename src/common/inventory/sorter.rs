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
                let this = &info.get(this.id).name;
                let other = &info.get(other.id).name;

                this.cmp(other)
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
