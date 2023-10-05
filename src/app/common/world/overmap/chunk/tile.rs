use rlua::{ToLua, FromLua};

use serde::{Serialize, Deserialize};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tile
{
	id: usize
}

impl Tile
{
	pub fn new(id: usize) -> Self
	{
		Self{id}
	}

	pub fn id(&self) -> usize
	{
		self.id
	}

	pub fn none() -> Self
	{
		Self{id: 0}
	}

	pub fn is_none(&self) -> bool
	{
		self.id == 0
	}
}

impl<'lua> ToLua<'lua> for Tile
{
    fn to_lua(self, lua: rlua::Context<'lua>) -> rlua::Result<rlua::Value<'lua>>
    {
        self.id.to_lua(lua)
    }
}

impl<'lua> FromLua<'lua> for Tile
{
    fn from_lua(value: rlua::Value<'lua>, lua: rlua::Context<'lua>) -> rlua::Result<Self>
    {
        usize::from_lua(value, lua).map(|value| Tile::new(value))
    }
}
