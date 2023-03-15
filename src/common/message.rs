use std::mem;

use serde::{Serialize, Deserialize};

use enum_amount::EnumCount;

use super::{
	EntityType,
	player::Player,
	entity::transform::Transform
};


#[derive(Debug, Clone, EnumCount, Serialize, Deserialize)]
pub enum Message
{
	PlayerCreate{player: Player},
	PlayersList{player_id: usize},
	PlayerFullyConnected,
	EntityTransform{entity: EntityType, transform: Transform}
}

impl Message
{
	pub fn entity_type(&self) -> Option<EntityType>
	{
		match self
		{
			Message::EntityTransform{entity, ..} => Some(*entity),
			_ => None
		}
	}
}

#[derive(Debug, Clone)]
pub struct MessageBuffer
{
	buffer: Vec<Message>
}

impl MessageBuffer
{
	pub fn new() -> Self
	{
		Self{buffer: Vec::new()}
	}

	pub fn set_message(&mut self, message: Message)
	{
		self.buffer.push(message);
	}

	pub fn get_buffered(&mut self) -> impl Iterator<Item=Message> + '_
	{
		mem::replace(&mut self.buffer, Vec::new()).into_iter()
	}
}