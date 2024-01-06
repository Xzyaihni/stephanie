use std::mem;

use serde::{Serialize, Deserialize};

use enum_amount::EnumCount;

use crate::common::{
	Transform,
	EntityType,
	player::Player,
	world::{Chunk, GlobalPos}
};


#[derive(Debug, Clone, EnumCount, Serialize, Deserialize)]
pub enum Message
{
	PlayerConnect{name: String},
	PlayerCreate{id: usize, player: Player},
	PlayerDestroy{id: usize},
	PlayerOnConnect{id: usize},
	PlayerFullyConnected,
	EntitySyncTransform{entity_type: EntityType, transform: Transform},
	ChunkRequest{pos: GlobalPos},
	ChunkSync{pos: GlobalPos, chunk: Chunk}
}

impl Message
{
	pub fn entity_type(&self) -> Option<EntityType>
	{
		match self
		{
			Message::EntitySyncTransform{entity_type, ..} => Some(*entity_type),
			_ => None
		}
	}

	pub fn overwriting(&self) -> Option<usize>
	{
		match self
		{
			Message::EntitySyncTransform{..} => Some(0),
			_ => None
		}
	}

	pub fn forward(&self) -> bool
	{
		match self
		{
			Message::ChunkRequest{..} => false,
			_ => true
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
		if let Some(id) = message.overwriting()
		{
			self.overwriting_buffer[id] = Some(message);
		} else
		{
			self.buffer.push(message);
		}
	}

	pub fn get_buffered(&mut self) -> impl Iterator<Item=Message> + '_
	{
		mem::take(&mut self.buffer).into_iter()
			.chain(
				mem::replace(&mut self.overwriting_buffer, [None; OVERWRITING_COUNT])
					.into_iter()
					.flatten()
			)
	}
}
