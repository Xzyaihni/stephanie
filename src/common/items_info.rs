use std::{
    fs::File,
    path::{Path, PathBuf},
    collections::HashMap
};

use nalgebra::{Vector2, Vector3};

use serde::Deserialize;

use yanyaengine::{Assets, TextureId};

use crate::{
    client::game_state::UsageKind,
    common::{
        lerp,
        generic_info::*,
        character::HAND_SCALE,
        Drug,
        DamageType,
        Item,
        Light
    }
};


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
    comfort: Option<f32>,
    sharpness: Option<f32>,
    side_sharpness: Option<f32>,
    scale: Option<f32>,
    mass: Option<f32>,
    lighting: Option<f32>,
    groups: Vec<String>,
    texture: Option<String>
}

pub type ItemsInfoRaw = Vec<ItemInfoRaw>;

#[derive(Debug, Clone)]
pub struct ItemInfo
{
    pub name: String,
    pub ranged: Option<Ranged>,
    pub drug: Option<Drug>,
    pub comfort: f32,
    pub sharpness: f32,
    pub side_sharpness: f32,
    pub scale: f32,
    pub aspect: Vector2<f32>,
    pub mass: f32,
    pub lighting: Light,
    pub texture: Option<TextureId>
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
        assets: &Assets,
        textures_root: &Path,
        raw: ItemInfoRaw
    ) -> Self
    {
        let texture_name = raw.texture.unwrap_or_else(||
        {
            let folder: String = raw.groups.first().cloned().unwrap_or_default();

            let path = PathBuf::from(folder).join(&raw.name);

            path.to_string_lossy().into_owned()
        });

        let texture = load_texture(assets, textures_root, &texture_name);

        let aspect = assets.texture(texture).lock().aspect_min();

        let scale = raw.scale.unwrap_or(0.1) * 4.0;

        Self{
            name: raw.name,
            ranged: raw.ranged,
            drug: raw.drug,
            comfort: raw.comfort.unwrap_or(1.0),
            sharpness: raw.sharpness.unwrap_or(0.0),
            side_sharpness: raw.side_sharpness.unwrap_or(0.0),
            // scale is in meters
            scale,
            aspect,
            mass: raw.mass.unwrap_or(1.0),
            lighting: raw.lighting.map(|strength| Light{strength, ..Default::default()}).unwrap_or_default(),
            texture: Some(texture)
        }
    }

    pub fn hand() -> Self
    {
        Self{
            name: "hand".to_owned(),
            ranged: None,
            drug: None,
            comfort: 2.0,
            sharpness: 0.0,
            side_sharpness: 0.0,
            scale: HAND_SCALE,
            aspect: Vector2::repeat(1.0),
            mass: 0.05, // 0.3 would be more accurate but i want balance
            lighting: Light::default(),
            texture: None
        }
    }

    pub fn with_changed(mut self, mut f: impl FnMut(&mut Self)) -> Self
    {
        f(&mut self);

        self
    }

    fn damage_base(&self) -> f32
    {
        self.mass
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

    pub fn scale3(&self) -> Vector3<f32>
    {
        (self.aspect * lerp(self.scale, 1.0, 0.2)).xyx()
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
    generic_info: GenericInfo<ItemId, ItemInfo>,
    groups: HashMap<String, Vec<ItemId>>
}

impl ItemsInfo
{
    pub fn empty() -> Self
    {
        let generic_info = GenericInfo::new(Vec::new());
        let groups = HashMap::new();

        Self{generic_info, groups}
    }

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

        let generic_info = GenericInfo::new(items);

        Self{generic_info, groups}
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

    pub fn group(&self, name: &str) -> &[ItemId]
    {
        self.groups.get(name).map(|x|
        {
            let items: &[_] = x.as_ref();

            items
        }).unwrap_or_else(||
        {
            eprintln!("group named `{name}` doesnt exist");

            &[]
        })
    }

    pub fn random(&self) -> Item
    {
        let id = ItemId(fastrand::usize(0..self.generic_info.items().len()));

        Item{id}
    }
}
