use std::{
    f32,
    ops::{Mul, MulAssign},
    fmt::{self, Debug}
};

use serde::{Serialize, Deserialize};

use crate::common::Side2d;


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

#[derive(Clone, Serialize, Deserialize)]
pub struct Damage
{
    pub data: DamageType,
    pub direction: DamageDirection
}

impl Debug for Damage
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        f.debug_struct("Damage")
            .field("data", &self.data)
            .field("direction", &self.direction)
            .finish()
    }
}

impl Mul<f32> for Damage
{
    type Output = Self;

    fn mul(self, scale: f32) -> Self
    {
        Self{
            data: self.data * scale,
            ..self
        }
    }
}

impl Damage
{
    pub fn new(direction: DamageDirection, data: DamageType) -> Self
    {
        Self{data, direction}
    }

    pub fn area_each(amount: f32) -> Self
    {
        Self{data: DamageType::AreaEach(amount), direction: DamageDirection::default()}
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DamageType
{
    AreaEach(f32),
    Blunt(f32),
    Sharp{sharpness: f32, damage: f32},
    Bullet(f32)
}

impl Mul<f32> for DamageType
{
    type Output = Self;

    fn mul(mut self, scale: f32) -> Self
    {
        self *= scale;

        self
    }
}

impl MulAssign<f32> for DamageType
{
    fn mul_assign(&mut self, scale: f32)
    {
        match self
        {
            Self::AreaEach(x) => *x *= scale,
            Self::Blunt(x) => *x *= scale,
            Self::Sharp{damage, ..} => *damage *= scale,
            Self::Bullet(x) => *x *= scale
        }
    }
}

impl DamageType
{
    pub fn is_piercing(&self) -> bool
    {
        match self
        {
            Self::AreaEach(_) => false,
            Self::Blunt(_) => false,
            Self::Sharp{..} => true,
            Self::Bullet(_) => true
        }
    }

    pub fn as_flat(self) -> f32
    {
        match self
        {
            Self::AreaEach(x) => x,
            Self::Blunt(x) => x,
            Self::Sharp{damage, ..} => damage,
            Self::Bullet(x) => x
        }
    }

    pub fn as_ranged_pierce(self) -> f32
    {
        self.as_flat() * 0.0001
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DamageHeight
{
    Top,
    Middle,
    Bottom
}

impl Default for DamageHeight
{
    fn default() -> Self
    {
        Self::Middle
    }
}

impl DamageHeight
{
    pub fn random() -> Self
    {
        fastrand::choice([Self::Middle, Self::Middle, Self::Bottom, Self::Bottom, Self::Top]).unwrap()
    }

    pub fn from_z(z: f32) -> Self
    {
        debug_assert!(z <= 1.0);

        if (0.0..0.33).contains(&z)
        {
            Self::Bottom
        } else if (0.33..0.66).contains(&z)
        {
            Self::Middle
        } else
        {
            Self::Top
        }
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct DamageDirection
{
    pub side: Side2d,
    pub height: DamageHeight
}

pub trait Damageable
{
    fn damage(&mut self, damage: Damage) -> Option<Damage>;
    fn fall_damage(&mut self, damage: f32);

    fn is_full(&self) -> bool;
    fn heal(&mut self, amount: f32) -> Option<f32>;
}
