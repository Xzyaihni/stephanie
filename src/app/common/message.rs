use std::mem;

use serde::{Serialize, Deserialize};

use strum_macros::EnumCount;

use crate::common::{
	Transform,
    Physical,
    Entity,
    EntityInfo,
    RenderInfo,
    Player,
    Damage,
    Anatomy,
	world::{Chunk, GlobalPos}
};


#[derive(Debug, Clone, EnumCount, Serialize, Deserialize)]
pub enum Message
{
    EntitySet{entity: Entity, info: EntityInfo},
    SetTransform{entity: Entity, transform: Transform},
    SetRender{entity: Entity, render: RenderInfo},
    SetPlayer{entity: Entity, player: Player},
    SetPhysical{entity: Entity, physical: Physical},
    SetAnatomy{entity: Entity, anatomy: Anatomy},
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

    pub fn entity(&self) -> Option<Entity>
    {
        match self
        {
            Message::EntitySet{entity, ..} => Some(entity),
            Message::SetTransform{entity, ..} => Some(entity),
            Message::SetRender{entity, ..} => Some(entity),
            Message::SetPlayer{entity, ..} => Some(entity),
            Message::SetPhysical{entity, ..} => Some(entity),
            Message::SetAnatomy{entity, ..} => Some(entity),
            Message::EntityDestroy{entity, ..} => Some(entity),
            Message::EntityDamage{entity, ..} => Some(entity),
            Message::PlayerConnect{..}
            | Message::PlayerOnConnect{..}
            | Message::PlayerFullyConnected
            | Message::ChunkRequest{..}
            | Message::ChunkSync{..}
            | Message::RepeatMessage{..} => None
        }.copied()
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
