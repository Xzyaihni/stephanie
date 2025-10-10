use std::{
    f32,
    fmt::Debug,
    sync::Arc,
    net::TcpStream
};

#[allow(unused_imports)]
use std::sync::LazyLock;

use parking_lot::Mutex;

use message::Message;

pub use yanyaengine::{Transform, TransformContainer};

pub use objects_store::ObjectsStore;

pub use sender_loop::{sender_loop, BufferSender};
pub use receiver_loop::receiver_loop;

pub use tilemap::{
    TileMap,
    TileInfo,
    SpecialTile,
    TileMapWithTextures
};

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
    OnChangeInfo,
    Parent,
    Entity,
    EntityInfo,
    ClientEntityInfo,
    FullEntityInfo,
    Entities
};

pub use utility::*;

pub use sides::{Side1d, Side2d, Side3d};

pub use drug::Drug;
pub use loot::Loot;
pub use item::{Item, ItemRarity, ItemBuff};
pub use items_info::{ItemInfo, ItemId, ItemsInfo, Ranged};

pub use inventory::{InventorySorter, InventoryItem, Inventory};

pub use furnitures_info::{FurnitureId, FurnitureInfo, FurnituresInfo};

pub use character::{CharacterSyncInfo, Character, Faction};
pub use characters_info::{Hairstyle, CharacterId, CharactersInfo, CharacterInfo};

pub use player::{Player, OnConnectInfo};

pub use enemy::{EnemyBehavior, Enemy};
pub use enemies_info::{EnemyId, EnemyInfo, EnemiesInfo};

pub use chunk_saver::{SaveLoad, WorldChunksBlock, WorldChunkSaver, ChunkSaver, EntitiesSaver};

pub use occluding_plane::{
    OccludingVertex,
    Occluder,
    ClientOccluder,
    OccludingPlane,
    OccluderVisibilityChecker,
    OccludingCaster
};

pub use render_info::RenderInfo;

pub use saveable::Saveable;

pub use anatomy::{Anatomy, HumanAnatomy, HumanAnatomyInfo};
pub use damage::{Damageable, Damage, DamageType, DamageDirection, DamageHeight, DamagePartial};

pub use spatial::{SpatialInfo, SpatialGrid};
pub use collider::{OverrideTransform, ColliderType, Collider, CollidingInfo};
pub use physics::{Physical, PhysicalProperties, PhysicalFixed};

pub use world::{
    World,
    SkyOccludingVertex,
    SkyLightVertex,
    PosDirection,
    Pos3,
    Axis,
    FlatChunksContainer,
    ChunksContainer,
    pathfind::Pathfinder
};

pub use door::Door;
pub use joint::Joint;
pub use light::{Light, ClientLight};

pub use systems::*;

pub mod utility;
pub mod colors;

pub mod sides;
pub mod lisp;
pub mod objects_store;

pub mod raycast;

pub mod render_info;
pub mod occluding_plane;

pub mod saveable;

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

pub mod drug;
pub mod loot;
pub mod item;
pub mod items_info;

pub mod inventory;

pub mod player;

pub mod particle_creator;
pub mod furniture_creator;
pub mod furnitures_info;

pub mod enemy;
pub mod enemy_creator;
pub mod enemies_info;

pub mod message;

pub mod sender_loop;
pub mod receiver_loop;

pub mod tilemap;

pub mod chunk_saver;
pub mod world;

pub mod spatial;
pub mod collider;
pub mod physics;

pub mod door;
pub mod joint;
pub mod light;

pub mod systems;


pub type MessageSerError = bincode::error::EncodeError;
pub type MessageDeError = bincode::error::DecodeError;

pub const BINCODE_CONFIG: bincode::config::Configuration = bincode::config::standard();

#[macro_export]
macro_rules! get_time_this
{
    ($($tt:tt)*) =>
    {
        {
            let start_time = std::time::Instant::now();

            let value = $($tt)*;

            (start_time.elapsed().as_micros() as f64 / 1000.0, value)
        }
    }
}

