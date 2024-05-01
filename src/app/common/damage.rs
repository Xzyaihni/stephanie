use std::f32;

use nalgebra::Vector3;

use serde::{Serialize, Deserialize};

use crate::common::SeededRandom;


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
    Bullet(f32)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Side2d
{
    Left,
    Right,
    Front,
    Back
}

impl Side2d
{
    pub fn from_positions(rotation: f32, origin: Vector3<f32>, other: Vector3<f32>) -> Self
    {
        let offset = other - origin;

        Self::from_angle(offset.y.atan2(offset.x) - rotation)
    }

    pub fn from_angle(angle: f32) -> Self
    {
        const HALF: f32 = f32::consts::FRAC_PI_2;
        const QUARTER: f32 = f32::consts::FRAC_PI_4;

        if (-QUARTER..QUARTER).contains(&angle)
        {
            Self::Front
        } else if ((-HALF - QUARTER)..-QUARTER).contains(&angle)
        {
            Self::Left
        } else if (QUARTER..(HALF + QUARTER)).contains(&angle)
        {
            Self::Right
        } else
        {
            Self::Back
        }
    }

    pub fn opposite(self) -> Self
    {
        match self
        {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
            Self::Front => Self::Back,
            Self::Back => Self::Front
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
    fn damage(&mut self, damage: Damage);
}
