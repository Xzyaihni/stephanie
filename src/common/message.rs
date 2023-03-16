use std::mem;

use serde::{Serialize, Deserialize};

use enum_amount::EnumCount;

use super::{
	EntityType,
	entity::Entity,
	player::Player
};


#[derive(Debug, Clone, EnumCount, Serialize, Deserialize)]
pub enum Message
{
	PlayerConnect{name: String},
	PlayerCreate{id: usize, player: Player},
	PlayerDestroy{id: usize},
	PlayerOnConnect{id: usize},
	PlayerFullyConnected,
	EntitySync{entity_type: EntityType, entity: Entity}
}

impl Message
{
	pub fn entity_type(&self) -> Option<EntityType>
	{
		match self
		{
			Message::EntitySync{entity_type, ..} => Some(*entity_type),
			_ => None
		}
	}
}

const OVERWRITING_COUNT: usize = 1;

#[derive(Debug, Clone)]
pub struct MessageBuffer
{
	overwriting_buffer: [Option<Message>; OVERWRITING_COUNT],
	buffer: Vec<Message>
}

impl MessageBuffer
{
	pub fn new() -> Self
	{
		let overwriting_buffer = [None; OVERWRITING_COUNT];

		Self{overwriting_buffer, buffer: Vec::new()}
	}

	pub fn set_message(&mut self, message: Message)
	{
		let overwriting_id = match &message
		{
			Message::EntitySync{..} => Some(0),
			_ => None
		};

		if let Some(id) = overwriting_id
		{
			self.overwriting_buffer[id] = Some(message);
		} else
		{
			self.buffer.push(message);
		}
	}

	pub fn get_buffered(&mut self) -> impl Iterator<Item=Message> + '_
	{
		mem::replace(&mut self.buffer, Vec::new()).into_iter()
			.chain(mem::replace(&mut self.overwriting_buffer, [None; OVERWRITING_COUNT])
				.into_iter()
				.filter_map(|message| message)
			)
	}
}