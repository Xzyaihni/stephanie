use std::{
    fs::File,
    path::Path,
    collections::HashMap
};

use serde::{Serialize, Deserialize};

use yanyaengine::{Assets, TextureId};

use crate::common::{DamageType, Item};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemId(usize);

#[derive(Deserialize)]
pub enum Weapon
{
    Blunt{damage: f32},
    Pistol{damage: f32}
}

impl Default for Weapon
{
    fn default() -> Self
    {
        Self::Blunt{damage: 1.0}
    }
}

impl Weapon
{
    pub fn piercing(&self) -> bool
    {
        match self
        {
            Self::Blunt{..} => false,
            Self::Pistol{..} => true
        }
    }

    pub fn damage(&self) -> DamageType
    {
        let with_base = |base, value|
        {
            let damage = base * value;

            let spread = fastrand::f32() * damage * 0.05;

            damage * spread
        };

        match self
        {
            Self::Blunt{damage} =>
            {
                DamageType::Blunt(with_base(5.0, damage))
            },
            Self::Pistol{damage} =>
            {
                DamageType::Bullet(with_base(400.0, damage))
            }
        }
    }
}

#[derive(Deserialize)]
pub struct ItemInfoRaw
{
    name: String,
    #[serde(default)]
    weapon: Weapon,
    texture: String
}

pub type ItemsInfoRaw = Vec<ItemInfoRaw>;

pub struct ItemInfo
{
    pub name: String,
    pub weapon: Weapon,
    pub texture: TextureId
}

impl ItemInfo
{
    fn from_raw(
        assets: &Assets,
        textures_root: &Path,
        raw: ItemInfoRaw
    ) -> Self
    {
        let get_texture = |name|
        {
            let path = textures_root.join(name);
            let name = path.to_string_lossy();

            assets.texture_id(&name)
        };

        Self{
            name: raw.name,
            weapon: raw.weapon,
            texture: get_texture(raw.texture)
        }
    }
}

pub struct ItemsInfo
{
    mapping: HashMap<String, ItemId>,
    items: Vec<ItemInfo>
}

impl ItemsInfo
{
    pub fn parse(
        assets: &Assets,
        textures_root: impl AsRef<Path>,
        info: impl AsRef<Path>
    ) -> Self
    {
        let info = File::open(info.as_ref()).unwrap();

        let items: ItemsInfoRaw = serde_json::from_reader(info).unwrap();

        let textures_root = textures_root.as_ref();
        let items: Vec<_> = items.into_iter().map(|info_raw|
        {
            ItemInfo::from_raw(assets, textures_root, info_raw)
        }).collect();

        let mapping = items.iter().enumerate().map(|(index, item)|
        {
            (item.name.clone(), ItemId(index))
        }).collect();

        Self{mapping, items}
    }

    pub fn id(&self, name: &str) -> ItemId
    {
        self.mapping[name]
    }

    pub fn get(&self, id: ItemId) -> &ItemInfo
    {
        &self.items[id.0]
    }

    pub fn random(&self) -> Item
    {
        let id = ItemId(fastrand::usize(0..self.items.len()));

        Item{id}
    }
}