#[macro_export]
macro_rules! time_this
{
    ($name:literal, $($tt:tt)*) =>
    {
        {
            let (time, value) = $crate::get_time_this!($($tt)*);

            eprintln!("{} took {time:.2} ms", $name);

            value
        }
    }
}

#[macro_export]
macro_rules! debug_time_this
{
    ($name:expr, $($tt:tt)*) =>
    {
        {
            use $crate::debug_config::*;

            if DebugConfig::is_enabled(DebugTool::DebugTimings)
            {
                let (time, value) = $crate::get_time_this!($($tt)*);

                eprintln!("{} took {time:.2} ms", $name);

                value
            } else
            {
                $($tt)*
            }
        }
    }
}

#[macro_export]
macro_rules! time_this_additive
{
    ($result:expr, $($tt:tt)*) =>
    {
        {
            use $crate::debug_config::*;

            if DebugConfig::is_enabled(DebugTool::FrameTimings)
            {
                let start_time = std::time::Instant::now();

                let value = $($tt)*;

                let time = start_time.elapsed();

                $result = Some($result.map(|x| x + time).unwrap_or(time));

                value
            } else
            {
                $($tt)*
            }
        }
    }
}

#[derive(Default)]
pub struct TimingField<T>
{
    pub total: Option<f64>,
    pub times: usize,
    pub child: T
}

pub trait TimingsTrait
{
    fn display(&self, depth: usize) -> Option<String>;
}

impl TimingsTrait for ()
{
    fn display(&self, _depth: usize) -> Option<String> { None }
}

#[macro_export]
macro_rules! get_field_type
{
    () => { $crate::common::TimingField<()> };
    ($t:ty) => { $crate::common::TimingField<$t> }
}

#[macro_export]
macro_rules! define_timings
{
    ($name:ident, { $($field:ident $(is $inner_name:ident -> $field_next:tt)?),* }) =>
    {
        $(
            $(
                define_timings!{$inner_name, $field_next}
            )?
        )*

        #[derive(Default)]
        pub struct $name
        {
            $(pub $field: $crate::get_field_type!($($inner_name)?),)*
        }

        impl TimingsTrait for $name
        {
            fn display(&self, depth: usize) -> Option<String>
            {
                let mut s = String::new();

                let pad = "|".repeat(depth);

                $(
                    let child = self.$field.child.display(depth + 1);

                    let time = self.$field.total.map(|time|
                    {
                        if self.$field.times > 1
                        {
                            let times = self.$field.times;
                            let time_per_run = time / times as f64 * 1000.0;

                            format!("took {time:.2} ms total, ran {times} times ({time_per_run:.2} us per run)")
                        } else
                        {
                            format!("took {time:.2} ms")
                        }
                    }).unwrap_or_else(||
                    {
                        "is unrecorded".to_owned()
                    });

                    let this = format!("{pad}{} {time}", stringify!($field));

                    if self.$field.total.is_some() || child.is_some()
                    {
                        let this_s = if let Some(child) = child
                        {
                            format!("{this} ->\n{child}")
                        } else
                        {
                            format!("{this}\n")
                        };
                        s += &this_s;
                    }
                )*

                Some(s)
            }
        }
    }
}

