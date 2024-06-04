use std::f32;

use serde::{Serialize, Deserialize};

use crate::common::{Side2d, SeededRandom};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Damage
{
    pub rng: SeededRandom,
    pub data: DamageType,
    pub direction: DamageDirection
}

impl Damage
{
    pub fn new(direction: DamageDirection, data: DamageType) -> Self
    {
        Self{rng: SeededRandom::new(), data, direction}
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DamageType
{
    Blunt(f32),
    Bullet(f32)
}

impl DamageType
{
    pub fn as_flat(self) -> f32
    {
        match self
        {
            Self::Blunt(x) => x,
            Self::Bullet(x) => x
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DamageHeight
{
    Top,
    Middle,
    Bottom
}

impl DamageHeight
{
    pub fn random() -> Self
    {
        match fastrand::u32(0..3)
        {
            0 => Self::Top,
            1 => Self::Middle,
            2 => Self::Bottom,
            _ => unreachable!()
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DamageDirection
{
    pub side: Side2d,
    pub height: DamageHeight
}

pub trait Damageable
{
    fn damage(&mut self, damage: Damage) -> Option<DamageType>;
}
