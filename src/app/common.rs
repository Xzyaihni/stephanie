use std::{
    f32,
    io,
    fmt::Debug,
    sync::Arc,
    net::TcpStream
};

use parking_lot::RwLock;

use message::Message;

pub use yanyaengine::{Transform, TransformContainer};

pub use objects_store::ObjectsStore;

pub use sender_loop::{sender_loop, BufferSender};
pub use receiver_loop::receiver_loop;

pub use tilemap::{TileMap, TileInfo, SpecialTile, TileMapWithTextures};

pub use outlineable::Outlineable;

pub use lazy_mix::LazyMix;
pub use lazy_transform::{
    LazyTransform,
    LazyTransformInfo,
    LazyTargettable
};

pub use entity::{
    AnyEntities,
    ServerToClient,
    Component,
    Parent,
    Entity,
    EntityInfo,
    ClientEntityInfo,
    Entities
};

pub use utility::*;

pub use sides::{Side1d, Side2d, Side3d};

pub use loot::Loot;
pub use item::Item;
pub use items_info::{ItemInfo, ItemsInfo, Ranged};

pub use inventory::{InventorySorter, InventoryItem, Inventory};

pub use character::{Character, Faction};
pub use characters_info::{Hairstyle, CharacterId, CharactersInfo, CharacterInfo};

pub use player::Player;

pub use enemy::{EnemyBehavior, Enemy};
pub use enemy_builder::EnemyBuilder;
pub use furniture_builder::FurnitureBuilder;
pub use enemies_info::{EnemyId, EnemyInfo, EnemiesInfo};

pub use chunk_saver::{SaveLoad, WorldChunkSaver, ChunkSaver, EntitiesSaver};

pub use occluding_plane::{OccludingPlane, OccludingPlaneServer, OccludingCaster};
pub use render_info::RenderInfo;

pub use anatomy::{Anatomy, HumanAnatomy, HumanAnatomyInfo};
pub use damage::{Damageable, Damage, DamageType, DamageDirection, DamageHeight, DamagePartial};

pub use collider::{ColliderType, Collider, CollidingInfo};
pub use physics::{Physical, PhysicalProperties};

pub mod utility;

pub mod sides;
pub mod lisp;
pub mod objects_store;

pub mod raycast;

pub mod render_info;
pub mod occluding_plane;

pub mod damaging;
pub mod damage;
pub mod anatomy;

pub mod character;
pub mod characters_info;

pub mod outlineable;

pub mod watcher;
pub mod lazy_mix;
pub mod lazy_transform;
pub mod entity;

pub mod generic_info;

pub mod loot;
pub mod item;
pub mod items_info;

pub mod inventory;

pub mod player;

pub mod particle_creator;
pub mod furniture_builder;

pub mod enemy;
pub mod enemy_builder;
pub mod enemies_info;

pub mod message;

pub mod sender_loop;
pub mod receiver_loop;

pub mod tilemap;

pub mod chunk_saver;
pub mod world;

pub mod collider;
pub mod physics;


pub type MessageSerError = ciborium::ser::Error<io::Error>;
pub type MessageDeError = ciborium::de::Error<io::Error>;

#[macro_export]
macro_rules! time_this
{
    ($name:expr, $($tt:tt),*) =>
    {
        {
            use std::time::Instant;

            let start_time = Instant::now();

            $($tt)*

            eprintln!("{} took {} ms", $name, start_time.elapsed().as_millis());
        }
    }
}

pub const ENTITY_SCALE: f32 = 0.09;

#[derive(Clone)]
pub struct DataInfos
{
    pub items_info: Arc<ItemsInfo>,
    pub enemies_info: Arc<EnemiesInfo>,
    pub characters_info: Arc<CharactersInfo>,
    pub player_character: CharacterId
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(pub usize);

pub trait EntityPasser
{
    fn send_single(&mut self, id: ConnectionId, message: Message);
    fn send_message(&mut self, message: Message);
}

pub trait EntitiesController
{
    type Container;
    type Passer: EntityPasser;

    fn container_ref(&self) -> &Self::Container;
    fn container_mut(&mut self) -> &mut Self::Container;
    fn passer(&self) -> Arc<RwLock<Self::Passer>>;
}

#[derive(Debug)]
pub struct MessagePasser
{
    buffer: [u8; 64 * 1024],
    stream: TcpStream
}

impl MessagePasser
{
    pub fn new(stream: TcpStream) -> Self
    {
        Self{buffer: [0; 64 * 1024], stream}
    }

    pub fn send_one(&mut self, message: &Message) -> Result<(), MessageSerError>
    {
        self.send_many(&vec![message.clone()])
    }

    pub fn send_many(&mut self, messages: &Vec<Message>) -> Result<(), MessageSerError>
    {
        if messages.is_empty()
        {
            return Ok(());
        }

        ciborium::into_writer(messages, &mut self.stream)
    }

    pub fn receive(&mut self) -> Result<Vec<Message>, MessageDeError>
    {
        ciborium::from_reader_with_buffer(&mut self.stream, &mut self.buffer)
    }

    pub fn receive_one(&mut self) -> Result<Option<Message>, MessageDeError>
    {
        self.receive().map(|messages|
        {
            debug_assert!(messages.len() == 1);

            messages.into_iter().next()
        })
    }

    pub fn try_clone(&self) -> Self
    {
        Self{buffer: self.buffer, stream: self.stream.try_clone().unwrap()}
    }
}
