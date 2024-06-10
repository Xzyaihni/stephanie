use std::ops::Range;

use crate::common::{
    WeightedPicker,
    Inventory,
    Item,
    ItemsInfo
};


pub struct Loot<'a>
{
    info: &'a ItemsInfo,
    groups: Vec<&'static str>,
    rarity: f32
}

impl<'a> Loot<'a>
{
    pub fn new(
        info: &'a ItemsInfo,
        groups: Vec<&'static str>,
        rarity: f32
    ) -> Self
    {
        Self{info, groups, rarity}
    }

    pub fn create(&mut self) -> Option<Item>
    {
        let possible = self.groups.iter().flat_map(|name| self.info.group(name));

        let loot_commonness = (self.rarity as f64).recip();

        let scaled_commonness = |c: f64|
        {
            ((c - loot_commonness).exp() + 1.0).ln()
        };

        let id = WeightedPicker::pick_from(fastrand::f64(), possible, |id|
        {
            scaled_commonness(self.info.get(*id).commonness)
        });

        id.map(|&id|
        {
            Item{
                id
            }
        })
    }

    pub fn create_random(&mut self, items: &mut Inventory, amount: Range<usize>)
    {
        (0..fastrand::usize(amount)).filter_map(|_| self.create()).for_each(|item|
        {
            items.push(item);
        });
    }
}
