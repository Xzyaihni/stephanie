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
    SetParent{entity: Entity, component: Box<Parent>},
    SetTransform{entity: Entity, component: Box<Transform>},
    SetLazyTransform{entity: Entity, component: Box<LazyTransform>},
    SetLazyMix{entity: Entity, component: Box<LazyMix>},
    SetOutlineable{entity: Entity, component: Box<Outlineable>},
    SetFollowRotation{entity: Entity, component: Box<FollowRotation>},
    SetFollowPosition{entity: Entity, component: Box<FollowPosition>},
    SetInventory{entity: Entity, component: Box<Inventory>},
    SetRender{entity: Entity, component: Box<RenderInfo>},
    SetCollider{entity: Entity, component: Box<Collider>},
    SetPhysical{entity: Entity, component: Box<Physical>},
    SetJoint{entity: Entity, component: Box<Joint>},
    SetWatchers{entity: Entity, component: Box<Watchers>},
    SetDamaging{entity: Entity, component: Box<Damaging>},
    SetAnatomy{entity: Entity, component: Box<Anatomy>},
    SetCharacter{entity: Entity, component: Box<Character>},
    SetPlayer{entity: Entity, component: Box<Player>},
    SetEnemy{entity: Entity, component: Box<Enemy>},
    SetNamed{entity: Entity, component: Box<String>},
    SetOccluder{entity: Entity, component: Box<Occluder>},
    SetNone{entity: Entity, component: Box<()>},
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
