use std::fmt::{self, Display};

use nalgebra::Vector3;

use serde::{Serialize, Deserialize};

use strum::{IntoEnumIterator, EnumIter};

use crate::{
    client::{
        CommonTextures,
        ui_common::{GREEN_COLOR, BLUE_COLOR, ACCENT_COLOR}
    },
    common::{
        random_f32,
        watcher::*,
        collider::*,
        lazy_transform::*,
        physics::*,
        particle_creator::*,
        Transform,
        ItemInfo,
        ItemsInfo
    }
};

pub use crate::common::items_info::ItemId;


pub fn item_disappear_watcher(textures: &CommonTextures) -> Watcher
{
    let explode_info = ParticlesKind::Dust.create(textures, true, 0.0);

    Watcher{
        kind: WatcherType::Lifetime(60.0.into()),
        action: Watcher::explode_action(ExplodeInfo{
            info: ParticlesInfo{
                speed: ParticleSpeed::Random(0.1),
                position: ParticlePosition::Spread(1.0),
                ..explode_info.info
            },
            ..explode_info
        }),
        ..Default::default()
    }
}

pub fn item_physical(info: &ItemInfo) -> PhysicalProperties
{
    PhysicalProperties{
        inverse_mass: info.mass.recip(),
        ..Default::default()
    }
}

pub fn item_collider() -> ColliderInfo
{
    ColliderInfo{
        kind: ColliderType::Rectangle,
        ..Default::default()
    }
}

pub fn item_lazy_transform(
    info: &ItemInfo,
    position: Vector3<f32>,
    rotation: f32
) -> LazyTransformInfo
{
    LazyTransformInfo{
        deformation: Deformation::Stretch(StretchDeformation{
            animation: ValueAnimation::EaseOut(2.0),
            limit: 2.0,
            onset: 0.05,
            strength: 2.0
        }),
        transform: Transform{
            position,
            rotation,
            scale: info.scale3(),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, Serialize, Deserialize)]
pub enum ItemRarity
{
    Normal,
    Uncommon,
    Rare,
    Mythical
}

impl ItemRarity
{
    pub fn random() -> Self
    {
        Self::iter().find(|_|
        {
            fastrand::u32(0..5) != 0
        }).unwrap_or(Self::Mythical)
    }

    pub fn random_buffs(&self) -> Vec<ItemBuff>
    {
        if let Self::Normal = self
        {
            return Vec::new();
        }

        let damage_boost = ItemBuff::Damage(random_f32(match self
        {
            Self::Normal => unreachable!(),
            Self::Uncommon => 0.05..=0.1,
            Self::Rare => 0.1..=0.2,
            Self::Mythical => 0.2..=0.4
        }));

        let crit_boost = ItemBuff::Crit(random_f32(match self
        {
            Self::Normal => unreachable!(),
            Self::Uncommon => 0.005..=0.01,
            Self::Rare => 0.01..=0.02,
            Self::Mythical => 0.02..=0.05
        }));

        if let Self::Uncommon = self
        {
            return vec![(if fastrand::bool() { damage_boost } else { crit_boost })];
        }

        vec![damage_boost, crit_boost]
    }

    pub fn name(&self) -> Option<&'static str>
    {
        match self
        {
            Self::Normal => None,
            Self::Uncommon => Some("uncommon"),
            Self::Rare => Some("rare"),
            Self::Mythical => Some("mythical")
        }
    }

    pub fn hue_chroma(&self) -> Option<(f32, f32)>
    {
        match self
        {
            Self::Normal => None,
            Self::Uncommon => Some((GREEN_COLOR.h, GREEN_COLOR.c)),
            Self::Rare => Some((BLUE_COLOR.h, BLUE_COLOR.c)),
            Self::Mythical => Some((ACCENT_COLOR.h, ACCENT_COLOR.c + 40.0))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ItemBuff
{
    Damage(f32),
    Crit(f32)
}

impl Display for ItemBuff
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            Self::Damage(x) => write!(f, "{:+}% damage", (x * 100.0).round() as u32),
            Self::Crit(x) => write!(f, "{:+.1}% crit chance", x * 100.0)
        }
    }
}

impl ItemBuff
{
    pub fn is_positive(&self) -> bool
    {
        match self
        {
            Self::Damage(x) => *x > 0.0,
            Self::Crit(x) => *x > 0.0
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item
{
    pub rarity: ItemRarity,
    pub buffs: Vec<ItemBuff>,
    pub id: ItemId
}

impl Default for Item
{
    fn default() -> Self
    {
        Self{
            id: ItemId::from(0),
            buffs: Vec::new(),
            rarity: ItemRarity::Normal
        }
    }
}

impl Item
{
    pub fn new(info: &ItemsInfo, id: ItemId) -> Self
    {
        let rarity = if info.get(id).rarity_rolls { ItemRarity::random() } else { ItemRarity::Normal };
        Item{rarity, buffs: rarity.random_buffs(), id}
    }

    pub fn damage_scale(&self) -> Option<f32>
    {
        self.buffs.iter().find_map(|x| if let ItemBuff::Damage(x) = x { Some(x + 1.0) } else { None })
    }

    pub fn crit_chance(&self) -> Option<f32>
    {
        self.buffs.iter().find_map(|x| if let ItemBuff::Crit(x) = x { Some(*x) } else { None })
    }
}
