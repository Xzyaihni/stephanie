use std::mem;

use serde::{Serialize, Deserialize};

use strum_macros::EnumCount;

use crate::common::{
    watcher::*,
    lazy_transform::*,
    damaging::*,
    Transform,
    Collider,
    Physical,
    Inventory,
    Entity,
    EntityInfo,
    Player,
    PlayerEntities,
    Parent,
    Enemy,
    Damage,
    Anatomy,
    RenderInfo,
    world::{TilePos, Tile, Chunk, GlobalPos}
};


#[derive(Debug, Clone, EnumCount, Serialize, Deserialize)]
pub enum Message
{
    EntitySet{entity: Entity, info: EntityInfo},
    SetParent{entity: Entity, component: Parent},
    SetTransform{entity: Entity, component: Transform},
    SetLazyTransform{entity: Entity, component: LazyTransform},
    SetFollowRotation{entity: Entity, component: FollowRotation},
    SetInventory{entity: Entity, component: Inventory},
    SetRender{entity: Entity, component: RenderInfo},
    SetPlayer{entity: Entity, component: Player},
    SetCollider{entity: Entity, component: Collider},
    SetPhysical{entity: Entity, component: Physical},
    SetWatchers{entity: Entity, component: Watchers},
    SetDamaging{entity: Entity, component: Damaging},
    SetAnatomy{entity: Entity, component: Anatomy},
    SetEnemy{entity: Entity, component: Enemy},
    SetNamed{entity: Entity, component: String},
    SetNone{entity: Entity, component: ()},
    SetTarget{entity: Entity, target: Transform},
    EntityDestroy{entity: Entity},
    EntityDamage{entity: Entity, damage: Damage},
    PlayerConnect{name: String},
    PlayerOnConnect{player_entities: PlayerEntities},
    PlayerFullyConnected,
    PlayerDisconnect{host: bool},
    PlayerDisconnectFinished,
    SetTrusted,
    ChunkRequest{pos: GlobalPos},
    ChunkSync{pos: GlobalPos, chunk: Chunk},
    SetTile{pos: TilePos, tile: Tile},
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
            | Message::PlayerFullyConnected
            | Message::PlayerDisconnect{..}
            | Message::PlayerDisconnectFinished => false,
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
            | Message::SetFollowRotation{entity, ..}
            | Message::SetInventory{entity, ..}
            | Message::SetRender{entity, ..}
            | Message::SetPlayer{entity, ..}
            | Message::SetCollider{entity, ..}
            | Message::SetPhysical{entity, ..}
            | Message::SetWatchers{entity, ..}
            | Message::SetDamaging{entity, ..}
            | Message::SetAnatomy{entity, ..}
            | Message::SetEnemy{entity, ..}
            | Message::SetNamed{entity, ..}
            | Message::SetNone{entity, ..}
            | Message::SetTarget{entity, ..}
            | Message::EntityDestroy{entity, ..}
            | Message::EntityDamage{entity, ..} => Some(*entity),
            Message::PlayerConnect{..}
            | Message::PlayerOnConnect{..}
            | Message::PlayerFullyConnected
            | Message::PlayerDisconnect{..}
            | Message::PlayerDisconnectFinished
            | Message::SetTrusted
            | Message::ChunkRequest{..}
            | Message::ChunkSync{..}
            | Message::SetTile{..}
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

    pub fn clear(&mut self)
    {
        self.buffer.clear();
    }

    pub fn get_buffered(&mut self) -> Vec<Message>
    {
        mem::take(&mut self.buffer)
    }
}
