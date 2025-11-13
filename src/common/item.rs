use std::fmt::{self, Display};

use nalgebra::Vector3;

use serde::{Serialize, Deserialize};

use strum::{IntoEnumIterator, EnumIter, FromRepr, EnumCount};

use crate::{
    client::{
        CommonTextures,
        ui_common::{RED_COLOR, GREEN_COLOR, BLUE_COLOR, ACCENT_COLOR}
    },
    common::{
        random_f32,
        watcher::*,
        collider::*,
        lazy_transform::*,
        physics::*,
        particle_creator::*,
        TILE_SIZE,
        SimpleF32,
        Transform,
        ItemInfo,
        ItemsInfo,
        items_info::DEFAULT_ITEM_DURABILITY,
        colors::Lcha
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
        connection: Connection::Constant{speed: TILE_SIZE * 0.3},
        scaling: Scaling::EaseOut{decay: 10.0},
        ..Default::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumCount, EnumIter, FromRepr, Serialize, Deserialize)]
pub enum ItemRarity
{
    Broken = 0,
    Normal,
    Uncommon,
    Rare,
    Mythical
}

impl ItemRarity
{
    pub fn random() -> Self
    {
        Self::iter().skip(1).find(|_|
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

            let durability_boost = ItemBuff::Durability((-random_f32(match self
            {
                Self::Broken => -0.3..=-0.1,
                Self::Normal => unreachable!(),
                Self::Uncommon => 0.1..=0.2,
                Self::Rare => 0.2..=0.3,
                Self::Mythical => 0.3..=0.5,
            })).into());

            let damage_boost = ItemBuff::Damage(random_f32(match self
            {
                Self::Broken => -0.1..=-0.05,
                Self::Normal => unreachable!(),
                Self::Uncommon => 0.05..=0.1,
                Self::Rare => 0.1..=0.2,
                Self::Mythical => 0.2..=0.4
            }).into());

            let crit_boost = ItemBuff::Crit(random_f32(match self
            {
                Self::Broken => -0.01..=-0.005,
                Self::Normal => unreachable!(),
                Self::Uncommon => 0.005..=0.01,
                Self::Rare => 0.01..=0.02,
                Self::Mythical => 0.02..=0.05
            }).into());

            let amount = match self
            {
                Self::Broken => 1,
                Self::Normal => unreachable!(),
                Self::Uncommon => 1,
                Self::Rare => 2,
                Self::Mythical => 3
            };

            (0..amount).scan(vec![durability_boost, damage_boost, crit_boost], |possible_boosts, _|
            {
                if possible_boosts.is_empty()
                {
                    return None;
                }

                let i = fastrand::usize(0..possible_boosts.len());

                Some(possible_boosts.swap_remove(i))
            }).collect()
        }

    pub fn name(&self) -> Option<&'static str>
    {
        match self
        {
            Self::Broken => Some("broken"),
            Self::Normal => None,
            Self::Uncommon => Some("uncommon"),
            Self::Rare => Some("rare"),
            Self::Mythical => Some("mythical")
        }
    }

    pub fn color(&self) -> Option<Lcha>
    {
        match self
        {
            Self::Broken => Some(Lcha{l: 50.0, c: 10.0, h: RED_COLOR.h, a: 1.0}),
            Self::Normal => None,
            Self::Uncommon => Some(GREEN_COLOR),
            Self::Rare => Some(BLUE_COLOR),
            Self::Mythical => Some(Lcha{c: ACCENT_COLOR.c + 40.0, ..ACCENT_COLOR})
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemBuff
{
    Durability(SimpleF32),
    Damage(SimpleF32),
    Crit(SimpleF32)
}

impl Display for ItemBuff
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            Self::Durability(x) => write!(f, "{:+.1}% durability use", **x * 100.0),
            Self::Damage(x) => write!(f, "{:+.1}% damage", **x * 100.0),
            Self::Crit(x) => write!(f, "{:+.1}% crit chance", **x * 100.0)
        }
    }
}

impl ItemBuff
{
    pub fn is_positive(&self) -> bool
    {
        match self
        {
            Self::Durability(x) => **x < 0.0,
            Self::Damage(x) => **x > 0.0,
            Self::Crit(x) => **x > 0.0
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Item
{
    pub rarity: ItemRarity,
    pub buffs: Vec<ItemBuff>,
    pub durability: SimpleF32,
    pub id: ItemId
}

impl Default for Item
{
    fn default() -> Self
    {
        Self{
            id: ItemId::from(0),
            durability: DEFAULT_ITEM_DURABILITY.into(),
            buffs: Vec::new(),
            rarity: ItemRarity::Normal
        }
    }
}

impl Item
{
    pub fn new(info: &ItemsInfo, id: ItemId) -> Self
    {
        let info = info.get(id);
        let rarity = if info.rarity_rolls { ItemRarity::random() } else { ItemRarity::Normal };

        Item{rarity, buffs: rarity.random_buffs(), durability: info.durability.into(), id}
    }

    pub fn damage_durability(&mut self) -> bool
    {
        let amount = self.durability_scale().unwrap_or(1.0);

        *self.durability -= amount;

        *self.durability <= 0.0
    }

    pub fn durability_scale(&self) -> Option<f32>
    {
        self.buffs.iter().find_map(|x| if let ItemBuff::Durability(x) = x { Some(**x + 1.0) } else { None })
    }

    pub fn damage_scale(&self) -> Option<f32>
    {
        self.buffs.iter().find_map(|x| if let ItemBuff::Damage(x) = x { Some(**x + 1.0) } else { None })
    }

    pub fn crit_chance(&self) -> Option<f32>
    {
        self.buffs.iter().find_map(|x| if let ItemBuff::Crit(x) = x { Some(**x) } else { None })
    }
}
