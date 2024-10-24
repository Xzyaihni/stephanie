use std::mem;

use serde::{Serialize, Deserialize};

use strum::EnumCount;

use nalgebra::Vector3;

use crate::common::{
    watcher::*,
    lazy_transform::*,
    damaging::*,
    Occluder,
    Joint,
    LazyMix,
    Outlineable,
    Transform,
    Faction,
    Collider,
    Physical,
    Inventory,
    Entity,
    EntityInfo,
    Character,
    CharacterSyncInfo,
    Player,
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
    SetLazyMix{entity: Entity, component: LazyMix},
    SetOutlineable{entity: Entity, component: Outlineable},
    SetFollowRotation{entity: Entity, component: FollowRotation},
    SetFollowPosition{entity: Entity, component: FollowPosition},
    SetInventory{entity: Entity, component: Inventory},
    SetRender{entity: Entity, component: RenderInfo},
    SetCollider{entity: Entity, component: Collider},
    SetPhysical{entity: Entity, component: Physical},
    SetJoint{entity: Entity, component: Joint},
    SetWatchers{entity: Entity, component: Watchers},
    SetDamaging{entity: Entity, component: Damaging},
    SetAnatomy{entity: Entity, component: Anatomy},
    SetCharacter{entity: Entity, component: Character},
    SetPlayer{entity: Entity, component: Player},
    SetEnemy{entity: Entity, component: Enemy},
    SetNamed{entity: Entity, component: String},
    SetOccluder{entity: Entity, component: Occluder},
    SetNone{entity: Entity, component: ()},
    SetTarget{entity: Entity, target: Transform},
    SyncPosition{entity: Entity, position: Vector3<f32>},
    SyncPositionRotation{entity: Entity, position: Vector3<f32>, rotation: f32},
    SyncCharacter{entity: Entity, info: CharacterSyncInfo},
    EntityDestroy{entity: Entity},
    EntityDamage{entity: Entity, faction: Faction, damage: Damage},
    PlayerConnect{name: String},
    PlayerOnConnect{player_entity: Entity},
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
            | Message::SetLazyMix{entity, ..}
            | Message::SetOutlineable{entity, ..}
            | Message::SetFollowRotation{entity, ..}
            | Message::SetFollowPosition{entity, ..}
            | Message::SetInventory{entity, ..}
            | Message::SetRender{entity, ..}
            | Message::SetCollider{entity, ..}
            | Message::SetPhysical{entity, ..}
            | Message::SetJoint{entity, ..}
            | Message::SetWatchers{entity, ..}
            | Message::SetDamaging{entity, ..}
            | Message::SetAnatomy{entity, ..}
            | Message::SetCharacter{entity, ..}
            | Message::SetPlayer{entity, ..}
            | Message::SetEnemy{entity, ..}
            | Message::SetNamed{entity, ..}
            | Message::SetOccluder{entity, ..}
            | Message::SetNone{entity, ..}
            | Message::SetTarget{entity, ..}
            | Message::SyncPosition{entity, ..}
            | Message::SyncPositionRotation{entity, ..}
            | Message::SyncCharacter{entity, ..}
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
