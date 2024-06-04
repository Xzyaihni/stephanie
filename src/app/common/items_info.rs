use std::{
    fs::File,
    path::{Path, PathBuf},
    collections::HashMap
};

use nalgebra::{Vector2, Vector3};

use serde::{Serialize, Deserialize};

use yanyaengine::{Assets, TextureId};

use crate::common::{DamageType, Item};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemId(usize);

#[derive(Deserialize)]
pub enum Weapon
{
    Melee{comfort: f32, sharpness: f32},
    Pistol{damage: f32}
}

impl Default for Weapon
{
    fn default() -> Self
    {
        Self::Melee{comfort: 1.0, sharpness: 0.0}
    }
}

impl Weapon
{
    pub fn piercing(&self) -> bool
    {
        match self
        {
            Self::Melee{..} => false,
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
            Self::Melee{comfort, sharpness} =>
            {
                todo!()
            },
            Self::Pistol{damage} =>
            {
                DamageType::Bullet(with_base(400.0, damage))
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemInfoRaw
{
    name: String,
    #[serde(default)]
    weapon: Weapon,
    scale: Option<f32>,
    mass: Option<f32>,
    commonness: Option<f64>,
    groups: Vec<String>,
    texture: Option<String>
}

pub type ItemsInfoRaw = Vec<ItemInfoRaw>;

pub struct ItemInfo
{
    pub name: String,
    pub weapon: Weapon,
    pub scale: Vector2<f32>,
    pub mass: f32,
    pub commonness: f64,
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

        let texture_name = raw.texture.unwrap_or_else(||
        {
            let folder: String = raw.groups.first().cloned().unwrap_or_default();

            let name = raw.name.replace(' ', "_") + ".png";

            let path = PathBuf::from(folder).join(name);

            path.to_string_lossy().into_owned()
        });

        let texture = get_texture(texture_name);

        let aspect = assets.texture(texture).read().aspect_min();

        Self{
            name: raw.name,
            weapon: raw.weapon,
            // scale is in meters
            scale: aspect * raw.scale.unwrap_or(0.1) * 3.0,
            mass: raw.mass.unwrap_or(1.0),
            commonness: raw.commonness.unwrap_or(1.0),
            texture
        }
    }

    pub fn scale3(&self) -> Vector3<f32>
    {
        self.scale.xyx()
    }
}

pub struct ItemsInfo
{
    mapping: HashMap<String, ItemId>,
    items: Vec<ItemInfo>,
    groups: HashMap<String, Vec<ItemId>>
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

        let mut groups: HashMap<String, Vec<ItemId>> = HashMap::new();

        let textures_root = textures_root.as_ref();
        let items: Vec<_> = items.into_iter().enumerate().map(|(index, info_raw)|
        {
            let id = ItemId(index);

            info_raw.groups.iter().for_each(|group|
            {
                groups.entry(group.clone())
                    .and_modify(|x| { x.push(id); })
                    .or_insert(vec![id]);
            });

            ItemInfo::from_raw(assets, textures_root, info_raw)
        }).collect();

        let mapping = items.iter().enumerate().map(|(index, item)|
        {
            (item.name.clone(), ItemId(index))
        }).collect();

        Self{mapping, items, groups}
    }

    pub fn id(&self, name: &str) -> ItemId
    {
        self.mapping[name]
    }

    pub fn get(&self, id: ItemId) -> &ItemInfo
    {
        &self.items[id.0]
    }

    pub fn items(&self) -> &[ItemInfo]
    {
        &self.items
    }

    pub fn group(&self, name: &str) -> &[ItemId]
    {
        &self.groups[name]
    }

    pub fn random(&self) -> Item
    {
        let id = ItemId(fastrand::usize(0..self.items.len()));

        Item{id}
    }
}
