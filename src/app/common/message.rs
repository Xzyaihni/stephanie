use std::mem;

use serde::{Serialize, Deserialize};

use strum_macros::EnumCount;

use crate::common::{
	Transform,
    Entity,
    EntityInfo,
    RenderInfo,
    Player,
    Damage,
	world::{Chunk, GlobalPos}
};


#[derive(Debug, Clone, EnumCount, Serialize, Deserialize)]
pub enum Message
{
    EntitySet{entity: Entity, info: EntityInfo},
    SetTransform{entity: Entity, transform: Transform},
    SetRender{entity: Entity, render: RenderInfo},
    SetPlayer{entity: Entity, player: Player},
    EntityDestroy{entity: Entity},
    EntityDamage{entity: Entity, damage: Damage},
	PlayerConnect{name: String},
	PlayerOnConnect{entity: Entity},
	PlayerFullyConnected,
	ChunkRequest{pos: GlobalPos},
	ChunkSync{pos: GlobalPos, chunk: Chunk},
    RepeatMessage{message: Box<Message>}
}

impl Message
{
	pub fn forward(&self) -> bool
	{
		match self
		{
			Message::ChunkRequest{..} => false,
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
