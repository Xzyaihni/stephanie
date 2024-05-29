use std::mem;

use serde::{Serialize, Deserialize};

use strum_macros::EnumCount;

use crate::common::{
    Transform,
    Physical,
    Inventory,
    Entity,
    EntityInfo,
    RenderInfo,
    Player,
    PlayerEntities,
    Parent,
    Enemy,
    Damage,
    Anatomy,
    LazyTransform,
    world::{Chunk, GlobalPos}
};


#[derive(Debug, Clone, EnumCount, Serialize, Deserialize)]
pub enum Message
{
    EntitySet{entity: Entity, info: EntityInfo},
    SetParent{entity: Entity, parent: Parent},
    SetTransform{entity: Entity, transform: Transform},
    SetLazyTransform{entity: Entity, lazy_transform: LazyTransform},
    SetInventory{entity: Entity, inventory: Inventory},
    SetRender{entity: Entity, render: RenderInfo},
    SetPlayer{entity: Entity, player: Player},
    SetPhysical{entity: Entity, physical: Physical},
    SetAnatomy{entity: Entity, anatomy: Anatomy},
    SetEnemy{entity: Entity, enemy: Enemy},
    SetUiElement{entity: Entity, ui_element: ()},
    EntityDestroy{entity: Entity},
    EntityDamage{entity: Entity, damage: Damage},
    PlayerConnect{name: String},
    PlayerOnConnect{player_entities: PlayerEntities},
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
            Message::ChunkRequest{..}
            | Message::PlayerConnect{..}
            | Message::PlayerOnConnect{..}
            | Message::PlayerFullyConnected => false,
            _ => true
        }
    }

    pub fn entity(&self) -> Option<Entity>
    {
        match self
        {
            Message::EntitySet{entity, ..}
            | Message::SetParent{entity, ..}
            | Message::SetTransform{entity, ..}
            | Message::SetLazyTransform{entity, ..}
            | Message::SetInventory{entity, ..}
            | Message::SetRender{entity, ..}
            | Message::SetPlayer{entity, ..}
            | Message::SetPhysical{entity, ..}
            | Message::SetAnatomy{entity, ..}
            | Message::SetEnemy{entity, ..}
            | Message::SetUiElement{entity, ..}
            | Message::EntityDestroy{entity, ..}
            | Message::EntityDamage{entity, ..} => Some(*entity),
            Message::PlayerConnect{..}
            | Message::PlayerOnConnect{..}
            | Message::PlayerFullyConnected
            | Message::ChunkRequest{..}
            | Message::ChunkSync{..}
            | Message::RepeatMessage{..} => None
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

    pub fn get_buffered(&mut self) -> Vec<Message>
    {
        mem::take(&mut self.buffer)
    }
}
