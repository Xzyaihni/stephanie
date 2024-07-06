use std::f32;

use serde::{Serialize, Deserialize};

use crate::common::{Side2d, SeededRandom};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DamagePartial
{
    pub data: DamageType,
    pub height: DamageHeight
}

impl DamagePartial
{
    pub fn with_direction(self, side: Side2d) -> Damage
    {
        let direction = DamageDirection{
            side,
            height: self.height
        };

        Damage::new(direction, self.data)
    }
}

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

    pub fn scale(self, scale: f32) -> Self
    {
        Self{
            data: self.data.scale(scale),
            ..self
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DamageType
{
    Blunt(f32),
    Sharp{sharpness: f32, damage: f32},
    Bullet(f32)
}

impl DamageType
{
    pub fn as_flat(self) -> f32
    {
        match self
        {
            Self::Blunt(x) => x,
            Self::Sharp{damage, ..} => damage,
            Self::Bullet(x) => x
        }
    }

    pub fn scale(mut self, scale: f32) -> Self
    {
        match &mut self
        {
            Self::Blunt(x) => *x *= scale,
            Self::Sharp{damage, ..} => *damage *= scale,
            Self::Bullet(x) => *x *= scale
        }

        self
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

    pub fn from_z(z: f32) -> Self
    {
        if (0.0..0.33).contains(&z)
        {
            Self::Bottom
        } else if (0.33..0.66).contains(&z)
        {
            Self::Middle
        } else
        {
            // z bigger than 1? good luck bozo :)
            Self::Top
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
