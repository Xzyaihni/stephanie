use std::{
    fs::File,
    path::PathBuf
};

use serde::Deserialize;

use crate::common::{
    with_error,
    some_or_value,
    generic_info::*,
    ItemId,
    ItemTag,
    ItemsInfo
};

define_info_id!{CraftId}

#[derive(Deserialize)]
enum CraftRequireRaw
{
    WithTag(String),
    Item(String)
}

#[derive(Deserialize)]
struct CraftRaw
{
    produces: Vec<String>,
    requires: Vec<CraftRequireRaw>
}

pub enum CraftRequire
{
    WithTag(ItemTag),
    Item(ItemId)
}

pub struct Craft
{
    pub produces: Vec<ItemId>,
    pub requires: Vec<CraftRequire>
}

impl Craft
{
    fn from_raw(items_info: &ItemsInfo, raw: CraftRaw) -> Self
    {
        let parse_item = |name: &str| -> Option<ItemId>
        {
            let x = items_info.get_id(&name);
            if x.is_none()
            {
                eprintln!("item named `{name}` doesnt exist, ignoring");
            }

            x
        };

        let requires = raw.requires.into_iter().filter_map(|require|
        {
            match require
            {
                CraftRequireRaw::Item(x) => Some(CraftRequire::Item(parse_item(&x)?)),
                CraftRequireRaw::WithTag(x) =>
                {
                    let tag = items_info.get_tag(&x);
                    if tag.is_none()
                    {
                        eprintln!("tag named `{x}` not found, ignoring");
                    }

                    Some(CraftRequire::WithTag(tag?))
                }
            }
        }).collect();

        Self{
            produces: raw.produces.into_iter().filter_map(|x| parse_item(&x)).collect(),
            requires
        }
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

        let crafts: Vec<Craft> = crafts.into_iter().map(|craft|
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
