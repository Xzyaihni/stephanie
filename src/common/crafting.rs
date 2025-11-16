
use std::{
    fs::File,
    path::PathBuf
};

use serde::Deserialize;

use crate::common::{
    with_error,
    some_or_unexpected_return,
    some_or_value,
    some_or_return,
    damage_durability,
    inventory_remove_items,
    generic_info::*,
    Entity,
    ItemId,
    ItemTag,
    ItemsInfo,
    InventoryItem,
    Item,
    ItemRarity,
    Inventory,
    AnyEntities,
    player::StatId,
    entity::ClientEntities
};


define_info_id!{CraftId}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
enum CraftRequireRawItem
{
    WithTag(String),
    Item(String)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CraftRequireRaw
{
    item: CraftRequireRawItem,
    consume: Option<bool>
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CraftRaw
{
    name: Option<String>,
    produces: Vec<String>,
    requires: Vec<CraftRequireRaw>
}

pub fn craft_item_rarity(level: u32) -> ItemRarity
{
    let limit = 4.0;

    let value = 1.0 - (level as f32 * 0.1 + 1.0).recip();

    let value = value * limit + (fastrand::f32() * 2.0 - 1.0).powi(3) * 1.3;

    ItemRarity::from_repr((value.round() as i32).max(0) as usize).unwrap_or(ItemRarity::Mythical)
}

pub fn craft_item(entities: &ClientEntities, entity: Entity, items: Vec<CraftComponent>, craft: &Craft)
{
    {
        let mut inventory = some_or_return!(entities.inventory_mut(entity));
        let mut player = some_or_return!(entities.player_mut(entity));

        let infos = entities.infos();

        debug_assert!(items.iter().all(|x| inventory.get(x.id).is_some()));

        let average_durability = items.iter().map(|x|
        {
            inventory.get(x.id).map(|x| *x.durability / infos.items_info.get(x.id).durability).unwrap_or(0.0)
        }).sum::<f32>() / items.len() as f32;

        craft.produces.iter().copied().for_each(|id|
        {
            let crafting_stat = player.get_stat(StatId::Crafting);

            let item_info = infos.items_info.get(id);
            let rarity = if item_info.rarity_rolls
            {
                craft_item_rarity(crafting_stat.level())
            } else
            {
                ItemRarity::Normal
            };

            player.add_experience(StatId::Crafting, 5.0);

            let buffs = rarity.random_buffs();

            let item = Item{
                rarity,
                buffs,
                durability: (item_info.durability * average_durability).into(),
                id
            };

            inventory.push(&infos.items_info, item);
        });

        items.iter().filter(|x| !x.consume).for_each(|x|
        {
            damage_durability(entities, entity, x.id);
        });
    }

    inventory_remove_items(entities, entity, items.into_iter().filter_map(|x| x.consume.then_some(x.id)));
}

#[derive(Debug, Clone, Copy)]
pub struct CraftComponent
{
    pub id: InventoryItem,
    pub consume: bool
}

#[derive(Debug)]
pub enum CraftRequireItem
{
    WithTag(ItemTag),
    Item(ItemId)
}

impl CraftRequireItem
{
    pub fn fits(&self, items_info: &ItemsInfo, item: ItemId) -> bool
    {
        match self
        {
            Self::Item(x) => *x == item,
            Self::WithTag(x) => items_info.get(item).tags.contains(x)
        }
    }
}

#[derive(Debug)]
pub struct CraftRequire
{
    pub item: CraftRequireItem,
    pub consume: bool
}

#[derive(Debug)]
pub struct Craft
{
    pub name: Option<String>,
    pub produces: Vec<ItemId>,
    pub requires: Vec<CraftRequire>
}

impl Craft
{
    fn from_raw(items_info: &ItemsInfo, raw: CraftRaw) -> Option<Self>
    {
        let parse_item = |name: &str| -> Option<ItemId>
        {
            let x = items_info.get_id(name);
            if x.is_none()
            {
                eprintln!("item named `{name}` doesnt exist, ignoring");
            }

            x
        };

        let requires: Vec<CraftRequire> = raw.requires.into_iter().filter_map(|require|
        {
            let item = match require.item
            {
                CraftRequireRawItem::Item(x) => CraftRequireItem::Item(parse_item(&x)?),
                CraftRequireRawItem::WithTag(x) =>
                {
                    let tag = items_info.get_tag(&x);
                    if tag.is_none()
                    {
                        eprintln!("tag named `{x}` not found, ignoring");
                    }

                    CraftRequireItem::WithTag(tag?)
                }
            };

            Some(CraftRequire{item, consume: require.consume.unwrap_or(true)})
        }).collect();

        if requires.is_empty()
        {
            eprintln!("craft that has no requirements is invalid, ignoring");
            return None;
        }

        let produces: Vec<ItemId> = raw.produces.into_iter().filter_map(|x| parse_item(&x)).collect();

        if produces.is_empty()
        {
            eprintln!("craft that produces nothing is invalid, ignoring");
            return None;
        }

        Some(Self{
            name: raw.name,
            produces,
            requires
        })
    }

    pub fn name<'a>(&'a self, items_info: &'a ItemsInfo) -> &'a str
    {
        self.name.as_ref().unwrap_or_else(||
        {
            let first_produced = self.produces.first().expect("craft must be valid on creation");

            &items_info.get(*first_produced).name
        })
    }

    pub fn takes_items(&self, items_info: &ItemsInfo, inventory: &Inventory) -> Option<Vec<CraftComponent>>
    {
        self.requires.iter().try_fold(Vec::new(), |mut used_items: Vec<CraftComponent>, requires|
        {
            let mut possible_items = inventory.items_ids().filter(|(x, _)|
            {
                !used_items.iter().any(|check|
                {
                    if !check.consume && !requires.consume { return false; }

                    check.id == *x
                })
            });

            let used_item = possible_items.find(|(_, item)| requires.item.fits(items_info, item.id));

            if let Some(used_item) = used_item
            {
                used_items.push(CraftComponent{id: used_item.0, consume: requires.consume});

                Some(used_items)
            } else
            {
                None
            }
        })
    }

    pub fn is_possible_with(
        &self,
        items_info: &ItemsInfo,
        inventory: &Inventory,
        items: impl ExactSizeIterator<Item=InventoryItem> + Clone
    ) -> bool
    {
        debug_assert!(self.requires.len() == items.len());

        let available_start: Vec<InventoryItem> = items.clone().collect();
        self.requires.iter().zip(items).try_fold(available_start, |mut available, (requires, takes)|
        {
            if available.is_empty()
            {
                return None;
            }

            if requires.item.fits(items_info, some_or_unexpected_return!(inventory.get(takes)).id)
            {
                if requires.consume
                {
                    available.retain(|x| *x != takes);
                }

                Some(available)
            } else
            {
                None
            }
        }).is_some()
    }

    pub fn is_craftable(&self, items_info: &ItemsInfo, inventory: &Inventory) -> bool
    {
        self.takes_items(items_info, inventory).is_some()
    }
}

pub struct Crafts
{
    crafts: Vec<Craft>
}

impl Crafts
{
    pub fn empty() -> Self
    {
        Self{crafts: Vec::new()}
    }

    pub fn parse(
        items_info: &ItemsInfo,
        info: PathBuf
    ) -> Self
    {
        let info = some_or_value!(with_error(File::open(info)), Self::empty());

        let crafts: Vec<CraftRaw> = serde_json::from_reader(info).unwrap_or_else(|err|
        {
            eprintln!("error parsing crafts: {err}");

            Vec::new()
        });

        let crafts: Vec<Craft> = crafts.into_iter().filter_map(|craft|
        {
            Craft::from_raw(items_info, craft)
        }).collect();

        Self{
            crafts
        }
    }

    pub fn get(&self, id: CraftId) -> &Craft
    {
        &self.crafts[id.0]
    }

    pub fn iter(&self) -> impl Iterator<Item=&Craft>
    {
        self.crafts.iter()
    }

    pub fn ids(&self) -> impl Iterator<Item=CraftId>
    {
        (0..self.crafts.len()).map(CraftId)
    }
}
