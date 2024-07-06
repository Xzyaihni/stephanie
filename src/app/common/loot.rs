use std::ops::Range;

use crate::common::{
    pick_by_commonness,
    Inventory,
    Item,
    ItemsInfo
};


pub struct Loot<'a>
{
    info: &'a ItemsInfo,
    groups: Vec<&'static str>,
    commonness: f32
}

impl<'a> Loot<'a>
{
    pub fn new(
        info: &'a ItemsInfo,
        groups: Vec<&'static str>,
        commonness: f32
    ) -> Self
    {
        Self{info, groups, commonness}
    }

    pub fn create(&mut self) -> Option<Item>
    {
        let possible = self.groups.iter().flat_map(|name| self.info.group(name));

        let id = pick_by_commonness(self.commonness as f64, possible, |id|
        {
            self.info.get(*id).commonness
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

#[cfg(test)]
mod tests
{
    use std::{iter, collections::HashMap};

    use crate::common::pick_by_commonness;


    fn distribution(this_commonness: f64)
    {
        let items = [
            ("bag", 1.2),
            ("rock", 1.0),
            ("stick", 0.7),
            ("bottle", 0.5),
            ("three", 0.33),
            ("gem", 0.1)
        ];

        let commonnest = items.iter().map(|(_, x)| x).max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();

        let items = items.into_iter().map(|(name, x)| (name, x / commonnest)).collect::<Vec<_>>();

        let longest_item = items.iter().map(|(name, _)| name.len()).max().unwrap();

        let mut gotten: HashMap<&str, (usize, f64)> = items.iter()
            .map(|(name, commonness)| (*name, (0_usize, *commonness)))
            .collect();

        let trials = 1500;

        (0..trials).for_each(|_|
        {
            let picked = pick_by_commonness(this_commonness, items.iter(), |(_, commonness)|
            {
                *commonness
            });

            gotten.get_mut(picked.unwrap().0).unwrap().0 += 1;
        });

        let largest_amount = *gotten.iter().map(|(_, (amount, _))| amount).max().unwrap();

        let mut sorted: Vec<(&str, (usize, f64))> = gotten.into_iter().collect();
        sorted.sort_unstable_by(|a, b| a.1.1.partial_cmp(&b.1.1).unwrap());

        let bar_size = 25;

        println!("drops distribution for commonness {this_commonness}");
        sorted.into_iter().for_each(|(name, (amount, _))|
        {
            let bar: String = {
                let amount = amount as f32 / largest_amount as f32;

                (0..bar_size).map(|i|
                {
                    let fraction = i as f32 / (bar_size - 1) as f32;

                    if fraction <= amount
                    {
                        '#'
                    } else
                    {
                        '-'
                    }
                }).collect()
            };

            let rate = (amount as f32 / trials as f32) * 100.0;

            println!("{name:>longest_item$}: {bar} {rate:.1}%");
        });
        println!(
            "{}",
            iter::repeat('=').take(longest_item + bar_size + 8).collect::<String>()
        );
    }

    #[test]
    fn run_distributions()
    {
        [
            0.01,
            0.2,
            0.5,
            1.0,
            2.0,
            4.0,
            9.0,
            30.0,
            60.0,
            99.0
        ].into_iter().for_each(distribution);
    }
}
