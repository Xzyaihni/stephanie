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

    pub fn as_lisp_value(&self, memory: &mut LispMemory) -> Result<LispValue, lisp::Error>
    {
        let restore = memory.with_saved_registers([Register::Value, Register::Temporary]);

        memory.set_register(Register::Value, self.id as i32);
        memory.set_register(Register::Temporary, self.rotation as i32);

        memory.cons(Register::Value, Register::Value, Register::Temporary)?;

        let value = memory.get_register(Register::Value);

        restore(memory)?;

        Ok(value)
    }

    pub fn from_lisp_value(
        memory: &LispMemory,
        value: LispValue
    ) -> Result<Self, lisp::Error>
    {
        let lst = value.as_list(memory).unwrap();

        let id = lst.car.as_integer()? as usize;

        let rotation = lst.cdr.as_integer()?;
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
