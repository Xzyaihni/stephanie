use std::f32;

use serde::{Serialize, Deserialize};

use strum::{EnumCount, FromRepr, IntoStaticStr};

use nalgebra::Vector3;

use crate::common::short_rotation;


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, IntoStaticStr, Serialize, Deserialize)]
pub enum Side1d
{
    Left,
    Right
}

impl TryFrom<Side3d> for Side1d
{
    type Error = ();

    fn try_from(side: Side3d) -> Result<Self, ()>
    {
        match side
        {
            Side3d::Left => Ok(Self::Left),
            Side3d::Right => Ok(Self::Right),
            _ => Err(())
        }
    }
}

impl TryFrom<Side2d> for Side1d
{
    type Error = ();

    fn try_from(side: Side2d) -> Result<Self, ()>
    {
        match side
        {
            Side2d::Left => Ok(Self::Left),
            Side2d::Right => Ok(Self::Right),
            _ => Err(())
        }
    }
}

impl Side1d
{
    pub fn opposite(self) -> Self
    {
        match self
        {
            Self::Left => Self::Right,
            Self::Right => Self::Left
        }
    }

    #[allow(dead_code)]
    pub fn to_angle(self) -> f32
    {
        match self
        {
            Self::Left => f32::consts::PI,
            Self::Right => 0.0
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Side2d
{
    Left,
    Right,
    Front,
    Back
}

impl From<Side1d> for Side2d
{
    fn from(side: Side1d) -> Self
    {
        match side
        {
            Side1d::Left => Self::Left,
            Side1d::Right => Self::Right
        }
    }
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
        let angle = short_rotation(angle);

        const HALF: f32 = f32::consts::FRAC_PI_2;
        const QUARTER: f32 = f32::consts::FRAC_PI_4;

        if (-QUARTER..QUARTER).contains(&angle)
        {
            Self::Front
        } else if ((-HALF - QUARTER)..-QUARTER).contains(&angle)
        {
            Self::Right
        } else if (QUARTER..(HALF + QUARTER)).contains(&angle)
        {
            Self::Left
        } else
        {
            Self::Back
        }
    }

    pub fn to_angle(self) -> f32
    {
        match self
        {
            Self::Right => -f32::consts::FRAC_PI_2,
            Self::Front => 0.0,
            Self::Left => f32::consts::FRAC_PI_2,
            Self::Back => -f32::consts::PI
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumCount, FromRepr, Serialize, Deserialize)]
pub enum Side3d
{
    Left,
    Right,
    Top,
    Bottom,
    Front,
    Back
}

impl From<Side2d> for Side3d
{
    fn from(side: Side2d) -> Self
    {
        match side
        {
            Side2d::Left => Self::Left,
            Side2d::Right => Self::Right,
            Side2d::Front => Self::Front,
            Side2d::Back => Self::Back
        }
    }
}
