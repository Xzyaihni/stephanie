use std::f32;

use serde::{Serialize, Deserialize};

use strum::{FromRepr, EnumString};

use crate::common::lisp::{self, Register, LispValue, LispMemory};


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

#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tile(pub Option<TileExisting>);

impl Tile
{
    pub fn new(id: usize) -> Self
    {
        Self(Some(TileExisting{id, rotation: TileRotation::default()}))
    }

    pub fn id_string(&self) -> String
    {
        if let Some(tile) = self.0
        {
            format!("{}{}", tile.id, tile.rotation.to_arrow_str())
        } else
        {
            "_".to_owned()
        }
    }

    pub fn as_lisp_value(&self, memory: &mut LispMemory) -> Result<LispValue, lisp::Error>
    {
        Ok(if let Some(tile) = self.0
        {
            let restore = memory.with_saved_registers([Register::Value, Register::Temporary]);

            memory.set_register(Register::Value, tile.id as i32);
            memory.set_register(Register::Temporary, tile.rotation as i32);

            memory.cons(Register::Value, Register::Value, Register::Temporary)?;

            let value = memory.get_register(Register::Value);

            restore(memory)?;

            value
        } else
        {
            ().into()
        })
    }

    pub fn from_lisp_value(
        memory: &LispMemory,
        value: LispValue
    ) -> Result<Self, lisp::Error>
    {
        if value.is_null()
        {
            return Ok(Tile::none());
        }

        let lst = value.as_list(memory)?;

        let id = lst.car.as_integer()? as usize;

        let rotation = lst.cdr.as_integer()?;
        let rotation = TileRotation::from_repr(rotation as usize).unwrap_or_else(||
        {
            panic!("{rotation} is an invalid rotation number")
        });

        Ok(Tile(Some(TileExisting{id, rotation})))
    }

    pub fn id(&self) -> Option<usize>
    {
        self.0.map(|x| x.id)
    }

    pub fn none() -> Self
    {
        Self::default()
    }

    pub fn is_none(&self) -> bool
    {
        self.0.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileExisting
{
    id: usize,
    pub rotation: TileRotation
}

impl TileExisting
{
    pub fn id(&self) -> usize
    {
        self.id
    }
}
