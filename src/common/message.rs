use std::mem;

use serde::{Serialize, Deserialize};

use strum::EnumCount;

use nalgebra::Vector3;

use crate::common::{
    watcher::*,
    lazy_transform::*,
    damaging::*,
    Occluder,
    Door,
    Joint,
    Light,
    LazyMix,
    Outlineable,
    Transform,
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
    Anatomy,
    RenderInfo,
    world::{Pos3, TilePos, Tile, Chunk, GlobalPos}
};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DebugMessage
{
    PrintServerOvermaps,
    PrintEntityInfo(Entity)
}

#[derive(Debug, Clone, EnumCount, Serialize, Deserialize)]
pub enum Message
{
    EntitySet{entity: Entity, info: Box<EntityInfo>},
    SetParent{entity: Entity, component: Option<Box<Parent>>},
    SetTransform{entity: Entity, component: Option<Box<Transform>>},
    SetLazyTransform{entity: Entity, component: Option<Box<LazyTransform>>},
    SetLazyMix{entity: Entity, component: Option<Box<LazyMix>>},
    SetOutlineable{entity: Entity, component: Option<Box<Outlineable>>},
    SetFollowRotation{entity: Entity, component: Option<Box<FollowRotation>>},
    SetFollowPosition{entity: Entity, component: Option<Box<FollowPosition>>},
    SetInventory{entity: Entity, component: Option<Box<Inventory>>},
    SetRender{entity: Entity, component: Option<Box<RenderInfo>>},
    SetCollider{entity: Entity, component: Option<Box<Collider>>},
    SetPhysical{entity: Entity, component: Option<Box<Physical>>},
    SetDoor{entity: Entity, component: Option<Box<Door>>},
    SetJoint{entity: Entity, component: Option<Box<Joint>>},
    SetLight{entity: Entity, component: Option<Box<Light>>},
    SetWatchers{entity: Entity, component: Option<Box<Watchers>>},
    SetDamaging{entity: Entity, component: Option<Box<Damaging>>},
    SetAnatomy{entity: Entity, component: Option<Box<Anatomy>>},
    SetCharacter{entity: Entity, component: Option<Box<Character>>},
    SetPlayer{entity: Entity, component: Option<Box<Player>>},
    SetEnemy{entity: Entity, component: Option<Box<Enemy>>},
    SetNamed{entity: Entity, component: Option<Box<String>>},
    SetOccluder{entity: Entity, component: Option<Box<Occluder>>},
    SetNone{entity: Entity, component: Option<Box<()>>},
    SetTarget{entity: Entity, target: Box<Transform>},
    SyncPosition{entity: Entity, position: Vector3<f32>},
    SyncPositionRotation{entity: Entity, position: Vector3<f32>, rotation: f32},
    SyncCharacter{entity: Entity, info: CharacterSyncInfo},
    EntityDestroy{entity: Entity},
    PlayerConnect{name: String},
    PlayerOnConnect{player_entity: Entity, player_position: Pos3<f32>},
    PlayerFullyConnected,
    PlayerDisconnect{restart: bool, host: bool},
    PlayerDisconnectFinished,
    SetTrusted,
    ChunkRequest{pos: GlobalPos},
    ChunkSync{pos: GlobalPos, chunk: Chunk},
    SetTile{pos: TilePos, tile: Tile},
    RepeatMessage{message: Box<Message>},
    #[cfg(debug_assertions)]
    DebugMessage(DebugMessage)
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
            | Message::SetDoor{entity, ..}
            | Message::SetJoint{entity, ..}
            | Message::SetLight{entity, ..}
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
            | Message::EntityDestroy{entity, ..}  => Some(*entity),
            Message::PlayerConnect{..}
            | Message::PlayerOnConnect{..}
            | Message::PlayerFullyConnected
            | Message::PlayerDisconnect{..}
            | Message::PlayerDisconnectFinished
            | Message::SetTrusted
            | Message::ChunkRequest{..}
            | Message::ChunkSync{..}
            | Message::SetTile{..}
            | Message::RepeatMessage{..} => None,
            #[cfg(debug_assertions)]
            Message::DebugMessage(_) => None
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
