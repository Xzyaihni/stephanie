use std::{
    fs::File,
    path::Path
};

use nalgebra::{Vector2, Vector3};

use serde::Deserialize;

use yanyaengine::Assets;

use crate::{
    client::game_state::UsageKind,
    common::{
        with_z,
        ENTITY_SCALE,
        generic_info::*,
        Drug,
        DamageType,
        Item,
        Light
    }
};


pub const DEFAULT_ITEM_DURABILITY: f32 = 25.0;

define_info_id!{ItemId}

#[derive(Debug, Clone, Deserialize)]
pub enum Ranged
{
    Pistol{cooldown: f32, damage: f32}
}

impl Ranged
{
    pub fn piercing(&self) -> bool
    {
        match self
        {
            Self::Pistol{..} => true
        }
    }

    pub fn cooldown(&self) -> f32
    {
        match self
        {
            Self::Pistol{cooldown, ..} => *cooldown
        }
    }

    pub fn damage(&self) -> DamageType
    {
        let with_base = |base, value|
        {
            let damage = base * value;

            let spread = fastrand::f32() * damage * 0.05;

            damage + spread
        };

        match self
        {
            Self::Pistol{damage, ..} =>
            {
                DamageType::Bullet(with_base(10.0, damage))
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemInfoRaw
{
    name: String,
    ranged: Option<Ranged>,
    drug: Option<Drug>,
    rarity_rolls: Option<bool>,
    damage_scale: Option<f32>,
    comfort: Option<f32>,
    sharpness: Option<f32>,
    side_sharpness: Option<f32>,
    mass: Option<f32>,
    durability: Option<f32>,
    lighting: Option<f32>,
    texture: Option<String>
}

pub type ItemsInfoRaw = Vec<ItemInfoRaw>;

#[derive(Debug, Clone)]
pub struct ItemInfo
{
    pub name: String,
    pub ranged: Option<Ranged>,
    pub drug: Option<Drug>,
    pub rarity_rolls: bool,
    pub damage_scale: f32,
    pub comfort: f32,
    pub sharpness: f32,
    pub side_sharpness: f32,
    pub mass: f32,
    pub durability: f32,
    pub lighting: Light,
    pub texture: Sprite
}

impl GenericItem for ItemInfo
{
    fn name(&self) -> String
    {
        self.name.clone()
    }
}

impl ItemInfo
{
    fn from_raw(
        assets: Option<&Assets>,
        textures_root: &Path,
        raw: ItemInfoRaw
    ) -> Self
    {
        let texture_name = raw.texture.unwrap_or_else(||
        {
            raw.name.clone()
        });

        let texture = assets.map(|assets|
        {
            load_texture(assets, textures_root, &texture_name)
        }).unwrap_or_else(||
        {
            Sprite{id: 0.into(), scale: Vector2::repeat(ENTITY_SCALE)}
        });

        Self{
            name: raw.name,
            ranged: raw.ranged,
            drug: raw.drug,
            rarity_rolls: raw.rarity_rolls.unwrap_or(true),
            damage_scale: raw.damage_scale.unwrap_or(1.0),
            comfort: raw.comfort.unwrap_or(1.0),
            sharpness: raw.sharpness.unwrap_or(0.0),
            side_sharpness: raw.side_sharpness.unwrap_or(0.0),
            mass: raw.mass.unwrap_or(1.0),
            durability: raw.durability.unwrap_or(1.0) * DEFAULT_ITEM_DURABILITY,
            lighting: raw.lighting.map(|strength| Light{strength, ..Default::default()}).unwrap_or_default(),
            texture
        }
    }

    pub fn with_changed(mut self, mut f: impl FnMut(&mut Self)) -> Self
    {
        f(&mut self);

        self
    }

    fn damage_base(&self) -> f32
    {
        self.mass * self.damage_scale
    }

    pub fn bash_damage(&self) -> DamageType
    {
        if self.side_sharpness == 0.0
        {
            DamageType::Blunt(self.damage_base())
        } else
        {
            DamageType::Sharp{sharpness: self.side_sharpness, damage: self.damage_base()}
        }
    }

    pub fn poke_damage(&self) -> DamageType
    {
        if self.sharpness == 0.0
        {
            DamageType::Blunt(self.damage_base() * 0.5)
        } else
        {
            DamageType::Sharp{sharpness: self.sharpness, damage: self.damage_base()}
        }
    }

    pub fn oxygen_cost(&self, strength: f32) -> f32
    {
        let raw_use = self.mass / strength * 4.0;

        0.3 + raw_use / self.comfort
    }

    pub fn aspect(&self) -> Vector2<f32>
    {
        self.texture.aspect()
    }

    pub fn scale_scalar(&self) -> f32
    {
        self.texture.scale.max() / ENTITY_SCALE
    }

    pub fn scale3(&self) -> Vector3<f32>
    {
        with_z(self.texture.scale, ENTITY_SCALE * 0.1)
    }

    pub fn usage(&self) -> Option<UsageKind>
    {
        if self.drug.is_some()
        {
            return Some(UsageKind::Ingest);
        }

        None
    }
}

pub struct ItemsInfo
{
    generic_info: GenericInfo<ItemId, ItemInfo>
}

impl ItemsInfo
{
    pub fn empty() -> Self
    {
        let generic_info = GenericInfo::new(Vec::new());

        Self{generic_info}
    }

    pub fn parse(
        assets: Option<&Assets>,
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

        let generic_info = GenericInfo::new(items);

        Self{generic_info}
    }

    pub fn id(&self, name: &str) -> ItemId
    {
        self.generic_info.id(name)
    }

    pub fn get_id(&self, name: &str) -> Option<ItemId>
    {
        self.generic_info.get_id(name)
    }

    pub fn get(&self, id: ItemId) -> &ItemInfo
    {
        self.generic_info.get(id)
    }

    pub fn items(&self) -> &[ItemInfo]
    {
        self.generic_info.items()
    }

    pub fn random(&self) -> Item
    {
        let id = ItemId(fastrand::usize(0..self.generic_info.items().len()));

        Item::new(self, id)
    }
}