define_timings!
{
    Timings,
    {
    server_update is TimingsServerUpdate -> {
        process_messages,
        update_sprites,
        create_queued
    },
    update is TimingsUpdate -> {
        update_pre is TimingsUpdatePre -> {
            lazy_transform_update,
            anatomy_system_update,
            spatial_grid_build,
            sleeping_update,
            enemy_system_update,
            lazy_mix_update,
            outlineable_update,
            physical_update,
            collider_system_update is TimingsCollisionSystemUpdate -> {
                world is TimingsCollisionSystemWorld -> {
                    flat_time,
                    z_time
                },
                collision
            },
            physical_system_apply,
            collided_entities_sync,
            damaging_system_update,
            collision_system_resolution,
            world_update is TimingsWorldUpdate -> {
                world_receiver,
                visual_overmap
            }
        },
        ui_update,
        game_state_update is TimingsGameStateUpdate -> {
            before_render_pass,
            process_messages is ProcessMessages -> {
                send_buffered,
                world_handle_message,
                render,
                light,
                occluder,
                parent,
                sibling,
                furniture,
                health,
                lazy_mix,
                outlineable,
                lazy_transform,
                follow_rotation,
                follow_position,
                watchers,
                damaging,
                inventory,
                named,
                transform,
                character,
                enemy,
                player,
                collider,
                physical,
                anatomy,
                door,
                joint,
                saveable,
                handle_message_common,
                entity_set_many,
                entity_set,
                entity_remove_many,
                entity_remove,
                entity_remove_chunk
            },
            characters_update,
            watchers_update,
            create_queued is TimingsCreateQueued -> {
                lazy_set,
                common,
                remove
            },
            sync_changed,
            handle_on_change,
            create_render_queued,
            rare
        }
    },
    update_buffers is TimingsUpdateBuffers -> {
        world_update_buffers_normal,
        world_update_buffers_shadows,
        entities_update_buffers is TimingsEntitiesUpdateBuffers -> {
            normal,
            lights
        }
    },
    draw
    }
}

#[cfg(debug_assertions)]
pub static THIS_FRAME_TIMINGS: LazyLock<Mutex<Timings>> = LazyLock::new(|| Mutex::new(Timings::default()));

pub const TARGET_FPS: u32 = 60;

#[macro_export]
macro_rules! frame_timed
{
    ([$($parent:ident),* $(,)?] -> $name:ident, $time_ms:expr) =>
    {
        #[cfg(debug_assertions)]
        {
            let time = $time_ms;
            let mut timings = $crate::common::THIS_FRAME_TIMINGS.lock();

            $(
                let timings = &mut timings.$parent.child;
            )*

            if let Some(timings) = timings.$name.total.as_mut()
            {
                *timings += time;
            } else
            {
                timings.$name.total = Some(time)
            };

            timings.$name.times += 1;
        }
    }
}

#[macro_export]
macro_rules! frame_time_this
{
    ([$($parent:ident),*] -> $name:ident, $($tt:tt)*) =>
    {
        {
            use $crate::debug_config::*;

            if DebugConfig::is_enabled(DebugTool::PrintStage)
            {
                eprintln!("currently in {}", stringify!($name));
            }

            if DebugConfig::is_enabled(DebugTool::FrameTimings)
            {
                let (time, value) = $crate::get_time_this!($($tt)*);

                $crate::frame_timed!([$($parent,)*] -> $name, time);

                value
            } else
            {
                $($tt)*
            }
        }
    }
}

pub const ENTITY_SCALE: f32 = TILE_SIZE * 0.9;
pub const ENTITY_PIXEL_SCALE: u32 = 32;

#[derive(Clone)]
pub struct DataInfos
{
    pub items_info: Arc<ItemsInfo>,
    pub enemies_info: Arc<EnemiesInfo>,
    pub furnitures_info: Arc<FurnituresInfo>,
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
}

#[derive(Debug)]
pub struct MessagePasser
{
    stream: TcpStream
}

impl MessagePasser
{
    pub fn new(stream: TcpStream) -> Self
    {
        Self{stream}
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

        bincode::serde::encode_into_std_write(messages, &mut self.stream, BINCODE_CONFIG)?;

        Ok(())
    }

    pub fn receive(&mut self) -> Result<Vec<Message>, MessageDeError>
    {
        bincode::serde::decode_from_std_read(&mut self.stream, BINCODE_CONFIG)
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
        Self{stream: self.stream.try_clone().unwrap()}
    }
}
