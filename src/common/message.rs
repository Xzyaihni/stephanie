use std::mem;

use serde::{Serialize, Deserialize};

use strum::{EnumCount, IntoStaticStr};

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
    OnConnectInfo,
    entity::{EntityRemoveMany, EntityRemove},
    world::{Pos3, TilePos, Tile, Chunk, GlobalPos}
};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DebugMessage
{
    PrintServerOvermaps,
    PrintEntityInfo(Entity)
}

#[derive(Debug, Clone, EnumCount, IntoStaticStr, Serialize, Deserialize)]
pub enum Message
{
    EntitySet{entity: Entity, info: Box<EntityInfo>},
    EntitySetMany{entities: Vec<(Entity, EntityInfo)>},
    EntityRemove(EntityRemove),
    EntityRemoveFinished{entity: Entity},
    EntityRemoveMany(EntityRemoveMany),
    EntityRemoveManyFinished{entities: Vec<Entity>},
    EntityRemoveChunk{pos: GlobalPos, entities: EntityRemoveMany},
    EntityRemoveChunkFinished{pos: GlobalPos, entities: Vec<Entity>},
    SetParent{entity: Entity, component: Option<Box<Parent>>},
    SetSibling{entity: Entity, component: Option<Box<Entity>>},
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
    SyncCamera{position: Pos3<f32>},
    SyncWorldTime{time: f64},
    PlayerConnect{name: String, host: bool},
    PlayerOnConnect(OnConnectInfo),
    PlayerFullyConnected,
    PlayerDisconnect{time: Option<f64>, restart: bool, host: bool},
    PlayerDisconnectFinished,
    SetTrusted,
    ChunkRequest{pos: GlobalPos},
    ChunkSync{pos: GlobalPos, chunk: Chunk, entities: Vec<(Entity, EntityInfo)>},
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
            | Message::EntityRemoveChunkFinished{..}
            | Message::SyncCamera{..}
            | Message::SyncWorldTime{..}
            | Message::PlayerConnect{..}
            | Message::PlayerOnConnect{..}
            | Message::PlayerFullyConnected
            | Message::PlayerDisconnect{..}
            | Message::PlayerDisconnectFinished => false,
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

    pub fn clear(&mut self)
    {
        self.buffer.clear();
    }

    pub fn get_buffered(&mut self) -> Vec<Message>
    {
        mem::take(&mut self.buffer)
    }
}
