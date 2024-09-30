use std::f32;

use serde::{Serialize, Deserialize};

use strum::{FromRepr, EnumString};

use crate::common::lisp::{self, LispMemory, ValueRaw};


#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr, EnumString, Serialize, Deserialize)]
pub enum TileRotation
{
    Up,
    Right,
    Left,
    Down
}

impl Default for TileRotation
{
    fn default() -> Self
    {
        Self::Up
    }
}

impl TileRotation
{
    pub fn to_angle(&self) -> f32
    {
        match self
        {
            Self::Right => 0.0,
            Self::Up => f32::consts::FRAC_PI_2,
            Self::Left => f32::consts::PI,
            Self::Down => -f32::consts::FRAC_PI_2
        }
    }

    pub fn to_arrow_str(&self) -> &str
    {
        match self
        {
            Self::Up => "↑",
            Self::Right => "→",
            Self::Left => "←",
            Self::Down => "↓"
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tile
{
    id: usize,
    pub rotation: TileRotation
}

impl Tile
{
    pub fn new(id: usize) -> Self
    {
        Self{id, rotation: TileRotation::default()}
    }

    pub fn id_string(&self) -> String
    {
        format!("{}{}", self.id, self.rotation.to_arrow_str())
    }

    pub fn as_lisp_value(&self, memory: &mut LispMemory)
    {
        memory.push_return(self.id as i32);
        memory.push_return(self.rotation as i32);

        memory.cons();
    }

    /// # Safety
    /// value must be of type ValueTag::List
    pub unsafe fn from_lisp_value(
        memory: &LispMemory,
        value: ValueRaw
    ) -> Result<Self, lisp::Error>
    {
        let lst = memory.get_list(unsafe{ value.list });

        let id = lst.car().as_integer()? as usize;

        let rotation = lst.cdr().as_integer()?;
        let rotation = TileRotation::from_repr(rotation as usize).unwrap_or_else(||
        {
            panic!("{rotation} is an invalid rotation number")
        });

        Ok(Tile{id, rotation})
    }

    pub fn id(&self) -> usize
    {
        self.id
    }

    pub fn none() -> Self
    {
        Self::new(0)
    }

    pub fn is_none(&self) -> bool
    {
        self.id == 0
    }
}
