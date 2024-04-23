use std::mem;

use serde::{Serialize, Deserialize};

use enum_amount::EnumCount;

use crate::common::{
	Transform,
	EntityType,
    EntityAny,
	world::{Chunk, GlobalPos}
};


#[derive(Debug, Clone, EnumCount, Serialize, Deserialize)]
pub enum Message
{
    EntitySet{id: EntityType, entity: EntityAny},
    EntityAdd{entity: EntityAny},
    EntityDestroy{id: EntityType},
	PlayerConnect{name: String},
	PlayerOnConnect{id: usize},
	PlayerFullyConnected,
	EntitySyncTransform{entity_type: EntityType, transform: Transform},
	ChunkRequest{pos: GlobalPos},
	ChunkSync{pos: GlobalPos, chunk: Chunk},
    RepeatMessage{message: Box<Message>}
}

impl Message
{
	pub fn entity_type(&self) -> Option<EntityType>
	{
		match self
		{
            Message::EntitySet{id, ..} => Some(*id),
            Message::EntityDestroy{id, ..} => Some(*id),
			Message::EntitySyncTransform{entity_type, ..} => Some(*entity_type),
			_ => None
		}
	}

	pub fn forward(&self) -> bool
	{
		match self
		{
			Message::ChunkRequest{..} | Message::EntityAdd{..} => false,
			_ => true
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
		mem::take(&mut self.buffer).into_iter()
	}
}
